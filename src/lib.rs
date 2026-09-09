//! Statically wired dependency injection.
//!
//! This package is under development. Container generation is not implemented yet.
#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod app_container;

mod fallible;
pub use fallible::Fallible;

mod storage;
pub use storage::{Ref, RefMut};

/// Runtime support for generated code; not a stable hand-written API.
#[doc(hidden)]
pub mod __private {
    pub use crate::storage::{CopySlot, TakeSlot};
}
