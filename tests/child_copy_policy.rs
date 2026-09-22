//! Child projections carry declaration-site Copy policy into receiving storage.
#![forbid(unsafe_code)]
use systasis::app_container::Error;

mod child {
    pub trait IValue {}
    impl<T> IValue for T {}
    #[systasis::container]
    pub fn run<T>(value: T, call: impl FnOnce(&SystasisContainer<T>)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: T as IValue);
        }
        .build();
        call(&container);
    }
}

mod copied_child {
    pub trait IValue {}
    impl<T> IValue for T {}
    #[systasis::container]
    pub fn run<T: Copy>(value: T, call: impl FnOnce(&SystasisContainer<T>)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: T as IValue);
        }
        .build();
        call(&container);
    }
}

mod owned_parent {
    use super::*;
    trait ISelected {}
    impl<T> ISelected for T {}
    #[systasis::container(require(!Sync))]
    pub fn run<T: PartialEq>(
        child: &crate::child::SystasisContainer<T>,
        expected: T,
    ) -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_container!(child: &crate::child::SystasisContainer<T>);
            register_value!(try_resolve_from!(IValue, child)?: resolve_type_from!(IValue, child) as ISelected);
        }.build::<Error>()?;
        assert!(matches!(
            child.try_resolve_i_value(),
            Err(Error::ValueAlreadyConsumed)
        ));
        {
            let guard: core::cell::Ref<'_, T> = container.try_resolve_i_selected_ref()?;
            assert!(*guard == expected);
            assert!(matches!(
                container.try_resolve_i_selected(),
                Err(Error::ValueAccessContention)
            ));
        }
        assert!(container.try_resolve_i_selected()? == expected);
        assert!(matches!(
            container.try_resolve_i_selected(),
            Err(Error::ValueAlreadyConsumed)
        ));
        Ok(())
    }
}

mod copied_parent {
    use super::*;
    trait ISelected {}
    impl<T> ISelected for T {}
    #[systasis::container(require(!Sync))]
    pub fn run<T: Copy + PartialEq>(child: &copied_child::SystasisContainer<T>, expected: T) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(child: &copied_child::SystasisContainer<T>);
            register_value!(resolve_from!(IValue, child): resolve_type_from!(IValue, child) as ISelected);
        }.build();
        let plain: &T = container.resolve_i_selected_ref();
        assert!(*plain == expected);
        assert!(container.resolve_i_selected() == expected);
        assert!(container.resolve_i_selected() == expected);
        assert!(child.resolve_i_value() == expected);
    }
}

mod explicit_parent {
    use super::*;
    trait ISelected {}
    impl<T> ISelected for T {}
    #[systasis::container]
    pub fn run<T: Copy + PartialEq>(
        child: &crate::child::SystasisContainer<T>,
        expected: T,
    ) -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_container!(child: &crate::child::SystasisContainer<T>);
            register_value!(try_resolve_from!(IValue, child)?: T as ISelected);
        }
        .build::<Error>()?;
        assert!(container.resolve_i_selected() == expected);
        assert!(container.resolve_i_selected() == expected);
        Ok(())
    }
}

#[test]
fn unbounded_copy_instantiation_stays_owned_and_parent_uses_local_storage() {
    child::run(23_u32, |value| owned_parent::run(value, 23).unwrap());
    child::run(String::from("owned"), |value| {
        owned_parent::run(value, String::from("owned")).unwrap()
    });
}

#[test]
fn bounded_copy_projection_stays_plain_in_local_parent() {
    copied_child::run(31_u32, |value| copied_parent::run(value, 31));
}

#[test]
fn explicit_parent_bound_takes_precedence() {
    child::run(47_u32, |value| explicit_parent::run(value, 47).unwrap());
}
