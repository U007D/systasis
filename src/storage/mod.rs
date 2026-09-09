//! Concrete storage policies used by generated containers.

mod local;
pub use local::LocalTakeSlot;
mod policy;
pub use policy::{
    CopyFallback, CopyKnown, CopyUnknown, DetectCopy, Pick, Policy, Select, verify_generic_fallback,
};

#[cfg(not(feature = "std"))]
mod spin;
#[cfg(feature = "std")]
mod standard;

#[cfg(not(feature = "std"))]
pub use spin::{Ref, RefMut, TakeSlot};
#[cfg(feature = "std")]
pub use standard::{Ref, RefMut, TakeSlot};

/// Immutable, untakeable storage: neither locks nor runtime borrow counters.
#[doc(hidden)]
#[repr(transparent)]
pub struct ReadSlot<T>(T);

impl<T> ReadSlot<T> {
    /// Stores the value directly.
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    /// Borrows the value without synchronization.
    pub fn resolve_ref(&self) -> &T {
        &self.0
    }

    /// Clones the value without changing the slot.
    pub fn resolve_clone(&self) -> T
    where
        T: Clone,
    {
        self.0.clone()
    }
}

/// Plain storage for a registration with established `Copy` behavior.
#[doc(hidden)]
#[repr(transparent)]
pub struct CopySlot<T>(T);

impl<T> CopySlot<T> {
    /// Stores a value without allocating or invoking user code.
    pub const fn new(value: T) -> Self {
        Self(value)
    }
}

impl<T: Copy> CopySlot<T> {
    /// Copies the stored value.
    pub fn resolve(&self) -> T {
        self.0
    }

    /// Explicitly invokes `Clone`, even when its behavior differs from copying.
    #[allow(clippy::clone_on_copy)]
    pub fn resolve_clone(&self) -> T {
        T::clone(&self.0)
    }

    /// Borrows the stored value without acquiring a lock.
    pub fn resolve_ref(&self) -> &T {
        &self.0
    }
}
