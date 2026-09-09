#![doc = include_str!("../docs/USAGE.md")]
#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod app_container;

/// Type-level support for generated child descriptors; not a stable manual API.
#[doc(hidden)]
pub mod scoped;

mod fallible;
pub use fallible::Fallible;

mod factory;
mod fresh;
mod storage;
pub use storage::{Ref, RefMut};
pub use systasis_macros::{container, systasis_container};

/// Runtime support for generated code; not a stable hand-written API.
#[doc(hidden)]
pub mod __private {
    pub use crate::factory::FactorySlot;
    pub use crate::fresh::FreshSlot;
    pub use crate::storage::{
        CopyFallback, CopyKnown, CopySlot, CopyUnknown, DetectCopy, LocalTakeSlot, Pick, Policy,
        ReadSlot, Select, TakeSlot, verify_generic_fallback,
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

    pub fn invoke<R>(constructor: impl Fn() -> R) -> R {
        constructor()
    }

    pub fn check_fallible<T, F: crate::Fallible<Output = T>>(value: F) -> F {
        value
    }
}
