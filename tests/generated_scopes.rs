//! Restricted descriptors preserve existing methods before child composition.
#![forbid(unsafe_code)]

use systasis::{
    container::Error,
    scoped::{
        AsScope, BorrowContext,
        mask::{Empty, Mask},
    },
};

trait IValue {}
impl IValue for String {}

#[systasis::container]
#[test]
fn descriptor_clones_without_exposing_backing_storage() {
    let value: String = "scoped".into();
    let Ok(container) = systasis::systasis_container! {
        register_value!(value: String as IValue);
    }
    .build();
    let mut value = {
        let scope = AsScope::<Empty>::scope(&container);
        scope.resolve_i_value_clone()
    };
    assert_eq!(value, "scoped");
    value.push('!');
    assert_eq!(value, "scoped!");
    let scope = AsScope::<Empty>::scope(&container);
    let Ok(clone) = scope.try_resolve_i_value_clone();
    assert_eq!(clone, "scoped");
}

mod restricted {
    use super::*;
    #[systasis::container]
    #[test]
    fn restricted_scope_keeps_nonconsuming_clone_access() {
        let value: String = "reserved-name-only".into();
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: String as IValue);
        }
        .build();
        type Restrictions = Mask<__systasis_injected::__systasis_RestrictionKey0, Empty>;
        let scope = AsScope::<Restrictions>::scope(&container);
        let mut value = scope.resolve_i_value_clone();
        value.push('!');
        assert_eq!(value, "reserved-name-only!");
        let Ok(clone) = scope.try_resolve_i_value_clone();
        assert_eq!(clone, "reserved-name-only");
    }
}

mod factory {
    use super::*;
    struct Value(String);
    impl IValue for Value {}
    #[systasis::container]
    #[test]
    fn private_factory_output_remains_repeatable_through_scope() {
        let text: String = "factory".into();
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(Value as IValue, move || Value(text.clone()));
        }
        .build();
        let scope = AsScope::<Empty>::scope(&container);
        assert_eq!(scope.resolve_i_value().0, "factory");
        assert_eq!(scope.resolve_i_value().0, "factory");
    }
}

mod borrowed_context {
    use super::*;
    struct Value(String);
    impl IValue for Value {}

    #[systasis::container]
    #[test]
    fn transferred_value_outlives_both_context_and_temporary_descriptor() -> Result<(), Error> {
        let Ok(container) = systasis::systasis_container! {
            register_value!(Value(String::from("context")): Value as IValue);
        }
        .build();
        let mut value = {
            let scope = AsScope::<Empty>::scope(&container);
            let context = scope.borrow_context();
            context.descriptor().try_resolve_i_value()?
        };
        assert_eq!(value.0, "context");
        assert!(matches!(
            container.try_resolve_i_value(),
            Err(Error::ValueAlreadyConsumed)
        ));
        value.0.push('!');
        assert_eq!(value.0, "context!");
        Ok(())
    }
}
