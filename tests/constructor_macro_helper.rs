//! Ordinary helper functions may contain macros without capture rewriting.
//! This is a supported workaround, not equivalent to arbitrary inline macros.
#![forbid(unsafe_code)]

struct Service(String);
trait IService {}
impl IService for Service {}

static CALLS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

fn construct(config: &str) -> Service {
    CALLS.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
    Service(format!("configured: {config}"))
}

#[systasis::container]
#[test]
fn macros_in_called_functions_execute_on_each_resolution() {
    let config: String = String::from("example");
    let Ok(container) = systasis::systasis_container! {
        register_type_with!(Service as IService, move || construct(&config));
    }
    .build();
    assert_eq!(CALLS.load(core::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(container.resolve_i_service().0, "configured: example");
    assert_eq!(container.resolve_i_service().0, "configured: example");
    assert_eq!(CALLS.load(core::sync::atomic::Ordering::SeqCst), 2);
}

mod borrowed_local_input {
    use super::{IService, Service};

    fn construct(config: &str) -> Service {
        Service(format!("configured: {config}"))
    }

    #[systasis::container(require(Send, Sync))]
    #[test]
    fn macro_function_can_use_a_borrowed_local_input() {
        macro_rules! construct {
            () => {
                7_usize
            };
        }
        #[cfg(any())]
        macro_rules! discarded {
            () => {
                nonexistent_binding
            };
        }
        let marker: usize = construct!();
        let text: String = String::from("example");
        let config: &str = &text;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(Service as IService, move || construct(config));
        }
        .build();
        assert_eq!(container.resolve_i_service().0, "configured: example");
        assert_eq!(container.resolve_i_service().0, "configured: example");
        assert_eq!(text, "example");
        assert_eq!(marker, 7);
    }
}
