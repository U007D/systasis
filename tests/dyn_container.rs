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
        }
        .build();
        let value: &dyn IValue = container.resolve_i_value_dyn_ref();
        assert_eq!(value.value(), 7);
        assert_eq!(container.resolve_i_value(), 7);
    }
}
