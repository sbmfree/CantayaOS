//! Inter-Process Communication
//! 
//! Windows-like IPC mechanisms: Events, Mutexes, Semaphores, Pipes

pub mod event;
pub mod mutex;
pub mod semaphore;
pub mod pipe;

/// Initialize IPC subsystem
pub fn init() {
}
