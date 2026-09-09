//! Explicit trait-object access leaves concrete storage and static access intact.
#![forbid(unsafe_code)]

use systasis::app_container::Error;

macro_rules! scenario {
    ($module:ident, ($($requirements:tt)*)) => {
        mod $module {
            use super::*;
            trait ILogger { fn text(&self) -> &str; }
            struct Logger(String);
            impl ILogger for Logger { fn text(&self) -> &str { &self.0 } }
            #[systasis::container($($requirements)*)]
            #[test]
            fn explicit_dyn_reader_retains_occupancy_and_borrow_checks() -> Result<(), Error> {
                let Ok(container) = systasis::systasis_container! {
                    register_value!(Logger(String::from("message")): Logger as dyn ILogger);
                }.build();
                let reader = container.try_resolve_i_logger_dyn_ref()?;
                assert_eq!(reader.text(), "message");
                assert!(matches!(container.try_resolve_i_logger(), Err(Error::ValueAccessContention)));
                drop(reader);
                let concrete: Logger = container.try_resolve_i_logger()?;
                assert_eq!(concrete.text(), "message");
                assert!(matches!(container.try_resolve_i_logger_dyn_ref(), Err(Error::ValueAlreadyConsumed)));
                Ok(())
            }
        }
    };
}
scenario!(synchronized, (require(Send, Sync)));
scenario!(local, (require(!Sync)));

macro_rules! queries {
    ($module:ident, ($($requirements:tt)*)) => {
        mod $module {
            use super::*;
            trait ILogger { fn text(&self) -> &str; }
            impl ILogger for String { fn text(&self) -> &str { self } }
            trait ILength {}
            impl ILength for usize {}
            trait IAgain {}
            impl IAgain for usize {}

            #[systasis::container($($requirements)*)]
            #[test]
            fn dyn_queries_work_during_build_and_repeated_construction() -> Result<(), Error> {
                let container = systasis::systasis_container! {
                    register_value!(try_resolve_dyn_ref!(ILogger)?.text().len(): usize as ILength);
                    register_type_with!(usize as IAgain, try || -> Result<usize, Error> {
                        Ok(try_resolve_dyn_ref!(ILogger)?.text().len())
                    });
                    register_value!(String::from("message"): String as dyn ILogger);
                }.build::<Error>()?;
                assert_eq!(container.resolve_i_length(), 7);
                assert_eq!(container.try_resolve_i_again()?, 7);
                container.try_resolve_i_logger_ref_mut()?.push('!');
                assert_eq!(container.try_resolve_i_again()?, 8);
                Ok(())
            }
        }
    };
}
queries!(synchronized_queries, (require(Send, Sync)));
queries!(local_queries, (require(!Sync)));

mod copy {
    trait IValue {
        fn value(&self) -> u32;
    }
    impl IValue for u32 {
        fn value(&self) -> u32 {
            *self
        }
    }
    #[systasis::container]
    #[test]
    fn copy_dyn_reference_has_no_guard() {
        let Ok(container) = systasis::systasis_container! {
            register_value!(7_u32: u32 as dyn IValue);
            register_value!(resolve_dyn_ref!(IValue).value() as usize: usize as ILength);
        }
        .build();
        let value: &dyn IValue = container.resolve_i_value_dyn_ref();
        assert_eq!(value.value(), 7);
        assert_eq!(container.resolve_i_value(), 7);
        assert_eq!(container.resolve_i_length(), 7);
    }
    trait ILength {}
    impl ILength for usize {}
}
