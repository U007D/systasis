//! Native closures retain macro-expanded captures in their original source scope.
#![forbid(unsafe_code)]
// These formatting calls deliberately exercise implicit macro captures.
#![allow(clippy::useless_format)]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Service(String);
trait IService {}
impl IService for Service {}

fn receive(container: &AppContainer) -> String {
    container.resolve_i_service().0
}

#[systasis::container(require(Send, Sync))]
#[test]
fn native_macro_captures_are_lazy_repeatable_and_nameable() {
    let config: String = String::from("configuration");
    let calls: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let Ok(container) = systasis::systasis_container! {
        register_type_with!(Service as IService, move || {
            calls.fetch_add(1, Ordering::SeqCst);
            Service(format!("{config}"))
        });
    }
    .build();
    assert_eq!(observed.load(Ordering::SeqCst), 0);
    assert_eq!(receive(&container), "configuration");
    assert_eq!(container.resolve_i_service().0, "configuration");
    assert_eq!(observed.load(Ordering::SeqCst), 2);
}

mod inferred_capture {
    use super::{IService, Service};

    fn receive(container: &AppContainer) -> String {
        container.resolve_i_service().0
    }

    #[systasis::container(require(Send, Sync))]
    #[test]
    fn inferred_capture_preserves_concrete_container_and_auto_traits() {
        // Intentionally unannotated: inference is accepted; docs stay typed.
        let config = String::from("inferred");
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(Service as IService, move || Service(format!("{config}")));
        }
        .build();
        assert_eq!(receive(&container), "inferred");
        assert_eq!(receive(&container), "inferred");
    }
}

mod returned_guard {
    use systasis::app_container::Error;
    struct View<'a>(systasis::Ref<'a, String>);
    trait IView {}
    impl IView for View<'_> {}
    trait IValue {}
    impl IValue for String {}

    #[systasis::container]
    #[test]
    fn macro_constructor_retains_dependency_guard() -> Result<(), Error> {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(View<'_> as IView, try || -> Result<View<'_>, Error> {
                let guard = try_resolve_ref!(IValue)?;
                assert_eq!(&*guard, "value");
                Ok(View(guard))
            });
            register_value!(String::from("value"): String as IValue);
        }
        .build();
        let view = container.try_resolve_i_view()?;
        assert!(matches!(
            container.try_resolve_i_value_ref_mut(),
            Err(Error::ValueAccessContention)
        ));
        assert_eq!(&*view.0, "value");
        drop(view);
        assert!(container.try_resolve_i_value_ref_mut().is_ok());
        Ok(())
    }
}

mod returned_write_guard {
    use systasis::app_container::Error;
    struct View<'a>(systasis::RefMut<'a, String>);
    trait IView {}
    impl IView for View<'_> {}
    trait IValue {}
    impl IValue for String {}

    #[systasis::container]
    #[test]
    fn native_constructor_retains_exclusive_guard_until_output_drops() -> Result<(), Error> {
        let Ok(container) = systasis::systasis_container! {
            register_value!(String::from("before"): String as IValue);
            register_type_with!(View<'_> as IView, try || -> Result<View<'_>, Error> {
                let guard = try_resolve_ref_mut!(IValue)?;
                assert!(!guard.is_empty());
                Ok(View(guard))
            });
        }
        .build();
        let mut view = container.try_resolve_i_view()?;
        assert!(matches!(
            container.try_resolve_i_value_ref(),
            Err(Error::ValueAccessContention)
        ));
        assert!(matches!(
            container.try_resolve_i_view(),
            Err(Error::ValueAccessContention)
        ));
        view.0.push_str("-after");
        drop(view);
        assert_eq!(&*container.try_resolve_i_value_ref()?, "before-after");
        assert_eq!(&*container.try_resolve_i_view()?.0, "before-after");
        Ok(())
    }
}

mod transitive {
    use super::*;
    trait IText {}
    impl IText for String {}
    trait IStored {}
    impl IStored for Service {}

    #[systasis::container]
    #[test]
    fn multiple_native_constructors_keep_transitive_initialization_order() {
        let config: String = String::from("configured");
        let Ok(container) = systasis::systasis_container! {
            register_value!(resolve!(IService): Service as IStored);
            register_type_with!(Service as IService, || {
                let text = resolve!(IText);
                Service(format!("{text}!"))
            });
            register_type_with!(String as IText, move || format!("{config}"));
        }
        .build();
        assert_eq!(container.try_resolve_i_stored().unwrap().0, "configured!");
        assert_eq!(container.resolve_i_service().0, "configured!");
    }
}

