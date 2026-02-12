//! Synchronization primitives
//!
//! Provides an IRQ-safe mutex that disables interrupts before acquiring
//! the inner spinlock, preventing deadlocks when IRQ handlers need to
//! acquire the same lock as non-interrupt code.

use spin::Mutex;
use core::ops::{Deref, DerefMut};
use crate::arch::aarch64::cpu;

/// An IRQ-safe mutex wrapper around `spin::Mutex`.
///
/// Disables interrupts (via DAIF mask) before acquiring the inner lock
/// and restores the previous interrupt state when the guard is dropped.
/// This prevents deadlocks where a timer/UART IRQ fires while the lock
/// is already held by non-interrupt code.
pub struct IrqMutex<T> {
    inner: Mutex<T>,
}

/// RAII guard that restores DAIF on drop.
struct DaifRestore(u64);

impl Drop for DaifRestore {
    fn drop(&mut self) {
        cpu::restore_interrupts(self.0);
    }
}

/// Guard returned by `IrqMutex::lock()`.
/// Fields are dropped in declaration order:
///   1. `guard` — releases the spinlock
///   2. `_daif` — restores interrupt state
/// This ensures interrupts are re-enabled only AFTER the lock is released.
pub struct IrqMutexGuard<'a, T> {
    guard: spin::MutexGuard<'a, T>,
    _daif: DaifRestore,
}

impl<T> IrqMutex<T> {
    pub const fn new(value: T) -> Self {
        IrqMutex {
            inner: Mutex::new(value),
        }
    }

    /// Lock the mutex, disabling interrupts first.
    /// Returns a guard that restores interrupts on drop.
    pub fn lock(&self) -> IrqMutexGuard<'_, T> {
        let daif = cpu::save_and_disable_interrupts();
        let guard = self.inner.lock();
        IrqMutexGuard {
            guard,
            _daif: DaifRestore(daif),
        }
    }
}

// Safety: IrqMutex is safe to share across threads/cores since
// the inner spin::Mutex provides mutual exclusion and we disable
// interrupts to prevent re-entrant access from IRQ handlers.
unsafe impl<T: Send> Sync for IrqMutex<T> {}
unsafe impl<T: Send> Send for IrqMutex<T> {}

impl<'a, T> Deref for IrqMutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &*self.guard
    }
}

impl<'a, T> DerefMut for IrqMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut *self.guard
    }
}
