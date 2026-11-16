//! Proposed API, compiled against the hand-expanded stable research provider.
#![forbid(unsafe_code, unstable_features)]

use core::error::Error;
use stable_rpit::infallible::{BuildResult, SystasisContainer};

fn init() -> BuildResult<impl Fn() -> u8, impl Error> {
    SystasisContainer::build()
}

fn read_number<F, E>(container: &SystasisContainer<F, E>) -> u8
where
    F: Fn() -> u8,
{
    container.resolve_i_number()
}

fn main() {
    let Ok(container) = init();
    println!("{}", read_number(&container));
}
