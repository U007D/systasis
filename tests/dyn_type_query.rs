//! Dyn target queries select the opted-in trait without changing static lookup.
#![forbid(unsafe_code)]

use systasis::container::Error;

macro_rules! scenario {
    ($module:ident, ($($requirements:tt)*)) => {
        mod $module {
            use super::*;
            trait ILogger { fn text(&self) -> &str; }
            impl ILogger for String { fn text(&self) -> &str { self } }
            trait ILength {}
            impl ILength for usize {}
            trait IReader {}
            struct Reader<T>(T);
            impl<T> IReader for Reader<T> {}

            #[systasis::container($($requirements)*)]
            #[test]
            fn dyn_target_in_initializer_and_constructor() -> Result<(), Error> {
                let container = systasis::systasis_container! {
                    register_value!({
                        let concrete: resolve_type!(ILogger) = String::from("concrete");
                        let owned = resolve_clone!(ILogger);
                        let reader: &resolve_type!(dyn ILogger) = &owned;
                        reader.text().len() + concrete.len()
                    }: usize as ILength);
                    register_type_with!(Reader<resolve_type!(ILogger)> as IReader,
                        try || -> Result<Reader<resolve_type!(ILogger)>, Error> {
                            let owned = resolve_clone!(ILogger);
                            let reader: &resolve_type!(dyn ILogger) = &owned;
                            let _length = reader.text().len();
                            Ok(Reader(owned))
                        });
                    register_value!(String::from("message"): String as dyn ILogger);
                }.build::<Error>()?;
                assert_eq!(container.resolve_i_length(), 15);
                let mut reader = container.try_resolve_i_reader()?;
                assert_eq!(reader.0.text(), "message");
                reader.0.push('!');
                assert_eq!(reader.0.text(), "message!");
                assert_eq!(container.try_resolve_i_reader()?.0.text(), "message");
                Ok(())
            }
        }
    };
}

scenario!(synchronized, (require(Send, Sync)));
scenario!(local, (require(!Sync)));

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
                let copied = resolve!(ISource<Item = u32>);
                let reader: &resolve_type!(dyn ISource<Item = u32>) = &copied;
                reader.item() as usize
            }: usize as ILength);
        }
        .build();
        assert_eq!(container.resolve_i_length(), 42);
    }
}
