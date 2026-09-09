//! Dyn type lookup must allow a shorter borrow of an intermediate container.
#![forbid(unsafe_code)]
use systasis::app_container::Error;

mod leaf {
    pub trait IValue {
        fn value(&self) -> u32;
    }
    pub struct Value(pub u32);
    impl IValue for Value {
        fn value(&self) -> u32 {
            self.0
        }
    }
    #[systasis::container]
    pub fn run(call: impl FnOnce(&AppContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(Value(42): Value as dyn IValue);
        }
        .build();
        call(container);
    }
}

mod middle {
    use super::*;
    #[systasis::container]
    pub fn run<'a>(primary: &'a leaf::AppContainer, call: impl FnOnce(&AppContainer<'a>)) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &'a leaf::AppContainer);
        }
        .build();
        call(container);
    }
}

mod outer {
    use super::*;
    trait IObserved {}
    impl IObserved for u32 {}
    #[systasis::container]
    pub fn run<'a>(branch: &middle::AppContainer<'a>) -> Result<u32, Error> {
        let container = systasis::systasis_container! {
            register_container!(branch: &middle::AppContainer<'a>);
            register_value!({
                let guard = try_resolve_dyn_ref_from!(IValue, branch::primary)?;
                let value: &resolve_type_from!(dyn IValue, branch::primary) = &*guard;
                value.value()
            }: u32 as IObserved);
        }
        .build::<Error>()?;
        Ok(container.resolve_i_observed())
    }
}

#[test]
fn nested_dyn_target_can_borrow_through_a_short_lived_middle() {
    leaf::run(|leaf| middle::run(leaf, |middle| assert_eq!(outer::run(middle).unwrap(), 42)));
}

mod borrowed_payload {
    use super::Error;
    mod leaf {
        pub trait IValue {
            type Number;
            fn value(&self) -> Self::Number;
        }
        pub struct Value<'a>(pub &'a u32);
        impl IValue for Value<'_> {
            type Number = u32;
            fn value(&self) -> u32 {
                *self.0
            }
        }
        #[systasis::container(require(!Sync))]
        pub fn run<'data>(input: &'data u32, call: impl FnOnce(&AppContainer<'data>)) {
            let Ok(container) = systasis::systasis_container! {
                register_value!(Value(input): Value<'data> as dyn IValue<Number = u32>);
            }
            .build();
            call(container);
        }
    }
    mod middle {
        use super::*;
        #[systasis::container(require(!Sync))]
        pub fn run<'a, 'data>(
            primary: &'a leaf::AppContainer<'data>,
            call: impl FnOnce(&AppContainer<'a, 'data>),
        ) {
            let Ok(container) = systasis::systasis_container! {
                register_container!(primary: &'a leaf::AppContainer<'data>);
            }
            .build();
            call(container);
        }
    }
    mod outer {
        use super::*;
        trait IObserved {}
        impl IObserved for u32 {}
        fn ordinary_rust_control<'a, 'data>(branch: &middle::AppContainer<'a, 'data>) {
            fn needs_relationship<'a, 'data: 'a>(_: &middle::AppContainer<'a, 'data>) {}
            needs_relationship(branch);
        }
        #[systasis::container(require(!Sync))]
        pub fn run<'a, 'data>(branch: &middle::AppContainer<'a, 'data>) -> Result<u32, Error> {
            ordinary_rust_control(branch);
            let container = systasis::systasis_container! {
                register_container!(branch: &middle::AppContainer<'a, 'data>);
                register_value!({
                    let guard = try_resolve_dyn_ref_from!(IValue<Number = u32>, branch::primary)?;
                    let value: &resolve_type_from!(dyn IValue<Number = u32>, branch::primary) = &*guard;
                    value.value()
                }: u32 as IObserved);
            }.build::<Error>()?;
            Ok(container.resolve_i_observed())
        }
    }
    #[test]
    fn nonstatic_payload_and_associated_binding_survive_local_nested_lookup() {
        let number = 17;
        leaf::run(&number, |leaf| {
            middle::run(leaf, |middle| assert_eq!(outer::run(middle).unwrap(), 17))
        });
    }
}
