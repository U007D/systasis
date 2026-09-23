//! Nested named descriptors preserve repeatable cloning and independent sibling state.
#![forbid(unsafe_code)]

use systasis::container::Error;

mod leaf {
    pub trait IValue {}
    impl IValue for String {}
    pub trait ISize {}
    impl ISize for usize {}
    #[systasis::container]
    pub fn run(value: String, call: impl FnOnce(&SystasisContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: String as IValue);
            register_type_with!(usize as ISize, try || -> Result<usize, systasis::container::Error> {
                Ok(resolve_clone!(IValue).len())
            });
        }.build();
        call(&container);
    }
}

mod middle {
    use super::*;
    trait ILength {}
    impl ILength for usize {}
    #[systasis::container]
    pub fn run<'a>(
        primary: &'a leaf::SystasisContainer,
        replica: &'a leaf::SystasisContainer,
        call: impl FnOnce(&SystasisContainer<'a>),
    ) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &'a leaf::SystasisContainer);
            register_container!(replica: &'a leaf::SystasisContainer);
            register_type_with!(usize as ILength, try || -> Result<usize, Error> {
                Ok(resolve_clone_from!(IValue, primary).len())
            });
        }
        .build();
        call(&container);
    }
}

mod outer {
    use super::*;
    trait IObserved {}
    impl IObserved for usize {}
    trait ICopied {}
    impl ICopied for String {}
    fn receive(scope: &branch::SubContainer<'_, '_>) -> Result<usize, Error> {
        scope.try_resolve_i_length()
    }
    #[systasis::container]
    pub fn run<'a>(branch: &middle::SystasisContainer<'a>) -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_container!(branch: &middle::SystasisContainer<'a>);
            register_value!(resolve_clone_from!(IValue, branch::replica): resolve_type_from!(IValue, branch::replica) as ICopied);
            register_type_with!(usize as IObserved, try || -> Result<usize, Error> {
                Ok(resolve_clone_from!(IValue, branch::primary).len())
            });
        }
        .build::<Error>()?;
        assert_eq!(receive(container.branch())?, 7);
        assert_eq!(container.try_resolve_i_observed()?, 7);
        let mut cloned = container.branch().primary().resolve_i_value_clone();
        assert_eq!(cloned, "primary");
        cloned.push('!');
        assert_eq!(cloned, "primary!");
        assert_eq!(receive(container.branch())?, 7);
        assert_eq!(container.try_resolve_i_observed()?, 7);
        assert_eq!(container.branch().primary().try_resolve_i_size()?, 7);
        assert_eq!(container.resolve_i_copied_clone(), "replica");
        assert_eq!(
            container.branch().replica().resolve_i_value_clone(),
            "replica"
        );
        Ok(())
    }
}

#[test]
fn nested_named_scopes_preserve_clone_access_and_independent_siblings() {
    leaf::run(String::from("primary"), |primary| {
        leaf::run(String::from("replica"), |replica| {
            middle::run(primary, replica, |branch| outer::run(branch).unwrap());
        });
    });
}
