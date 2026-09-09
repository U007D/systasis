//! Compile-time storage selection used by generated registration code.

use super::{CopySlot, LocalTakeSlot, TakeSlot};
use core::marker::PhantomData;

/// Probe whose inherent constant wins when the compiler establishes Copy.
#[doc(hidden)]
pub struct Pick<T>(PhantomData<fn() -> T>);

impl<T: Copy> Pick<T> {
    /// The registered type is Copy at this use site.
    pub const IS_COPY: bool = true;
}

/// Fallback imported by generated code for types without established Copy.
#[doc(hidden)]
pub trait CopyFallback {
    /// Unknown Copy behavior must use consumable storage.
    const IS_COPY: bool = false;
}

impl<T> CopyFallback for Pick<T> {}

/// Storage choice: copying is independent of the explicit local policy.
#[doc(hidden)]
pub struct Policy<const COPY: bool, const LOCAL: bool>;

/// Construct the selected slot without runtime policy dispatch.
#[doc(hidden)]
pub trait Select<T> {
    /// The concrete stored representation.
    type Slot;
    /// Moves the initial value into its slot.
    fn store(value: T) -> Self::Slot;
}

impl<T: Copy, const LOCAL: bool> Select<T> for Policy<true, LOCAL> {
    type Slot = CopySlot<T>;
    fn store(value: T) -> Self::Slot {
        CopySlot::new(value)
    }
}

impl<T> Select<T> for Policy<false, true> {
    type Slot = LocalTakeSlot<T>;
    fn store(value: T) -> Self::Slot {
        LocalTakeSlot::new(value)
    }
}

impl<T> Select<T> for Policy<false, false> {
    type Slot = TakeSlot<T>;
    fn store(value: T) -> Self::Slot {
        TakeSlot::new(value)
    }
}
