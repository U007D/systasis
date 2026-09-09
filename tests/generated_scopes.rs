//! Restricted descriptors preserve existing methods before child composition.
#![forbid(unsafe_code)]

use systasis::{
    app_container::Error,
    scoped::{
        AsScope,
        mask::{Empty, Mask},
    },
};

trait IValue {}
impl IValue for String {}

#[systasis::container]
#[test]
fn descriptor_retains_guards_without_exposing_backing_storage() -> Result<(), Error> {
    let value: String = "scoped".into();
    let Ok(container) = systasis::systasis_container! {
        register_value!(value: String as IValue);
    }
    .build();
    let guard = {
        let scope = AsScope::<Empty>::scope(container);
        scope.try_resolve_i_value_ref()?
    };
    assert_eq!(&*guard, "scoped");
    assert!(matches!(
        container.try_resolve_i_value(),
        Err(Error::ValueAccessContention)
    ));
    drop(guard);
    let scope = AsScope::<Empty>::scope(container);
    assert_eq!(scope.try_resolve_i_value()?, "scoped");
    Ok(())
}

mod restricted {
    use super::*;
    #[systasis::container]
    #[test]
    fn restricted_scope_keeps_shared_clone_and_mutable_access() -> Result<(), Error> {
        let value: String = "reserved-name-only".into();
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: String as IValue);
        }
        .build();
        type Restrictions = Mask<__systasis_injected::__SystasisRestrictionKey0, Empty>;
        let scope = AsScope::<Restrictions>::scope(container);
        scope.try_resolve_i_value_ref_mut()?.push('!');
        assert_eq!(&*scope.try_resolve_i_value_ref()?, "reserved-name-only!");
        assert_eq!(scope.try_resolve_i_value_clone()?, "reserved-name-only!");
        Ok(())
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
        let scope = AsScope::<Empty>::scope(container);
        assert_eq!(scope.resolve_i_value().0, "factory");
        assert_eq!(scope.resolve_i_value().0, "factory");
    }
}
