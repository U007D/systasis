//! Zero-state construction: the returned value is never stored in the container.

use core::marker::PhantomData;

/// Zero-state descriptor for a fresh default-constructed value.
pub struct FreshSlot<T>(PhantomData<fn() -> T>);

impl<T> FreshSlot<T> {
    /// Create a descriptor without constructing a value.
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T> Default for FreshSlot<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Default> FreshSlot<T> {
    /// Construct one independent value.
    pub fn resolve(&self) -> T {
        T::default()
    }
}
