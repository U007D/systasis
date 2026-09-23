//! Custom constructors retain typed captures and run only on resolution.
#![forbid(unsafe_code)]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use systasis::container::Error;

struct Service(String);
trait IService {}
impl IService for Service {}
struct Wrapper(Service);
trait IWrapper {}
impl IWrapper for Wrapper {}
trait IStored {}
impl IStored for Wrapper {}

#[systasis::container(require(Send, Sync))]
#[test]
fn typed_captures_and_transitive_queries_run_on_demand() -> Result<(), Error> {
    let config: String = String::from("configured");
    let counter: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
    let observed = counter.clone();
    let built = systasis::systasis_container! {
        register_type_with!(Wrapper as IWrapper, || Wrapper(resolve!(IService)));
        register_value!(resolve!(IWrapper): Wrapper as IStored);
        register_type_with!(Service as IService, move || {
            counter.fetch_add(1, Ordering::SeqCst);
            let config = config.clone();
            Service(config)
        });
    }
    .build::<Error>();
    let container = built?;
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert_eq!(container.try_resolve_i_stored()?.0.0, "configured");
    assert_eq!(container.resolve_i_wrapper().0.0, "configured");
    assert_eq!(container.resolve_i_service().0, "configured");
    assert_eq!(observed.load(Ordering::SeqCst), 3);
    Ok(())
}

mod fallible {
    use super::*;
    #[derive(Debug, PartialEq)]
    struct Failure;
    #[systasis::container]
    #[test]
    fn preserves_exact_result_return_without_making_build_fallible() {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(Service as IService, try || -> Result<Service, Failure> { Err(Failure) });
        }.build();
        let result: Result<Service, Failure> = container.try_resolve_i_service();
        assert!(matches!(result, Err(Failure)));
    }
}

mod optional {
    use super::*;
    type MaybeService = Option<Service>;
    #[systasis::container]
    #[test]
    fn preserves_option_alias() {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(Service as IService, try || -> MaybeService { None });
        }
        .build();
        let result: MaybeService = container.try_resolve_i_service();
        assert!(result.is_none());
    }
}

mod cleanup {
    use super::*;
    struct Config(Arc<AtomicUsize>);
    impl Drop for Config {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    trait IFailure {}
    impl IFailure for () {}
    #[systasis::container]
    #[test]
    fn failing_build_drops_captures_of_uninitialized_factory_immediately() {
        let observed = Arc::new(AtomicUsize::new(0));
        let config: Config = Config(observed.clone());
        let built = systasis::systasis_container! {
            register_value!(Err::<(), ()>(())?: () as IFailure);
            register_type_with!(Service as IService, move || {
                let _ = &config;
                Service(String::new())
            });
        }
        .build::<()>();
        assert!(built.is_err());
        assert_eq!(observed.load(Ordering::SeqCst), 1);
    }
}

mod build_error {
    use super::*;
    trait IStored {}
    impl IStored for Service {}
    #[systasis::container]
    #[test]
    fn resolving_a_fallible_factory_can_fail_build() {
        let built = systasis::systasis_container! {
            register_value!(try_resolve!(IService)?: Service as IStored);
            register_type_with!(Service as IService, try || -> Result<Service, &'static str> { Err("failed") });
        }.build::<&'static str>();
        assert!(matches!(built, Err("failed")));
    }
}

mod borrowed_output {
    struct View<'a>(&'a str);
    trait IView {}
    impl IView for View<'_> {}
    #[systasis::container]
    #[test]
    fn returned_value_can_borrow_captured_configuration() {
        let config: String = String::from("borrowed");
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(View<'_> as IView, move || View(config.as_str()));
        }
        .build();
        let view = container.resolve_i_view();
        assert_eq!(view.0, "borrowed");
    }
}

mod returned_guard {
    use super::Error;
    struct View<'a>(systasis::Ref<'a, String>);
    trait IView {}
    impl IView for View<'_> {}
    trait IValue {}
    impl IValue for String {}
    #[systasis::container]
    #[test]
    fn returned_guard_keeps_dependency_borrowed() -> Result<(), Error> {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(View<'_> as IView, try || -> Result<View<'_>, Error> {
                Ok(View(try_resolve_ref!(IValue)?))
            });
            register_value!(String::from("value"): String as IValue);
        }
        .build();
        let view = container.try_resolve_i_view()?;
        assert_eq!(&*view.0, "value");
        assert!(matches!(
            container.try_resolve_i_value_ref_mut(),
            Err(Error::ValueAccessContention)
        ));
        drop(view);
        assert_eq!(&*container.try_resolve_i_value_ref_mut()?, "value");
        Ok(())
    }
}

mod tuple_capture {
    use super::*;

    #[systasis::container(require(Send, Sync))]
    fn check((prefix, .., suffix): (String, u8, String)) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(Service as IService, move || Service(prefix.clone() + &suffix));
        }
        .build();
        assert_eq!(container.resolve_i_service().0, "prefix-suffix");
        assert_eq!(container.resolve_i_service().0, "prefix-suffix");
    }

    #[test]
    fn typed_destructured_parameters_supply_capture_types() {
        check((String::from("prefix-"), 0, String::from("suffix")));
    }
}

mod array_capture {
    use super::*;

    #[systasis::container]
    #[test]
    fn typed_array_bindings_move_only_the_captured_elements() {
        let [first, middle, last]: [String; 3] = [
            String::from("first"),
            String::from("middle"),
            String::from("last"),
        ];
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(Service as IService, move || Service(first.clone() + &last));
        }
        .build();
        assert_eq!(middle, "middle");
        assert_eq!(container.resolve_i_service().0, "firstlast");
        assert_eq!(container.resolve_i_service().0, "firstlast");
    }
}

mod import_types {
    pub(crate) type Configuration = String;
}

mod imported_capture {
    use super::*;

    #[systasis::container]
    #[test]
    fn explicit_function_local_imports_remain_available_to_capture_storage() {
        use crate::import_types::Configuration as Config;
        use systasis::systasis_container;
        let config: Config = String::from("imported");
        let Ok(container) = systasis_container! {
            register_type_with!(Service as IService, move || {
                use std::string::String as OutputString;
                let output: OutputString = config.clone();
                Service(output)
            });
        }
        .build();
        assert_eq!(container.resolve_i_service().0, "imported");
    }
}

mod reference_capture {
    use super::*;

    #[systasis::container]
    #[test]
    #[allow(clippy::toplevel_ref_arg)] // The binding syntax is the behavior under test.
    fn ref_binding_keeps_caller_owned_value_alive() {
        let ref config: String = String::from("borrowed");
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(Service as IService, move || Service((*config).clone()));
        }
        .build();
        assert_eq!(container.resolve_i_service().0, "borrowed");
        assert_eq!(config, "borrowed");
    }
}
