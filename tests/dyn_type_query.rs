//! Dyn target queries select the opted-in trait without changing static lookup.
#![forbid(unsafe_code)]

use systasis::app_container::Error;

macro_rules! scenario {
    ($module:ident, ($($requirements:tt)*), $guard:path) => {
        mod $module {
            use super::*;
            use $guard as Guard;
            trait ILogger { fn text(&self) -> &str; }
            impl ILogger for String { fn text(&self) -> &str { self } }
            trait ILength {}
            impl ILength for usize {}
            trait IReader {}
            struct Reader<'a, T: ?Sized>(Guard<'a, T>);
            impl<T: ?Sized> IReader for Reader<'_, T> {}

            #[systasis::container($($requirements)*)]
            #[test]
            fn dyn_target_in_initializer_and_constructor_output() -> Result<(), Error> {
                let container = systasis::systasis_container! {
                    register_value!({
                        let concrete: resolve_type!(ILogger) = String::from("concrete");
                        let guard = try_resolve_dyn_ref!(ILogger)?;
                        let reader: &resolve_type!(dyn ILogger) = &*guard;
                        reader.text().len() + concrete.len()
                    }: usize as ILength);
                    register_type_with!(Reader<'_, resolve_type!(dyn ILogger)> as IReader,
                        try || -> Result<Reader<'_, resolve_type!(dyn ILogger)>, Error> {
                            Ok(Reader(try_resolve_dyn_ref!(ILogger)?))
                        });
                    register_value!(String::from("message"): String as dyn ILogger);
                }.build::<Error>()?;
                assert_eq!(container.resolve_i_length(), 15);
                let reader = container.try_resolve_i_reader()?;
                assert_eq!(reader.0.text(), "message");
                assert!(matches!(container.try_resolve_i_logger_ref_mut(), Err(Error::ValueAccessContention)));
                drop(reader);
                container.try_resolve_i_logger_ref_mut()?.push('!');
                assert_eq!(container.try_resolve_i_reader()?.0.text(), "message!");
                Ok(())
            }
        }
    };
}

scenario!(synchronized, (require(Send, Sync)), systasis::Ref);
scenario!(local, (require(!Sync)), core::cell::Ref);

mod associated_type {
    trait ISource {
        type Item;
        fn item(&self) -> Self::Item;
    }
    impl ISource for u32 {
        type Item = u32;
        fn item(&self) -> u32 {
            *self
        }
    }
    trait ILength {}
    impl ILength for usize {}

    #[systasis::container]
    #[test]
    fn associated_binding_is_preserved() {
        let Ok(container) = systasis::systasis_container! {
            register_value!(42_u32: u32 as dyn ISource<Item = u32>);
            register_value!({
                let reader: &resolve_type!(dyn ISource<Item = u32>) = resolve_dyn_ref!(ISource<Item = u32>);
                reader.item() as usize
            }: usize as ILength);
        }.build();
        assert_eq!(container.resolve_i_length(), 42);
    }
}
