//! Mutex Objects

use crate::sync::IrqMutex;
use alloc::collections::{BTreeMap, VecDeque};

extern crate alloc;

/// Kernel mutex object
pub struct KMutex {
    owner: Option<crate::process::Tid>,
    waiters: VecDeque<crate::process::Tid>,
}

impl KMutex {
    pub fn new() -> Self {
        KMutex {
            owner: None,
            waiters: VecDeque::new(),
        }
    }
    
    /// Acquire mutex
    pub fn acquire(&mut self, tid: crate::process::Tid) -> bool {
        if self.owner.is_none() {
            self.owner = Some(tid);
            true
        } else if self.owner == Some(tid) {
            // Recursive acquisition (not supported)
            false
        } else {
            self.waiters.push_back(tid);
            false
        }
    }
    
    /// Release mutex
    pub fn release(&mut self, tid: crate::process::Tid) -> bool {
        if self.owner == Some(tid) {
            if let Some(next) = self.waiters.pop_front() {
                self.owner = Some(next);
                crate::process::scheduler::ready(next);
            } else {
                self.owner = None;
            }
            true
        } else {
            false
        }
    }
}

static MUTEXES: IrqMutex<BTreeMap<u32, KMutex>> = IrqMutex::new(BTreeMap::new());
static NEXT_MUTEX_ID: IrqMutex<u32> = IrqMutex::new(1);

/// Create new mutex
pub fn create_mutex() -> u32 {
    let mut next_id = NEXT_MUTEX_ID.lock();
    let id = *next_id;
    *next_id += 1;
    drop(next_id);
    
    MUTEXES.lock().insert(id, KMutex::new());
    id
}

/// Acquire mutex
pub fn acquire_mutex(id: u32, tid: crate::process::Tid) -> bool {
    if let Some(mutex) = MUTEXES.lock().get_mut(&id) {
        mutex.acquire(tid)
    } else {
        false
    }
}

/// Release mutex
pub fn release_mutex(id: u32, tid: crate::process::Tid) -> bool {
    if let Some(mutex) = MUTEXES.lock().get_mut(&id) {
        mutex.release(tid)
    } else {
        false
    }
}

/// Wait to acquire mutex — returns true if acquired immediately.
/// If returns false, the thread has been added to the waiter list and should block.
pub fn wait_mutex(id: u32, tid: crate::process::Tid) -> bool {
    if let Some(mutex) = MUTEXES.lock().get_mut(&id) {
        mutex.acquire(tid)
    } else {
        true // invalid ID
    }
}
