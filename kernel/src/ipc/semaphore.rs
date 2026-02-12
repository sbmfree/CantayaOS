//! Semaphore Objects
//!
//! Counting semaphore for producer/consumer synchronization.

use crate::sync::IrqMutex;
use alloc::collections::{BTreeMap, VecDeque};

extern crate alloc;

/// Kernel semaphore object
pub struct Semaphore {
    count: u32,
    max_count: u32,
    waiters: VecDeque<crate::process::Tid>,
}

impl Semaphore {
    pub fn new(max_count: u32) -> Self {
        Semaphore {
            count: max_count,
            max_count,
            waiters: VecDeque::new(),
        }
    }

    /// Acquire (wait/P operation) — decrements the count.
    /// Returns true if acquired immediately, false if the caller must block.
    pub fn acquire(&mut self, tid: crate::process::Tid) -> bool {
        if self.count > 0 {
            self.count -= 1;
            true
        } else {
            self.waiters.push_back(tid);
            false
        }
    }

    /// Release (signal/V operation) — increments the count.
    /// Wakes one waiter if any are blocked.
    pub fn release(&mut self) -> bool {
        if let Some(tid) = self.waiters.pop_front() {
            // Wake the waiter directly (count stays at 0 — the woken thread "consumes" the signal)
            crate::process::scheduler::ready(tid);
            true
        } else if self.count < self.max_count {
            self.count += 1;
            true
        } else {
            false // already at max
        }
    }

    /// Get current count
    pub fn count(&self) -> u32 {
        self.count
    }
}

static SEMAPHORES: IrqMutex<BTreeMap<u32, Semaphore>> = IrqMutex::new(BTreeMap::new());
static NEXT_SEM_ID: IrqMutex<u32> = IrqMutex::new(1);

/// Create a new semaphore with the given maximum count
pub fn create_semaphore(max_count: u32) -> u32 {
    let mut next_id = NEXT_SEM_ID.lock();
    let id = *next_id;
    *next_id += 1;
    drop(next_id);

    SEMAPHORES.lock().insert(id, Semaphore::new(max_count));
    id
}

/// Acquire semaphore
pub fn acquire_semaphore(id: u32, tid: crate::process::Tid) -> bool {
    if let Some(sem) = SEMAPHORES.lock().get_mut(&id) {
        sem.acquire(tid)
    } else {
        false
    }
}

/// Release semaphore
pub fn release_semaphore(id: u32) -> bool {
    if let Some(sem) = SEMAPHORES.lock().get_mut(&id) {
        sem.release()
    } else {
        false
    }
}

/// Destroy a semaphore
pub fn destroy_semaphore(id: u32) {
    SEMAPHORES.lock().remove(&id);
}

/// Wait to acquire semaphore — returns true if acquired immediately.
/// If returns false, the thread has been added to the waiter list and should block.
pub fn wait_semaphore(id: u32, tid: crate::process::Tid) -> bool {
    if let Some(sem) = SEMAPHORES.lock().get_mut(&id) {
        sem.acquire(tid)
    } else {
        true // invalid ID
    }
}
