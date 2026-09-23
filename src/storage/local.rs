//! Non-atomic borrow tracking for slots confined to one thread at a time.

use crate::container::Error;
use core::cell::{Ref, RefCell, RefMut};

/// Consumable storage without thread synchronization.
///
/// This slot is never Sync. Its guards must not cross threads. Generated code
/// must select this policy only when every access path respects those bounds.
#[doc(hidden)]
pub struct LocalTakeSlot<T>(RefCell<Option<T>>);

impl<T> LocalTakeSlot<T> {
    /// Stores a value without allocating.
    pub const fn new(value: T) -> Self {
        Self(RefCell::new(Some(value)))
    }

    /// Takes the value only when no guard remains outstanding.
    pub fn try_resolve(&self) -> Result<T, Error> {
        self.0
            .try_borrow_mut()
            .map_err(|_| Error::ValueAccessContention)?
            .take()
            .ok_or(Error::ValueAlreadyConsumed)
    }

    /// Retains a non-atomic shared borrow guard after checking occupancy.
    pub fn try_resolve_ref(&self) -> Result<Ref<'_, T>, Error> {
        let guard = self
            .0
            .try_borrow()
            .map_err(|_| Error::ValueAccessContention)?;
        Ref::filter_map(guard, Option::as_ref).map_err(|_| Error::ValueAlreadyConsumed)
    }

    /// Retains a non-atomic exclusive borrow guard after checking occupancy.
    pub fn try_resolve_ref_mut(&self) -> Result<RefMut<'_, T>, Error> {
        let guard = self
            .0
            .try_borrow_mut()
            .map_err(|_| Error::ValueAccessContention)?;
        RefMut::filter_map(guard, Option::as_mut).map_err(|_| Error::ValueAlreadyConsumed)
    }

    /// Clones under a shared borrow, leaving the original value stored.
    pub fn try_resolve_clone(&self) -> Result<T, Error>
    where
        T: Clone,
    {
        self.try_resolve_ref().map(|value| T::clone(&value))
    }
}
