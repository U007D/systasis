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
    lock: RwLock<()>,
    value: UnsafeCell<Option<T>>,
}

// SAFETY: every payload access through &self first acquires the lock. Shared
// acquisition exposes only &T, requiring Sync. Exclusive acquisition can move
// T between threads, requiring Send. Private fields prevent unguarded access.
unsafe impl<T: Send + Sync> Sync for TakeSlot<T> {}

// Match RwLock<Option<T>>: unwinding through a write guard poisons the lock.
// These advisory traits neither recover the value nor bypass poison checking.
impl<T> std::panic::UnwindSafe for TakeSlot<T> {}
impl<T> std::panic::RefUnwindSafe for TakeSlot<T> {}

impl<T> TakeSlot<T> {
    /// Stores a value without allocating or invoking user code.
    pub const fn new(value: T) -> Self {
        Self {
            lock: RwLock::new(()),
            value: UnsafeCell::new(Some(value)),
        }
    }

    /// Takes ownership, leaving the slot empty after successful acquisition.
    pub fn try_resolve(&self) -> Result<T, Error> {
        let _guard = self.lock.try_write().map_err(Error::from_lock)?;
        // SAFETY: the retained write lock excludes all other payload accesses
        // for the entire use of the mutable reference.
        unsafe { &mut *self.value.get() }
            .take()
            .ok_or(Error::ValueAlreadyConsumed)
    }

    /// Borrows a present value while retaining the acquired read lock.
    pub fn try_resolve_ref(&self) -> Result<Ref<'_, T>, Error> {
        let guard = self.lock.try_read().map_err(Error::from_lock)?;
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
        let guard = self.lock.try_write().map_err(Error::from_lock)?;
        // SAFETY: the exclusive lock excludes every other payload access.
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
    guard: RwLockReadGuard<'a, ()>,
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
    _guard: RwLockWriteGuard<'a, ()>,
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
