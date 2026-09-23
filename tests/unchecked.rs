//! Unchecked owned transfers retain occupancy tracking and caller preconditions.
#![cfg(feature = "resolve_unchecked")]

use systasis::container::Error;

trait IValue {}
struct Value(String);
impl IValue for Value {}
impl Value {
    fn len(&self) -> usize {
        self.0.len()
    }
}

#[systasis::container]
#[test]
fn generated_unchecked_transfer_marks_the_value_consumed() {
    let value: Value = Value("value".into());
    let Ok(container) = systasis::systasis_container! {
        register_value!(value: Value as IValue);
    }
    .build();
    let scope = systasis::scoped::AsScope::<systasis::scoped::mask::Empty>::scope(&container);
    // SAFETY: this thread has exclusive use of the freshly built container;
    // the value is present and no acquisition is in progress.
    let mut value = unsafe { scope.resolve_i_value_unchecked() };
    value.0.push('!');
    assert_eq!(value.0, "value!");
    assert!(matches!(
        container.try_resolve_i_value(),
        Err(Error::ValueAlreadyConsumed)
    ));
}

mod local {
    use super::*;

    #[systasis::container(require(!Sync))]
    #[test]
    fn local_named_unchecked_transfer_marks_the_value_consumed() {
        let value: Value = Value("local".into());
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: Value as IValue in primary);
        }
        .build();
        // SAFETY: the freshly built value is present and no acquisition exists.
        assert_eq!(
            unsafe { container.resolve_i_value_unchecked_in_primary() }.0,
            "local"
        );
        assert!(matches!(
            container.try_resolve_i_value_in_primary(),
            Err(Error::ValueAlreadyConsumed)
        ));
    }
}

mod queries {
    use super::*;
    trait ILength {}
    impl ILength for usize {}

    #[systasis::container]
    #[test]
    fn unsafe_queries_preserve_caller_context_and_dependency_order() {
        let value: Value = Value("ordered".into());
        let Ok(container) = systasis::systasis_container! {
            // SAFETY: this initializer is the only accessor and the dependency
            // DAG initializes the value first.
            register_value!(unsafe { resolve_unchecked_from!(IValue, primary) }.len(): usize as ILength);
            register_value!(value: Value as IValue in primary);
        }.build();
        assert_eq!(container.resolve_i_length(), 7);
        assert!(matches!(
            container.try_resolve_i_value_in_primary(),
            Err(Error::ValueAlreadyConsumed)
        ));
    }
}

mod forbid_consumer {
    #![forbid(unsafe_code)]
    use super::*;

    #[systasis::container]
    #[test]
    fn checked_consumers_can_enable_the_feature_with_forbid_unsafe() {
        let value: Value = Value("checked".into());
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: Value as IValue);
        }
        .build();
        assert_eq!(container.try_resolve_i_value().unwrap().0, "checked");
    }
}

mod constructor_transfer {
    use super::*;
    trait ITransferred {}
    impl ITransferred for Value {}

    #[systasis::container]
    #[test]
    fn lazy_constructor_can_transfer_an_available_value_unchecked() {
        let value: Value = Value("transferred".into());
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: Value as IValue);
            register_type_with!(Value as ITransferred, || {
                // SAFETY: this test invokes the constructor exactly once, with
                // no competing access to the freshly built stored value.
                unsafe { resolve_unchecked!(IValue) }
            });
        }
        .build();
        let value = container.resolve_i_transferred();
        assert_eq!(value.0, "transferred");
        assert!(matches!(
            container.try_resolve_i_value(),
            Err(Error::ValueAlreadyConsumed)
        ));
    }
}
