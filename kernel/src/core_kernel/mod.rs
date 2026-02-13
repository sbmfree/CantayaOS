// Kernel Core Module
//
// This layer sits between the HAL (below) and the Executive (above).
// It provides fundamental kernel services:
//   - Scheduler: preemptive task scheduling
//   - Synchronization primitives: spinlocks, mutexes (future)
//
// In Windows NT, the kernel core (ke) provides:
//   - Thread scheduling and context switching
//   - Dispatcher objects (events, mutexes, semaphores)
//   - DPC (Deferred Procedure Calls)
//   - APC (Asynchronous Procedure Calls)
//
// We start with a basic round-robin scheduler and will expand from there.

pub mod scheduler;
pub mod sync;
