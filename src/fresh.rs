//! Zero-state type lookup with optional Default construction.

use core::marker::PhantomData;

/// Type descriptor; resolving a value additionally requires `T: Default`.
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

    /// Constructs a fresh value with an uninhabited error type.
    pub fn try_resolve(&self) -> Result<T, crate::__private::Never> {
        Ok(self.resolve())
    }
}
