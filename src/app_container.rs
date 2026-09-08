//! Error contract shared by generated containers.

/// A checked stored-value access could not complete.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Ownership was already transferred out of the container.
    #[error("Error: `AppContainer` value has already been consumed.")]
    ValueAlreadyConsumed,
    /// The required access could not be acquired immediately.
    #[error("Error: `AppContainer` value access is contended.")]
    ValueAccessContention,
    /// A caller panic poisoned the lock. The error retains no lock guard.
    #[cfg(feature = "std")]
    #[error(
        "Error: `AppContainer` encountered a poisoned lock attempting to access a Container value (another thread or context panicked while holding a write lock)."
    )]
    PoisonedLock(#[source] std::sync::PoisonError<()>),
}

#[cfg(feature = "std")]
impl Error {
    pub(crate) fn from_lock<G>(error: std::sync::TryLockError<G>) -> Self {
        match error {
            std::sync::TryLockError::WouldBlock => Self::ValueAccessContention,
            std::sync::TryLockError::Poisoned(error) => {
                drop(error.into_inner());
                // This branch requires an existing PoisonError from std. When
                // std itself is built with panic=abort, PoisonError contains an
                // uninhabited field and locks cannot produce one. For unwind
                // std, new(()) simply constructs the payload without panicking.
                Self::PoisonedLock(std::sync::PoisonError::new(()))
            }
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::Error;
    use std::{
        error::Error as _,
        panic::{AssertUnwindSafe, catch_unwind},
        sync::{PoisonError, RwLock, TryLockError},
    };

    fn poisoned() -> RwLock<u32> {
        let lock = RwLock::new(7);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut guard = lock.write().expect("fixture starts unpoisoned");
            *guard = 9;
            panic!("deliberate caller panic");
        }));
        assert!(result.is_err());
        lock
    }

    #[test]
    fn contention_is_distinct_from_poisoning() {
        let error = Error::from_lock::<()>(TryLockError::WouldBlock);
        assert!(matches!(error, Error::ValueAccessContention));
        assert!(error.source().is_none());
    }

    #[test]
    fn retained_read_error_does_not_hold_lock() {
        let lock = poisoned();
        let error = Error::from_lock(lock.try_read().expect_err("fixture is poisoned"));
        let next = lock.try_write();
        assert!(matches!(next, Err(TryLockError::Poisoned(_))));
        assert!(matches!(error, Error::PoisonedLock(_)));
        assert!(lock.is_poisoned());
    }

    #[test]
    fn retained_write_error_does_not_hold_lock() {
        let lock = poisoned();
        let error = Error::from_lock(lock.try_write().expect_err("fixture is poisoned"));
        let next = lock.try_read();
        let Err(TryLockError::Poisoned(source)) = next else {
            panic!("must acquire and report poisoning, not contention");
        };
        assert_eq!(
            **source.get_ref(),
            9,
            "conversion does not consume the value"
        );
        assert!(matches!(error, Error::PoisonedLock(_)));
    }

    #[test]
    fn poison_source_has_no_guard_and_exact_display() {
        let lock = poisoned();
        let error = Error::from_lock(lock.try_read().expect_err("fixture is poisoned"));
        let source = error.source().expect("poison variant has a source");
        assert!(source.downcast_ref::<PoisonError<()>>().is_some());
        assert_eq!(
            error.to_string(),
            "Error: `AppContainer` encountered a poisoned lock attempting to access a Container value (another thread or context panicked while holding a write lock)."
        );
    }

    #[test]
    fn error_can_outlive_container_and_cross_threads() {
        fn assert_send_sync_static<T: Send + Sync + 'static>() {}
        assert_send_sync_static::<Error>();
        let error = {
            let lock = poisoned();
            Error::from_lock(lock.try_read().expect_err("fixture is poisoned"))
        };
        assert!(matches!(error, Error::PoisonedLock(_)));
    }
}