mod cleanup {
    use super::*;
    struct Capture(Arc<AtomicUsize>);
    impl Drop for Capture {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl std::fmt::Display for Capture {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("capture")
        }
    }
    trait IFailure {}
    impl IFailure for () {}

    #[systasis::container]
    #[test]
    fn failed_build_drops_uninitialized_native_captures_immediately() {
        let drops = Arc::new(AtomicUsize::new(0));
        let captured: Capture = Capture(drops.clone());
        let built = systasis::systasis_container! {
            register_value!(Err::<(), ()>(())?: () as IFailure);
            register_type_with!(Service as IService, move || Service(format!("{captured}")));
        }
        .build::<()>();
        assert!(built.is_err());
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
}

mod source_scope {
    use super::*;
    #[systasis::container]
    #[test]
    fn caller_and_body_local_macros_keep_rust_scoping() {
        macro_rules! construct {
            ($value:expr) => {
                Service($value)
            };
        }
        let config: String = String::from("outer");
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(Service as IService, move || {
                macro_rules! text { () => { String::from("inner") }; }
                let config = text!();
                construct!(config)
            });
        }
        .build();
        assert_eq!(container.resolve_i_service().0, "inner");
        assert_eq!(config, "outer");
    }
}

mod local_receiver {
    use super::*;
    #[systasis::container]
    #[test]
    fn nested_item_receivers_are_not_constructor_contexts() {
        let config: String = String::from("receiver");
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(Service as IService, move || {
                struct Local(String);
                impl Local {
                    fn text(&self) -> String { format!("{}", self.0) }
                }
                Service(Local(config.clone()).text())
            });
        }
        .build();
        assert_eq!(container.resolve_i_service().0, "receiver");
    }
}

mod generic_capture {
    struct View<'a, T>(&'a str, T);
    trait IView {}
    impl<T> IView for View<'_, T> {}

    #[systasis::container(require(Send, Sync))]
    fn check<'env, T: Clone + Send + Sync>(label: &'env str, value: T) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(View<'env, T> as IView, move || {
                assert!(!label.is_empty());
                View(label, value.clone())
            });
        }
        .build();
        fn receive<'env, T: Clone + Send + Sync>(
            container: &AppContainer<'env, T>,
        ) -> View<'env, T> {
            container.resolve_i_view()
        }
        let View(observed, value) = receive(&container);
        assert_eq!(observed, label);
        drop(value);
    }

    #[test]
    fn generic_native_captures_retain_external_lifetime_and_auto_traits() {
        let label = String::from("external");
        check(&label, String::from("owned"));
    }
}

mod configured {
    use super::*;
    #[systasis::container]
    #[test]
    fn configuration_staging_preserves_internal_feature_allowance() {
        #[cfg(any())]
        let config: String = undefined();
        #[cfg(not(any()))]
        let config: String = String::from("active");
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(Service as IService, move || Service(format!("{config}")));
        }
        .build();
        assert_eq!(container.resolve_i_service().0, "active");
    }
}

mod children {
    use super::*;
    mod leaf {
        use super::*;
        #[systasis::container]
        pub fn run(config: String) {
            let Ok(container) = systasis::systasis_container! {
                register_type_with!(Service as IService, move || Service(format!("{config}")));
            }
            .build();
            replica_factory::run(&container);
        }
    }
    mod replica_factory {
        use super::*;
        #[systasis::container]
        pub fn run(primary: &leaf::AppContainer) {
            let config: String = String::from("replica");
            let Ok(container) = systasis::systasis_container! {
                register_type_with!(Service as IService, move || Service(format!("{config}")));
            }
            .build();
            check(primary, &container);
        }
    }
    #[systasis::container]
    fn check<'a>(primary: &'a leaf::AppContainer, replica: &'a replica_factory::AppContainer) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &'a leaf::AppContainer);
            register_container!(replica: &'a replica_factory::AppContainer);
            register_type_with!(Service as IService, || {
                let primary = resolve_from!(IService, primary);
                let replica = resolve_from!(IService, replica);
                Service(format!("{}:{}", primary.0, replica.0))
            });
        }
        .build();
        fn receive(scope: &primary::SubContainer<'_>) -> String {
            scope.resolve_i_service().0
        }
        assert_eq!(receive(container.primary()), "primary");
        assert_eq!(container.replica().resolve_i_service().0, "replica");
        assert_eq!(container.resolve_i_service().0, "primary:replica");
    }
    #[test]
    fn multiple_child_containers_keep_named_scopes_and_native_constructors() {
        leaf::run(String::from("primary"));
    }
}
