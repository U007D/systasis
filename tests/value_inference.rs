//! A value can reuse its declared type without repeating it at registration.
//! Documentation continues to show explicit registration annotations.
#![forbid(unsafe_code)]

use systasis::app_container::Error;

trait IText {}
impl IText for String {}
trait ICount {}
impl ICount for usize {}

mod annotated_expression {
    use super::Error;
    use std::path::PathBuf;

    trait IConfigPath {}
    impl IConfigPath for PathBuf {}

    #[systasis::container]
    #[test]
    fn explicit_expression_type_builds_and_resolves() -> Result<(), Error> {
        let Ok(container) = systasis::systasis_container! {
            register_value!(PathBuf::from("/etc/app"): PathBuf as IConfigPath);
        }
        .build();

        let path: PathBuf = container.try_resolve_i_config_path()?;
        assert_eq!(path, PathBuf::from("/etc/app"));
        assert_eq!(
            container.try_resolve_i_config_path(),
            Err(Error::ValueAlreadyConsumed)
        );
        Ok(())
    }
}

mod locals {
    use super::*;

    fn count(container: &AppContainer) -> usize {
        container.resolve_i_count()
    }

    #[systasis::container(require(Send, Sync))]
    #[test]
    fn typed_locals_keep_their_normal_owned_and_copy_accessors() -> Result<(), Error> {
        let text: String = String::from("value");
        let size: usize = text.len();
        let Ok(container) = systasis::systasis_container! {
            register_value!(text as IText);
            register_value!(size as ICount);
        }
        .build();

        assert_eq!(count(&container), 5);
        assert_eq!(count(&container), 5);
        assert_eq!(&*container.try_resolve_i_text_ref()?, "value");
        assert_eq!(container.try_resolve_i_text()?, "value");
        assert_eq!(
            container.try_resolve_i_text(),
            Err(Error::ValueAlreadyConsumed)
        );
        Ok(())
    }
}

mod generic_consumable {
    use super::Error;
    trait IValue {}
    impl<T> IValue for T {}

    #[systasis::container]
    fn check<T: core::fmt::Debug + PartialEq>(value: T, expected: T) -> Result<(), Error> {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value as IValue);
        }
        .build();
        let _: &AppContainer<T> = &container;
        assert_eq!(container.try_resolve_i_value()?, expected);
        assert_eq!(
            container.try_resolve_i_value(),
            Err(Error::ValueAlreadyConsumed)
        );
        Ok(())
    }

    #[test]
    fn unbounded_parameter_stays_consumable_at_every_instantiation() -> Result<(), Error> {
        check(String::from("owned"), String::from("owned"))?;
        check(7_u32, 7)
    }
}

mod generic_copy {
    trait IValue {}
    impl<T> IValue for T {}

    #[systasis::container]
    fn check<T: Copy + core::fmt::Debug + PartialEq>(value: T) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value as IValue);
        }
        .build();
        let _: &AppContainer<T> = &container;
        assert_eq!(container.resolve_i_value(), value);
        assert_eq!(container.resolve_i_value(), value);
    }

    #[test]
    fn explicit_copy_parameter_keeps_copy_accessors() {
        check(7_u32);
    }
}

mod final_override {
    use super::*;

    #[systasis::container]
    #[test]
    fn discarded_value_does_not_need_a_type() {
        let text: String = String::from("winner");
        let Ok(container) = systasis::systasis_container! {
            register_value!(not_defined as IText);
            register_value!(text as IText);
        }
        .build();
        assert_eq!(container.try_resolve_i_text().unwrap(), "winner");
    }
}

mod group {
    use super::*;
    trait ILength {
        fn length(&self) -> usize;
    }
    impl ILength for String {
        fn length(&self) -> usize {
            self.len()
        }
    }

    #[systasis::container]
    #[test]
    fn named_dyn_group_and_explicit_cast_keep_their_resolvers() -> Result<(), Error> {
        let text: String = String::from("group");
        let Ok(container) = systasis::systasis_container! {
            register_value!(text as dyn IText + ILength in test);
            register_value!(5_u8 as usize as ICount);
        }
        .build();
        assert_eq!(
            container
                .try_resolve_i_length_i_text_dyn_ref_in_test()?
                .length(),
            5
        );
        assert_eq!(container.try_resolve_i_length_i_text_in_test()?, "group");
        assert_eq!(container.resolve_i_count(), 5);
        Ok(())
    }
}
