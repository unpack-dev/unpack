//! Stable-Rust equivalent of the subset of `std::cell::SyncUnsafeCell` used here.
//!
//! The upstream crate uses the unstable standard-library type. This wrapper preserves
//! its synchronization contract while exposing only the operations needed by `WriteBatch`.

use std::cell::UnsafeCell;

#[repr(transparent)]
pub(crate) struct SyncUnsafeCell<T: ?Sized> {
    value: UnsafeCell<T>,
}

impl<T> SyncUnsafeCell<T> {
    pub(crate) const fn new(value: T) -> Self {
        Self {
            value: UnsafeCell::new(value),
        }
    }
}

impl<T: ?Sized> SyncUnsafeCell<T> {
    pub(crate) const fn get(&self) -> *mut T {
        self.value.get()
    }

    pub(crate) fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }
}

// SAFETY: This matches `std::cell::SyncUnsafeCell`: shared access is safe when the
// contained value is `Sync`. Mutation remains unsafe and is constrained by callers.
unsafe impl<T: ?Sized + Sync> Sync for SyncUnsafeCell<T> {}
