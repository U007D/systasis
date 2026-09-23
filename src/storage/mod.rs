//! Concrete storage policies used by generated containers.

mod local;
#[cfg(feature = "resolve_unchecked")]
mod unchecked;
pub use local::LocalTakeSlot;
mod policy;
pub use policy::{
    CloneFallback, CloneKnown, CloneUnknown, CopyFallback, CopyKnown, CopyUnknown, DetectClone,
    DetectCopy, Pick, Policy, Select, verify_generic_clone_fallback, verify_generic_fallback,
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

    /// Clones the immutable value; no availability or locking failure is possible.
    pub fn try_resolve_clone(&self) -> Result<T, crate::__private::Never>
    where
        T: Clone,
    {
        Ok(self.resolve_clone())
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

    /// Copies the immutable value; no availability or locking failure is possible.
    pub fn try_resolve(&self) -> Result<T, crate::__private::Never> {
        Ok(self.resolve())
    }

    /// Borrows the stored value without acquiring a lock.
    pub fn resolve_ref(&self) -> &T {
        &self.0
    }
}

#[cfg(test)]
mod value_policy_tests {
    use super::*;
    use core::cell::Cell;

    #[test]
    fn copying_has_an_uninhabited_error_and_is_repeatable() {
        let slot = CopySlot::new(42);
        let Ok(first) = slot.try_resolve();
        let Ok(second) = slot.try_resolve();
        assert_eq!((first, second), (42, 42));
    }

    #[test]
    fn cloning_keeps_the_original_and_calls_clone_each_time() {
        struct Counted<'a>(&'a Cell<usize>);
        impl Clone for Counted<'_> {
            fn clone(&self) -> Self {
                self.0.set(self.0.get() + 1);
                Self(self.0)
            }
        }
        let calls = Cell::new(0);
        let slot = <Policy<false, false, true> as Select<_>>::store(Counted(&calls));
        let _first = slot.resolve_clone();
        let Ok(_second) = slot.try_resolve_clone();
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn copy_policy_takes_precedence_over_clone() {
        let slot: CopySlot<u8> = <Policy<true, false, true> as Select<_>>::store(7);
        assert_eq!(slot.resolve(), 7);
    }

    #[test]
    fn a_mutable_reference_is_transferred_once() {
        let mut value = 0;
        let slot = <Policy<false, false> as Select<_>>::store(&mut value);
        let borrowed = slot.try_resolve().unwrap();
        *borrowed = 3;
        assert_eq!(
            slot.try_resolve(),
            Err(crate::container::Error::ValueAlreadyConsumed)
        );
        assert_eq!(value, 3);
    }
}
