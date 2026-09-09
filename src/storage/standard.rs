//! Stable std guards: the lock protects a separately stored optional payload.
use crate::app_container::Error;
use core::{
    cell::UnsafeCell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

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

// Match RwLock<Option<T>>: unwinding through a write guard poisons the lock.
// These advisory traits neither recover the value nor bypass poison checking.
impl<T> std::panic::UnwindSafe for TakeSlot<T> {}
impl<T> std::panic::RefUnwindSafe for TakeSlot<T> {}

impl<T> TakeSlot<T> {
    /// Stores a value without allocating or invoking user code.
    pub const fn new(value: T) -> Self {
        Self {
            reserved: RwLock::new(false),
            value: UnsafeCell::new(Some(value)),
        }
    }

    /// Takes ownership, leaving the slot empty after successful acquisition.
    pub fn try_resolve(&self) -> Result<T, Error> {
        let guard = self.reserved.try_write().map_err(Error::from_lock)?;
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
        let guard = self.reserved.try_read().map_err(Error::from_lock)?;
        // SAFETY: the read lock excludes removal and mutation. Other readers
        // only create shared references. No payload reference precedes locking.
        let value = unsafe { &*self.value.get() }
            .as_ref()
            .ok_or(Error::ValueAlreadyConsumed)?;
        Ok(Ref {
            guard,
            value: NonNull::from(value),
            marker: PhantomData,
        })
    }

    /// Mutably borrows a present value while retaining the acquired write lock.
    pub fn try_resolve_ref_mut(&self) -> Result<RefMut<'_, T>, Error> {
        let guard = self.reserved.try_write().map_err(Error::from_lock)?;
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
            value: NonNull::from(value),
            marker: PhantomData,
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
        let mut guard = self.reserved.try_write().map_err(Error::from_lock)?;
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

/// A shared reference to a present value, retaining its std read-lock guard.
///
/// This is a systasis guard, not `std::cell::Ref`. It cannot be sent to another
/// thread because the underlying std lock guard must be dropped on its thread.
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<systasis::Ref<'static, u32>>();
/// ```
pub struct Ref<'a, T: ?Sized> {
    guard: RwLockReadGuard<'a, bool>,
    value: NonNull<T>,
    marker: PhantomData<&'a T>,
}

impl<'a, T: ?Sized> Ref<'a, T> {
    /// Changes the reference target without releasing the original read lock.
    #[doc(hidden)]
    pub fn map<U: ?Sized, F>(original: Self, project: F) -> Ref<'a, U>
    where
        F: FnOnce(&T) -> &U,
    {
        let value = NonNull::from(project(&original));
        Ref {
            guard: original.guard,
            value,
            marker: PhantomData,
        }
    }
}

impl<T: ?Sized> Deref for Ref<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: construction checked Some under the retained lock. Mapping
        // preserves that lock and projects a reference valid while the source
        // remains valid. The returned lifetime is limited by &self.
        unsafe { self.value.as_ref() }
    }
}

// SAFETY: sharing the wrapper exposes only &T. Its retained read lock excludes
// mutation/removal, and T: Sync permits shared references across threads.
// No Send implementation: the std guard must drop on its owning thread.
unsafe impl<T: ?Sized + Sync> Sync for Ref<'_, T> {}

/// An exclusive reference to a present value, retaining its std write lock.
///
/// Like `&mut T`, this guard is invariant in T. Like its std lock guard, it is
/// not `Send`.
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<systasis::RefMut<'static, u32>>();
/// ```
///
/// ```compile_fail
/// fn shorten<'guard, 'short>(
///     value: systasis::RefMut<'guard, &'static str>,
/// ) -> systasis::RefMut<'guard, &'short str> {
///     value
/// }
/// ```
pub struct RefMut<'a, T: ?Sized> {
    _guard: RwLockWriteGuard<'a, bool>,
    value: NonNull<T>,
    marker: PhantomData<&'a mut T>,
}

impl<T: ?Sized> Deref for RefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the exclusive lock protects the checked, present value.
        // &self exposes only a shared reborrow for the lifetime of &self.
        unsafe { self.value.as_ref() }
    }
}

impl<T: ?Sized> DerefMut for RefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: &mut self guarantees an exclusive reborrow of the retained
        // exclusive access. PhantomData<&mut T> prevents covariant substitution.
        unsafe { self.value.as_mut() }
    }
}

// SAFETY: sharing &RefMut exposes only &T. DerefMut requires &mut RefMut,
// which cannot coexist with shared borrows. The exclusive lock remains held.
unsafe impl<T: ?Sized + Sync> Sync for RefMut<'_, T> {}
