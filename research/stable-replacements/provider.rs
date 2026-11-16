//! Hand-expanded factory metadata candidate; not production macro output.
#![no_std]
#![forbid(unsafe_code, unstable_features)]

extern crate alloc;

use alloc::string::String;
use core::{error::Error, fmt, str::FromStr};

#[derive(Debug)]
pub struct LocalFailure(alloc::rc::Rc<()>);

impl fmt::Display for LocalFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "local failure ({})",
            alloc::rc::Rc::strong_count(&self.0)
        )
    }
}
impl Error for LocalFailure {}

pub fn local_failure() -> Result<(), LocalFailure> {
    Err(LocalFailure(alloc::rc::Rc::new(())))
}
pub mod local_failure {
    pub type Error = super::LocalFailure;
}

pub struct LocalContainer {
    capture: alloc::rc::Rc<str>,
}

impl LocalContainer {
    pub fn new() -> Self {
        Self {
            capture: alloc::rc::Rc::from("local"),
        }
    }
    pub fn resolve(&self) -> &str {
        &self.capture
    }
}

impl Default for LocalContainer {
    fn default() -> Self {
        Self::new()
    }
}

// An attribute on each factory could emit its same-named metadata module.
// These modules are written out here to test the emitted Rust representation.
pub fn text() -> String {
    String::from("owned")
}
pub mod text {
    pub type Output = alloc::string::String;
}

pub fn number(input: &str) -> Result<u8, <u8 as FromStr>::Err> {
    input.parse()
}
pub mod number {
    pub type Error = <u8 as core::str::FromStr>::Err;
}

pub fn flag(input: &str) -> Result<bool, <bool as FromStr>::Err> {
    input.parse()
}
pub mod flag {
    pub type Error = <bool as core::str::FromStr>::Err;
}

// Capture reconstruction names the producer's output, not the native closure.
// No trait object, allocation for storage, or caller-visible storage generic.
pub struct SystasisContainer {
    capture: text::Output,
}

impl SystasisContainer {
    pub fn new() -> Self {
        let capture = text();
        Self { capture }
    }

    pub fn resolve_text(&self) -> String {
        self.capture.clone()
    }

    // Named storage also preserves lending from owned state.
    pub fn resolve_view(&self) -> &str {
        &self.capture
    }
}

impl Default for SystasisContainer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum SystasisContainerError {
    Number(number::Error),
    Flag(flag::Error),
}

impl fmt::Display for SystasisContainerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(error) => fmt::Display::fmt(error, f),
            Self::Flag(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl Error for SystasisContainerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Number(error) => error,
            Self::Flag(error) => error,
        })
    }
}

pub fn build(input: &str, boolean: &str) -> Result<SystasisContainer, SystasisContainerError> {
    let _ = number(input).map_err(SystasisContainerError::Number)?;
    let _ = flag(boolean).map_err(SystasisContainerError::Flag)?;
    Ok(SystasisContainer::new())
}

pub trait FunctionOutput {
    type Output;
}
impl<T, F: FnOnce() -> T> FunctionOutput for F {
    type Output = T;
}
pub type Never = <fn() -> ! as FunctionOutput>::Output;

pub fn infallible_build() -> Result<SystasisContainer, Never> {
    Ok(SystasisContainer::new())
}
