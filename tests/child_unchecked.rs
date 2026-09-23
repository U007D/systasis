//! Unchecked child forwarding transfers values once across nested scopes.
#![cfg(feature = "resolve_unchecked")]
#![cfg_attr(miri, feature(never_type))]

use systasis::container::Error;

mod leaf {
    pub trait IValue {}
    pub struct Value(pub String);
    impl IValue for Value {}
    #[systasis::container]
    pub fn run(call: impl FnOnce(&SystasisContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(Value(String::from("value")): Value as IValue);
            register_value!(Value(String::from("metric")): Value as IValue in metrics);
        }
        .build();
        call(&container);
    }
}

mod branch {
    use super::*;
    #[systasis::container]
    pub fn run<'a>(
        primary: &'a leaf::SystasisContainer,
        call: impl FnOnce(&SystasisContainer<'a>),
    ) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &'a leaf::SystasisContainer);
        }
        .build();
        call(&container);
    }
}

mod outer {
    use super::*;
    trait ILength {}
    impl ILength for usize {}
    #[systasis::container]
    pub fn run<'a>(branch: &crate::branch::SystasisContainer<'a>) -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_container!(branch: &crate::branch::SystasisContainer<'a>);
            register_value!({
                // SAFETY: the freshly built child value is present, and this
                // single-threaded initialization has no competing acquisition.
                let mut owned = unsafe { resolve_unchecked_from!(IValue, branch::primary) };
                owned.0.push('!');
                assert_eq!(owned.0, "value!");
                // SAFETY: this independent namespace value has not been accessed.
                let metric = unsafe { resolve_unchecked_from!(IValue, branch::primary::metrics) };
                assert_eq!(metric.0, "metric");
                owned.0.len()
            }: usize as ILength);
        }
        .build::<Error>()?;
        assert_eq!(container.resolve_i_length(), 6);
        assert!(matches!(
            container.branch().primary().try_resolve_i_value(),
            Err(Error::ValueAlreadyConsumed)
        ));
        assert!(matches!(
            container
                .branch()
                .primary()
                .try_resolve_i_value_in_metrics(),
            Err(Error::ValueAlreadyConsumed)
        ));
        Ok(())
    }
}

#[test]
fn nested_unchecked_queries_consume_only_the_selected_registrations() {
    leaf::run(|primary| {
        branch::run(primary, |branch| outer::run(branch).unwrap());
    });
}
