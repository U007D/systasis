//! Declaration placement, construction timing and inferred initialization errors.
#![forbid(unsafe_code)]

mod value {
    use systasis::systasis_container;

    systasis_container! {
        register_value!(42: u8 as Copy);
    }

    pub fn init_container() -> SystasisContainer {
        let Ok(container) = SystasisContainer::build();
        container
    }
}

#[test]
fn public_named_container_leaves_build_function() {
    let container: value::SystasisContainer = value::init_container();
    assert_eq!(container.resolve_copy(), 42);
}

#[test]
fn declaration_inside_block() {
    systasis::systasis_container! {
        register_value!(7: u8 as Copy);
    }
    let Ok(container) = SystasisContainer::build();
    assert_eq!(container.resolve_copy(), 7);
}

#[test]
fn declaration_uses_types_defined_in_its_block() {
    struct Value(u8);
    trait IValue {}
    impl IValue for Value {}
    systasis::systasis_container! {
        register_value!(Value(42): Value as IValue);
    }
    let Ok(container) = SystasisContainer::build();
    assert_eq!(container.try_resolve_i_value().unwrap().0, 42);
}

#[test]
fn fallible_declaration_inside_block() {
    systasis::systasis_container! {
        register_value!("invalid".parse::<u8>()?: u8 as Copy);
    }
    fn init() -> Result<SystasisContainer, SystasisContainerError> {
        SystasisContainer::build()
    }
    let Err(error) = init() else {
        panic!("parse fails")
    };
    assert_eq!(error.to_string(), "invalid digit found in string");
}

mod lifecycle {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static INITIALIZATIONS: AtomicUsize = AtomicUsize::new(0);
    static CONSTRUCTIONS: AtomicUsize = AtomicUsize::new(0);
    trait IValue {}
    trait IFresh {}
    impl IValue for usize {}
    impl IFresh for usize {}

    systasis::systasis_container! {
        register_value!(INITIALIZATIONS.fetch_add(1, Ordering::Relaxed): usize as IValue);
        register_type_with!(usize as IFresh, move || CONSTRUCTIONS.fetch_add(1, Ordering::Relaxed));
    }

    #[test]
    fn only_build_initializes_and_only_resolution_runs_constructor() {
        assert_eq!(INITIALIZATIONS.load(Ordering::Relaxed), 0);
        assert_eq!(CONSTRUCTIONS.load(Ordering::Relaxed), 0);
        let Ok(first) = SystasisContainer::build();
        let Ok(second) = SystasisContainer::build();
        assert_eq!(first.resolve_i_value(), 0);
        assert_eq!(second.resolve_i_value(), 1);
        assert_eq!(CONSTRUCTIONS.load(Ordering::Relaxed), 0);
        assert_eq!(first.resolve_i_fresh(), 0);
        assert_eq!(first.resolve_i_fresh(), 1);
        assert_eq!(second.resolve_i_fresh(), 2);
    }
}

mod dependencies {
    trait IValue {}
    trait IFresh {}
    impl IValue for u8 {}
    impl IFresh for u8 {}
    systasis::systasis_container! {
        register_type_with!(u8 as IFresh, move || resolve!(IValue) + 1);
        register_value!(42: u8 as IValue);
    }

    #[test]
    fn constructor_keeps_container_resolution() {
        let Ok(container) = SystasisContainer::build();
        assert_eq!(container.resolve_i_fresh(), 43);
    }
}

mod fallible {
    systasis::systasis_container! {
        register_value!("invalid".parse::<u8>()?: u8 as Copy);
    }

    fn init() -> Result<SystasisContainer, SystasisContainerError> {
        SystasisContainer::build()
    }

    #[test]
    fn inferred_error_retains_source_and_auto_traits() {
        let Err(error) = init() else {
            panic!("parse fails");
        };
        fn transferable<T: Send + Sync>(_: &T) {}
        transferable(&error);
        assert_eq!(error.to_string(), "invalid digit found in string");
        assert!(
            core::error::Error::source(&error)
                .unwrap()
                .is::<core::num::ParseIntError>()
        );
    }
}

mod cleanup {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static CREATED: AtomicUsize = AtomicUsize::new(0);
    static DROPPED: AtomicUsize = AtomicUsize::new(0);
    static SKIPPED: AtomicUsize = AtomicUsize::new(0);
    struct Resource;
    trait IResource {}
    impl IResource for Resource {}
    impl Resource {
        fn new() -> Self {
            CREATED.fetch_add(1, Ordering::Relaxed);
            Self
        }
    }
    impl Drop for Resource {
        fn drop(&mut self) {
            DROPPED.fetch_add(1, Ordering::Relaxed);
        }
    }
    systasis::systasis_container! {
        register_value!(Resource::new(): Resource as IResource);
        register_value!("invalid".parse::<u8>()?: u8 as Copy);
        register_value!(SKIPPED.fetch_add(1, Ordering::Relaxed): usize as Clone);
    }

    #[test]
    fn failure_drops_prior_values_and_skips_later_values() {
        assert_eq!(CREATED.load(Ordering::Relaxed), 0);
        for expected in 1..=2 {
            assert!(SystasisContainer::build().is_err());
            assert_eq!(CREATED.load(Ordering::Relaxed), expected);
            assert_eq!(DROPPED.load(Ordering::Relaxed), expected);
            assert_eq!(SKIPPED.load(Ordering::Relaxed), 0);
        }
    }
}

