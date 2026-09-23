//! Allocation-free no_std storage with guard-free permanent read reservations.

use crate::container::Error;
use ::spin::lock_api::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};

/// Consumable storage with nonblocking, synchronized access.
#[doc(hidden)]
pub struct TakeSlot<T> {
    reserved: RwLock<bool>,
    value: UnsafeCell<Option<T>>,
}

// SAFETY: access initially acquires the lock. A permanent reservation permits
// only shared payload access thereafter, even after its lock is released.
// Sharing T requires Sync; moving it between threads before reservation requires
// Send. Private fields prevent resetting reservations or unguarded mutation.
unsafe impl<T: Send + Sync> Sync for TakeSlot<T> {}

impl<T> TakeSlot<T> {
    /// Stores a value without allocating or invoking user code.
    pub const fn new(value: T) -> Self {
        Self {
            reserved: RwLock::new(false),
            value: UnsafeCell::new(Some(value)),
        }
    }

    fn read(&self) -> Result<RwLockReadGuard<'_, bool>, Error> {
        self.reserved.try_read().ok_or(Error::ValueAccessContention)
    }

    fn write(&self) -> Result<RwLockWriteGuard<'_, bool>, Error> {
        self.reserved
            .try_write()
            .ok_or(Error::ValueAccessContention)
    }

    /// Takes ownership, leaving the slot empty after successful acquisition.
    pub fn try_resolve(&self) -> Result<T, Error> {
        let guard = self.write()?;
        if *guard {
            return Err(Error::ValueAccessContention);
        }
        // SAFETY: the write lock excludes ordinary accesses, and the checked
        // absence of a reservation excludes outstanding guard-free references.
        unsafe { &mut *self.value.get() }
            .take()
            .ok_or(Error::ValueAlreadyConsumed)
    }

    /// Borrows a present value while retaining the acquired read lock.
    pub fn try_resolve_ref(&self) -> Result<Ref<'_, T>, Error> {
        let guard = self.read()?;
        // SAFETY: the read lock excludes removal and mutation. Ordinary readers
        // and any permanent reservations expose only shared references.
        let value = unsafe { &*self.value.get() }
            .as_ref()
            .ok_or(Error::ValueAlreadyConsumed)?;
        Ok(Ref { guard, value })
    }

    /// Mutably borrows a present value while retaining the acquired write lock.
    pub fn try_resolve_ref_mut(&self) -> Result<RefMut<'_, T>, Error> {
        let guard = self.write()?;
        if *guard {
            return Err(Error::ValueAccessContention);
        }
        // SAFETY: exclusive acquisition and the absence of a reservation
        // exclude every other payload reference before forming &mut Option<T>.
        let value = unsafe { &mut *self.value.get() }
            .as_mut()
            .ok_or(Error::ValueAlreadyConsumed)?;
        Ok(RefMut {
            _guard: guard,
            value,
        })
    }

    /// Invokes `Clone` under a read lock, retaining the original stored value.
    pub fn try_resolve_clone(&self) -> Result<T, Error>
    where
        T: Clone,
    {
        self.try_resolve_ref().map(|guard| T::clone(&guard))
    }

    /// Permanently reserves shared access and returns a guard-free reference.
    ///
    /// The reservation lasts until this slot is destroyed, even after the
    /// returned reference's last use. Taking and mutable access then report
    /// contention. Acquisition is nonblocking and requires an exclusive lock,
    /// including repeated reservations. The reference cannot outlive the slot.
    #[doc(hidden)]
    pub fn try_reserve_ref(&self) -> Result<&T, Error> {
        let mut guard = self.write()?;
        // SAFETY: exclusive acquisition excludes ordinary writers and readers.
        // Existing reservations permit only shared access. No mutable payload
        // reference is formed, including on repeated reservation attempts.
        let value = unsafe { &*self.value.get() }
            .as_ref()
            .ok_or(Error::ValueAlreadyConsumed)?;
        *guard = true;
        drop(guard);
        Ok(value)
    }
}

/// A shared reference to a present value, retaining its Spin read-lock guard.
///
/// This is a systasis guard, not `std::cell::Ref`.
pub struct Ref<'a, T: ?Sized> {
    guard: RwLockReadGuard<'a, bool>,
    value: &'a T,
}

impl<'a, T: ?Sized> Ref<'a, T> {
    /// Changes the reference target without releasing the original read lock.
    #[doc(hidden)]
    pub fn map<U: ?Sized, F>(original: Self, project: F) -> Ref<'a, U>
    where
        F: FnOnce(&T) -> &U,
    {
        Ref {
            value: project(original.value),
            guard: original.guard,
        }
    }
}

impl<T: ?Sized> Deref for Ref<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value
    }
}

/// An exclusive reference to a present value, retaining its Spin write lock.
///
/// Like `&mut T`, this guard is invariant in T.
pub struct RefMut<'a, T: ?Sized> {
    _guard: RwLockWriteGuard<'a, bool>,
    value: &'a mut T,
}

impl<T: ?Sized> Deref for RefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value
    }
}

impl<T: ?Sized> DerefMut for RefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value
    }
}
