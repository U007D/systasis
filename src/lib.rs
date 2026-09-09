//! Statically wired dependency injection.
//!
//! This package is under development; container generation currently covers stored values.
#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod app_container;

mod fallible;
pub use fallible::Fallible;

mod storage;
pub use storage::{Ref, RefMut};
pub use systasis_macros::{container, systasis_container};

/// Runtime support for generated code; not a stable hand-written API.
#[doc(hidden)]
pub mod __private {
    pub use crate::storage::{
        CopyFallback, CopySlot, LocalTakeSlot, Pick, Policy, ReadSlot, Select, TakeSlot,
    };
    pub use crate::{Ref, RefMut, app_container::Error};

    pub fn split<T, E>(result: Result<T, E>) -> (Option<T>, Option<E>) {
        match result {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error)),
        }
    }

    pub fn discard<T>(value: T) {
        drop(value);
    }
}
