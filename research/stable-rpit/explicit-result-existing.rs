#![forbid(unsafe_code, unstable_features)]
use core::error::Error;
#[cfg(not(feature = "fallible_only"))]
use stable_rpit::infallible;
use stable_rpit::mixed_success;

#[cfg(not(feature = "fallible_only"))]
fn infallible() -> Result<
    infallible::SystasisContainer<impl Fn() -> u8, impl Error>,
    infallible::SystasisContainerError<impl Error>,
> {
    infallible::SystasisContainer::build()
}

fn fallible() -> Result<
    mixed_success::SystasisContainer<impl Fn() -> u8, impl Error>,
    mixed_success::SystasisContainerError<impl Error>,
> {
    mixed_success::SystasisContainer::build()
}

fn main() {
    #[cfg(not(feature = "fallible_only"))]
    {
        let Ok(number) = infallible();
        assert_eq!(number.resolve_i_number(), 42);
    }
    assert!(fallible().is_ok());
}
