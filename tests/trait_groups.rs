//! Complete trait groups share one value and one order-independent identity.
#![forbid(unsafe_code)]

use systasis::container::Error;

trait IReader {}
trait IWriter {}
trait IOutput {}
impl IReader for String {}
impl IWriter for String {}
impl IReader for u32 {}
impl IWriter for u32 {}
impl IOutput for String {}

mod owned {
    use super::*;
    #[systasis::container]
    #[test]
    fn complete_group_queries_use_one_owned_slot() -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_value!(try_resolve!(IWriter + IReader)?: registered_type!(IReader + IWriter) as IOutput);
            register_value!(String::from("group"): String as IWriter + IReader);
        }.build::<Error>()?;
        assert_eq!(container.try_resolve_i_output()?, "group");
        assert!(matches!(
            container.try_resolve_i_reader_i_writer(),
            Err(Error::ValueAlreadyConsumed)
        ));
        Ok(())
    }
}

mod distinct {
    use super::*;
    #[systasis::container(require(!Sync))]
    #[test]
    fn groups_and_individual_members_are_independent() -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_value!(String::from("group"): String as IWriter + IReader);
            register_value!(String::from("individual"): String as IReader);
        }
        .build::<Error>()?;
        {
            let mut group = container.try_resolve_i_reader_i_writer_ref_mut()?;
            group.push('!');
            assert!(matches!(
                container.try_resolve_i_reader_i_writer_ref(),
                Err(Error::ValueAccessContention)
            ));
        }
        assert_eq!(&*container.try_resolve_i_reader_i_writer_ref()?, "group!");
        assert_eq!(container.try_resolve_i_reader()?, "individual");
        Ok(())
    }
}

mod copy {
    use super::*;
    #[systasis::container]
    #[test]
    fn copy_group_has_alphabetized_copy_accessors() {
        let Ok(container) = systasis::systasis_container! {
            register_value!(17: u32 as IWriter + IReader);
        }
        .build();
        assert_eq!(container.resolve_i_reader_i_writer(), 17);
        assert_eq!(*container.resolve_i_reader_i_writer_ref(), 17);
    }
}

mod fresh {
    use super::*;
    #[systasis::container]
    #[test]
    fn defaults_support_complete_groups() {
        let Ok(container) = systasis::systasis_container! {
            register_type!(String as IWriter + IReader);
        }
        .build();
        assert_eq!(container.resolve_i_reader_i_writer(), "");
        assert_eq!(container.resolve_i_reader_i_writer(), "");
    }
}

mod constructors {
    use super::*;
    #[systasis::container]
    #[test]
    fn custom_group_queries_and_overrides_are_normalized() -> Result<(), Error> {
        let config: String = String::from("fresh");
        let container = systasis::systasis_container! {
            register_type_with!(MissingType as IWriter + IReader, || missing_macro!());
            register_value!(resolve!(IWriter + IReader): resolve_type!(IReader + IWriter) as IOutput);
            register_type_with!(String as IReader + IWriter, move || config.clone());
        }.build::<Error>()?;
        assert_eq!(container.try_resolve_i_output()?, "fresh");
        assert_eq!(container.resolve_i_reader_i_writer(), "fresh");
        assert_eq!(container.resolve_i_reader_i_writer(), "fresh");
        Ok(())
    }
}

mod overrides {
    use super::*;
    #[systasis::container]
    #[test]
    fn reordered_group_override_discards_initializer_and_invalid_dependency() {
        let Ok(container) = systasis::systasis_container! {
            register_value!(try_resolve!(IMissing)?: MissingType as IWriter + IReader);
            register_value!(24: u32 as IReader + IWriter);
        }
        .build();
        assert_eq!(container.resolve_i_reader_i_writer(), 24);
    }
}

mod constructor_dependency {
    use super::*;
    #[systasis::container]
    #[test]
    fn borrowed_constructor_queries_use_the_complete_group() -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_type_with!(String as IOutput, try || -> Result<String, Error> {
                Ok(try_resolve_ref!(IWriter + IReader)?.clone())
            });
            register_value!(String::from("borrowed"): String as IReader + IWriter);
        }
        .build::<Error>()?;
        assert_eq!(container.try_resolve_i_output()?, "borrowed");
        assert_eq!(container.try_resolve_i_output()?, "borrowed");
        container.try_resolve_i_reader_i_writer_ref_mut()?.push('!');
        assert_eq!(container.try_resolve_i_output()?, "borrowed!");
        Ok(())
    }
}
