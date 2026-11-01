use std::sync::{Mutex, MutexGuard};

/// Locking for state that stays valid after a panicking holder: a poisoned lock is taken over, not propagated.
pub trait LockExt<T> {
    fn lock_any(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_any(&self) -> MutexGuard<'_, T> {
        self.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
