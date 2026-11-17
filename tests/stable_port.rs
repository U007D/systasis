//! Stable code generation through the public macros, including related closures.
#![forbid(unsafe_code, unstable_features)]

mod factories {
    trait IFirst {}
    trait ISecond {}
    impl IFirst for u8 {}
    impl ISecond for u8 {}

    systasis::systasis_container! {
        register_type_with!(u8 as IFirst, || {
            let value = core::hint::black_box(41);
            assert!(value > 0);
            value
        });
        register_type_with!(u8 as ISecond, || {
            let first = resolve!(IFirst);
            assert_eq!(first, 41);
            first + 1
        });
    }

    fn init() -> BuildResult<impl SystasisFactories> {
        SystasisContainer::build()
    }

    #[test]
    fn related_native_factories_are_inferred_together() {
        let Ok(container) = init();
        assert_eq!(container.resolve_i_second(), 42);
        assert_eq!(container.resolve_i_second(), 42);
    }
}

mod factories_and_errors {
    trait IFresh {}
    impl IFresh for u8 {}

    systasis::systasis_container! {
        register_value!("42".parse::<u8>()?: u8 as Copy);
        register_type_with!(u8 as IFresh, || {
            let value = resolve!(Copy);
            assert_eq!(value, 42);
            value
        });
    }

    fn init() -> BuildResult<impl SystasisFactories, impl core::error::Error> {
        SystasisContainer::build()
    }

    #[test]
    fn inferred_errors_and_factories_share_one_build_result() {
        let container = init().unwrap();
        assert_eq!(container.resolve_i_fresh(), 42);
    }
}

mod infallible_sources {
    fn value() -> Result<u8, systasis::__private::Never> {
        Ok(42)
    }

    trait IFresh {}
    impl IFresh for u8 {}
    systasis::systasis_container! {
        register_value!(value()?: u8 as Copy);
        register_type_with!(u8 as IFresh, || {
            let value = resolve!(Copy);
            assert_eq!(value, 42);
            value
        });
    }

    fn init() -> BuildResult<impl SystasisFactories, impl core::error::Error> {
        SystasisContainer::build()
    }

    #[test]
    fn native_storage_preserves_irrefutable_pattern_through_returning_signature() {
        let Ok(container) = init();
        assert_eq!(container.resolve_i_fresh(), 42);
    }
}

mod local_error {
    use std::{fmt, rc::Rc};

    #[derive(Debug)]
    struct LocalError(Rc<()>);
    impl fmt::Display for LocalError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{} owners", Rc::strong_count(&self.0))
        }
    }
    impl core::error::Error for LocalError {}
    fn value() -> Result<u8, LocalError> {
        Ok(42)
    }
    trait IFresh {}
    impl IFresh for u8 {}
    systasis::systasis_container! {
        register_value!(value()?: u8 as Copy);
        register_type_with!(u8 as IFresh, || {
            let value = resolve!(Copy);
            assert_eq!(value, 42);
            value
        });
    }

    #[test]
    fn successful_native_container_does_not_inherit_error_auto_traits() {
        fn send_sync<T: Send + Sync>(_: &T) {}
        let container = SystasisContainer::build().unwrap();
        send_sync(&container);
        assert_eq!(container.resolve_i_fresh(), 42);
    }
}

mod expanded_error {
    macro_rules! value {
        () => {
            match "invalid".parse::<u8>() {
                Ok(value) => value,
                Err(error) => return Err(error),
            }
        };
    }
    systasis::systasis_container! {
        register_value!(value!(): u8 as Copy);
    }

    #[test]
    fn expanded_return_keeps_its_inferred_error() {
        let Err(error) = SystasisContainer::build() else {
            panic!("parse fails");
        };
        assert_eq!(error.to_string(), "invalid digit found in string");
    }
}

mod expanded_infallible {
    macro_rules! value {
        () => {
            42
        };
    }
    systasis::systasis_container! {
        register_value!(value!(): u8 as Copy);
    }
    #[test]
    fn macro_presence_does_not_make_a_build_fallible() {
        let Ok(container) = SystasisContainer::build();
        assert_eq!(container.resolve_copy(), 42);
    }
}
