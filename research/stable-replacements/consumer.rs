#![forbid(unsafe_code, unstable_features)]

use stable_metadata::{SystasisContainer, SystasisContainerError};

fn receive(container: &SystasisContainer) -> String {
    container.resolve_text()
}

fn init() -> Result<SystasisContainer, SystasisContainerError> {
    stable_metadata::build("42", "true")
}

fn send_sync<T: Send + Sync>(_: &T) {}

fn main() {
    let container = init().unwrap();
    send_sync(&container);
    assert_eq!(receive(&container), "owned");
    assert_eq!(receive(&container), "owned");
    assert_eq!(container.resolve_view(), "owned");

    let Err(error) = stable_metadata::build("bad", "true") else {
        panic!("number must fail");
    };
    send_sync(&error);
    assert!(
        core::error::Error::source(&error)
            .unwrap()
            .is::<core::num::ParseIntError>()
    );
    let Err(error) = stable_metadata::build("42", "bad") else {
        panic!("flag must fail");
    };
    assert!(
        core::error::Error::source(&error)
            .unwrap()
            .is::<core::str::ParseBoolError>()
    );

    let Ok(container) = stable_metadata::infallible_build();
    assert_eq!(receive(&container), "owned");
    assert_eq!(stable_metadata::LocalContainer::new().resolve(), "local");
    assert_eq!(
        stable_metadata::local_failure().unwrap_err().to_string(),
        "local failure (1)"
    );
}
