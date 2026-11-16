#![forbid(unsafe_code, unstable_features)]

use core::error::Error;
use stable_rpit::{infallible, mixed_success, opaque_failure, parse_failure, utf8_failure};

fn receive<F, E>(container: &infallible::SystasisContainer<F, E>) -> u8
where
    F: Fn() -> u8,
{
    container.resolve_i_number()
}

fn returned() -> infallible::BuildResult<impl Fn() -> u8, impl Error> {
    infallible::SystasisContainer::build()
}

fn returned_fallible() -> mixed_success::BuildResult<impl Fn() -> u8, impl Error> {
    mixed_success::SystasisContainer::build()
}

fn send_sync<T: Send + Sync>(_: &T) {}

fn main() {
    assert_eq!(stable_rpit::counts(), (0, 0, 0));
    {
        let Ok(container) = returned();
        assert_eq!(stable_rpit::counts(), (1, 0, 0));
        assert_eq!(receive(&container), 42);
        assert_eq!(receive(&container), 42);
        send_sync(&container);
        assert_eq!(stable_rpit::counts(), (1, 2, 0));
    }
    assert_eq!(stable_rpit::counts(), (1, 2, 1));

    stable_rpit::reset_counts();
    let error = match parse_failure::SystasisContainer::build() {
        Err(e) => e,
        Ok(_) => unreachable!(),
    };
    assert_eq!(stable_rpit::counts(), (1, 0, 1));
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<core::num::ParseIntError>()
            .is_some()
    );
    send_sync(&error);
    let error = match utf8_failure::SystasisContainer::build() {
        Err(e) => e,
        Ok(_) => unreachable!(),
    };
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<core::str::Utf8Error>()
            .is_some()
    );
    send_sync(&error);
    let error = match opaque_failure::SystasisContainer::build() {
        Err(e) => e,
        Ok(_) => unreachable!(),
    };
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<core::num::ParseIntError>()
            .is_some()
    );
    send_sync(&error);

    let container = match returned_fallible() {
        Ok(c) => c,
        Err(_) => unreachable!(),
    };
    assert_eq!(container.resolve_i_number(), 42);
    send_sync(&container);

    #[cfg(feature = "local_probes")]
    {
        let container = match stable_rpit::local_error_success::SystasisContainer::build() {
            Ok(c) => c,
            Err(_) => unreachable!(),
        };
        send_sync(&container);
    }

    let value = 42u8;
    let borrowed = stable_rpit::borrowed(&value);
    assert_eq!(borrowed.resolve_i_number(), 42);

    let drops = core::cell::Cell::new(0);
    let calls = core::cell::Cell::new(0);
    {
        let constructor = stable_rpit::tracked(stable_rpit::Tracked {
            drops: &drops,
            calls: &calls,
        });
        assert_eq!(calls.get(), 0);
        assert_eq!(constructor(), 42);
        assert_eq!(constructor(), 42);
        assert_eq!(drops.get(), 0);
    }
    assert_eq!(calls.get(), 2);
    assert_eq!(drops.get(), 1);
    println!(
        "PASS: downstream naming, inferred captures/errors, irrefutable let Ok, source identity and auto traits"
    );
}
