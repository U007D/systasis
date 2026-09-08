//! Allocation-free no_std storage using Spin's mapped lock guards.
#![forbid(unsafe_code)]

use crate::app_container::Error;
use ::spin::lock_api::{
    MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLock, RwLockReadGuard, RwLockWriteGuard,
};
use core::ops::{Deref, DerefMut};

/// Consumable storage with nonblocking, synchronized access.
#[doc(hidden)]
pub struct TakeSlot<T> {
    value: RwLock<Option<T>>,
}

impl<T> TakeSlot<T> {
    /// Stores a value without allocating or invoking user code.
    pub const fn new(value: T) -> Self {
        Self {
            value: RwLock::new(Some(value)),
        }
    }

    fn read(&self) -> Result<RwLockReadGuard<'_, Option<T>>, Error> {
        self.value.try_read().ok_or(Error::ValueAccessContention)
    }

    fn write(&self) -> Result<RwLockWriteGuard<'_, Option<T>>, Error> {
        self.value.try_write().ok_or(Error::ValueAccessContention)
    }

    /// Takes ownership, leaving the slot empty after successful acquisition.
    pub fn try_resolve(&self) -> Result<T, Error> {
        self.write()?.take().ok_or(Error::ValueAlreadyConsumed)
    }

    /// Borrows a present value while retaining the acquired read lock.
    pub fn try_resolve_ref(&self) -> Result<Ref<'_, T>, Error> {
        RwLockReadGuard::try_map(self.read()?, Option::as_ref)
            .map(|guard| Ref { guard })
            .map_err(|_| Error::ValueAlreadyConsumed)
    }

    /// Mutably borrows a present value while retaining the acquired write lock.
    pub fn try_resolve_ref_mut(&self) -> Result<RefMut<'_, T>, Error> {
        RwLockWriteGuard::try_map(self.write()?, Option::as_mut)
            .map(|guard| RefMut { guard })
            .map_err(|_| Error::ValueAlreadyConsumed)
    }

    /// Invokes `Clone` under a read lock, retaining the original stored value.
    pub fn try_resolve_clone(&self) -> Result<T, Error>
    where
        T: Clone,
    {
        self.try_resolve_ref().map(|guard| T::clone(&guard))
    }
}

/// A shared reference to a present value, retaining its Spin read-lock guard.
///
/// This is a systasis guard, not `std::cell::Ref`.
pub struct Ref<'a, T: ?Sized> {
    guard: MappedRwLockReadGuard<'a, T>,
}

impl<'a, T: ?Sized> Ref<'a, T> {
    /// Changes the reference target without releasing the original read lock.
    #[doc(hidden)]
    pub fn map<U: ?Sized, F>(original: Self, project: F) -> Ref<'a, U>
    where
        F: FnOnce(&T) -> &U,
    {
        Ref {
            guard: MappedRwLockReadGuard::map(original.guard, project),
        }
    }
}

impl<T: ?Sized> Deref for Ref<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.guard
    }
}

/// An exclusive reference to a present value, retaining its Spin write lock.
pub struct RefMut<'a, T: ?Sized> {
    guard: MappedRwLockWriteGuard<'a, T>,
}

impl<T: ?Sized> Deref for RefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.guard
    }
}

impl<T: ?Sized> DerefMut for RefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}
