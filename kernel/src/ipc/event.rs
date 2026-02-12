//! Event Objects (Windows-like synchronization)

use crate::sync::IrqMutex;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

extern crate alloc;

/// Event object
pub struct Event {
    signaled: bool,
    auto_reset: bool,
    waiters: Vec<crate::process::Tid>,
}

impl Event {
    pub fn new(auto_reset: bool) -> Self {
        Event {
            signaled: false,
            auto_reset,
            waiters: Vec::new(),
        }
    }
    
    /// Set event to signaled state
    pub fn set(&mut self) {
        self.signaled = true;
        
        if self.auto_reset {
            // Wake one waiter
            if let Some(tid) = self.waiters.pop() {
                crate::process::scheduler::ready(tid);
                self.signaled = false;
            }
        } else {
            // Wake all waiters
            for tid in self.waiters.drain(..) {
                crate::process::scheduler::ready(tid);
            }
        }
    }
    
    /// Reset event to non-signaled state
    pub fn reset(&mut self) {
        self.signaled = false;
    }
    
    /// Wait for event
    pub fn wait(&mut self, tid: crate::process::Tid) -> bool {
        if self.signaled {
            if self.auto_reset {
                self.signaled = false;
            }
            true
        } else {
            self.waiters.push(tid);
            false
        }
    }
    /// Check if signaled without consuming or blocking
    pub fn is_signaled(&self) -> bool {
        self.signaled
    }
}

static EVENTS: IrqMutex<BTreeMap<u32, Event>> = IrqMutex::new(BTreeMap::new());
static NEXT_EVENT_ID: IrqMutex<u32> = IrqMutex::new(1);

/// Create new event
pub fn create_event(auto_reset: bool) -> u32 {
    let mut next_id = NEXT_EVENT_ID.lock();
    let id = *next_id;
    *next_id += 1;
    drop(next_id);
    
    EVENTS.lock().insert(id, Event::new(auto_reset));
    id
}

/// Set event
pub fn set_event(id: u32) -> bool {
    if let Some(event) = EVENTS.lock().get_mut(&id) {
        event.set();
        true
    } else {
        false
    }
}

/// Reset event
pub fn reset_event(id: u32) -> bool {
    if let Some(event) = EVENTS.lock().get_mut(&id) {
        event.reset();
        true
    } else {
        false
    }
}

/// Wait on an event — returns true if signaled immediately.
/// If returns false, the thread has been added to the waiter list and should block.
pub fn wait_event(id: u32, tid: crate::process::Tid) -> bool {
    if let Some(event) = EVENTS.lock().get_mut(&id) {
        event.wait(tid)
    } else {
        true // invalid ID — don't block, caller sees failure via status
    }
}