#[allow(
    clippy::needless_question_mark,
    reason = "exercise propagation inside nested and lazy constructors"
)]
mod handled_error {
    trait IValue {}
    trait IFresh {}
    impl IValue for u8 {}
    impl IFresh for Result<u8, core::num::ParseIntError> {}
    systasis::systasis_container! {
        register_value!((|| {
            Ok::<_, core::num::ParseIntError>("bad".parse::<u8>()?)
        })().unwrap_or(42): u8 as IValue);
        register_type_with!(Result<u8, core::num::ParseIntError> as IFresh, move || {
            Ok("bad".parse::<u8>()?)
        });
    }

    #[test]
    fn locally_handled_and_lazy_errors_do_not_make_build_fallible() {
        let Ok(container) = SystasisContainer::build();
        assert_eq!(container.resolve_i_value(), 42);
        assert!(container.resolve_i_fresh().is_err());
    }
}

mod repeated_source {
    trait ISecond {}
    impl ISecond for u8 {}
    systasis::systasis_container! {
        register_value!("42".parse::<u8>()?: u8 as Copy);
        register_value!("7".parse::<u8>()?: u8 as ISecond);
    }

    #[test]
    fn same_error_across_registrations_is_inferred() {
        let container = SystasisContainer::build().unwrap();
        assert_eq!(container.resolve_copy(), 42);
        assert_eq!(container.resolve_i_second(), 7);
    }
}

mod explicit_return {
    systasis::systasis_container! {
        register_value!({
            if let Err(error) = "bad".parse::<u8>() {
                return Err(error);
            }
            42
        }: u8 as Copy);
    }

    #[test]
    fn returned_error_is_inferred_without_question_mark() {
        let Err(error) = SystasisContainer::build() else {
            panic!("parse fails")
        };
        assert!(
            core::error::Error::source(&error)
                .unwrap()
                .is::<core::num::ParseIntError>()
        );
    }
}

mod different_sources {
    use core::sync::atomic::{AtomicU8, Ordering};
    static FAILURE: AtomicU8 = AtomicU8::new(0);
    trait IFlag {}
    impl IFlag for bool {}
    systasis::systasis_container! {
        register_value!(
            (if FAILURE.load(Ordering::Relaxed) == 1 { "bad" } else { "42" }).parse::<u8>()?: u8 as Copy
        );
        register_value!(
            (if FAILURE.load(Ordering::Relaxed) == 2 { "bad" } else { "true" }).parse::<bool>()?: bool as IFlag
        );
    }

    #[test]
    fn all_source_errors_and_success_are_inferred() {
        let container = SystasisContainer::build().unwrap();
        assert_eq!(container.resolve_copy(), 42);
        assert!(container.resolve_i_flag());
        for mode in [1, 2] {
            FAILURE.store(mode, Ordering::Relaxed);
            let Err(error): Result<SystasisContainer, SystasisContainerError> =
                SystasisContainer::build()
            else {
                panic!("selected input must fail");
            };
            fn send_sync<T: Send + Sync>(_: &T) {}
            send_sync(&error);
            let source = core::error::Error::source(&error).unwrap();
            if mode == 1 {
                assert!(source.is::<core::num::ParseIntError>());
            } else {
                assert!(source.is::<core::str::ParseBoolError>());
            }
            assert_eq!(error.to_string(), source.to_string());
        }
    }
}

mod several_sources_in_one_registration {
    systasis::systasis_container! {
        register_value!({
            let number = "42".parse::<u8>()?;
            let enabled = "true".parse::<bool>()?;
            if enabled { number } else { 0 }
        }: u8 as Copy);
    }

    #[test]
    fn a_registration_can_propagate_different_error_types() {
        assert_eq!(SystasisContainer::build().unwrap().resolve_copy(), 42);
    }
}

mod inferred_never_sources {
    trait IFlag {}
    impl IFlag for bool {}
    systasis::systasis_container! {
        register_value!(Ok(42)?: u8 as Copy);
        register_value!(Ok(true)?: bool as IFlag);
    }

    #[test]
    fn unconstrained_sources_remain_infallible() {
        let Ok(container) = SystasisContainer::build();
        assert_eq!(container.resolve_copy(), 42);
        assert!(container.resolve_i_flag());
    }
}

mod overridden_source {
    systasis::systasis_container! {
        register_value!(Err::<u8, ()>(())?: u8 as Copy);
        register_value!(42: u8 as Copy);
    }

    #[test]
    fn replaced_initializers_do_not_contribute_errors() {
        let Ok(container) = SystasisContainer::build();
        assert_eq!(container.resolve_copy(), 42);
    }
}

mod error_ownership {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static DROPPED: AtomicUsize = AtomicUsize::new(0);
    #[derive(Debug)]
    struct Failure;
    impl core::fmt::Display for Failure {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("owned failure")
        }
    }
    impl core::error::Error for Failure {}
    impl Drop for Failure {
        fn drop(&mut self) {
            DROPPED.fetch_add(1, Ordering::Relaxed);
        }
    }
    systasis::systasis_container! {
        register_value!(Err::<u8, Failure>(Failure)?: u8 as Copy);
    }

    #[test]
    fn generated_error_owns_and_drops_its_source_once() {
        let Err(error) = SystasisContainer::build() else {
            panic!("always fails")
        };
        assert_eq!(DROPPED.load(Ordering::Relaxed), 0);
        assert!(core::error::Error::source(&error).unwrap().is::<Failure>());
        drop(error);
        assert_eq!(DROPPED.load(Ordering::Relaxed), 1);
    }
}
