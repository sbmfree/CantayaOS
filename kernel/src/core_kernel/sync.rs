// Synchronization Primitives
//
// Kernel synchronization primitives for safe concurrent access to shared data.
//
// Current implementation:
//   - SpinLock: busy-waiting lock for short critical sections
//     (We re-export `spin::Mutex` for convenience)
//
// Future additions:
//   - Mutex (sleeping lock — blocks the calling thread instead of spinning)
//   - Semaphore (counting synchronization)
//   - Event (signaling between threads)
//   - RwLock (readers-writer lock)
//
// In Windows NT, these are the Dispatcher Objects:
//   - KSPIN_LOCK → our SpinLock
//   - KMUTEX → our future Mutex
//   - KSEMAPHORE → our future Semaphore
//   - KEVENT → our future Event
//
// Design Decision:
//   We use spinlocks initially because we don't have a scheduler that can
//   block threads yet. Once the scheduler supports blocking, we'll add
//   proper sleeping mutexes for longer critical sections.

pub use spin::Mutex as SpinLock;
pub use spin::RwLock as SpinRwLock;

/// A simple ticket-based spinlock for fair ordering.
///
/// Unlike a basic test-and-set spinlock, a ticket lock guarantees FIFO ordering,
/// preventing starvation when multiple CPUs/threads compete for the lock.
///
/// Reserved for future multi-core support.
pub struct TicketLock<T> {
    next_ticket: core::sync::atomic::AtomicU64,
    now_serving: core::sync::atomic::AtomicU64,
    data: spin::Mutex<T>,
}

impl<T> TicketLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            next_ticket: core::sync::atomic::AtomicU64::new(0),
            now_serving: core::sync::atomic::AtomicU64::new(0),
            data: spin::Mutex::new(data),
        }
    }
}
