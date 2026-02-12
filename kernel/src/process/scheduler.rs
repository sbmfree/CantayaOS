//! Scheduler - Windows-like priority-based preemptive scheduler

use crate::sync::IrqMutex;
use crate::arch::aarch64::exceptions::ExceptionContext;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use super::{Tid, ProcessState, ThreadContext, THREAD_TABLE};

extern crate alloc;

/// Priority levels (Windows-like)
pub const PRIORITY_IDLE: u8 = 0;
pub const PRIORITY_LOWEST: u8 = 4;
pub const PRIORITY_BELOW_NORMAL: u8 = 6;
pub const PRIORITY_NORMAL: u8 = 8;
pub const PRIORITY_ABOVE_NORMAL: u8 = 10;
pub const PRIORITY_HIGHEST: u8 = 12;
pub const PRIORITY_TIME_CRITICAL: u8 = 15;
pub const PRIORITY_REALTIME: u8 = 24;

/// Number of priority levels
const NUM_PRIORITIES: usize = 32;

/// Entry in the sleep queue — sorted by wakeup_tick ascending
struct SleepEntry {
    tid: Tid,
    wakeup_tick: u64,
}

/// Scheduler state
struct Scheduler {
    ready_queues: [VecDeque<Tid>; NUM_PRIORITIES],
    current_thread: Option<Tid>,
    quantum_remaining: u32,
    /// Flag indicating if scheduler is active (has started running threads)
    active: bool,
    /// Threads sleeping until a specific tick count
    sleep_queue: Vec<SleepEntry>,
}

impl Scheduler {
    const fn new() -> Self {
        const EMPTY_QUEUE: VecDeque<Tid> = VecDeque::new();
        Scheduler {
            ready_queues: [EMPTY_QUEUE; NUM_PRIORITIES],
            current_thread: None,
            quantum_remaining: 0,
            active: false,
            sleep_queue: Vec::new(),
        }
    }
    
    fn add_thread(&mut self, tid: Tid, priority: u8) {
        let priority = (priority as usize).min(NUM_PRIORITIES - 1);
        self.ready_queues[priority].push_back(tid);
    }
    
    fn select_next(&mut self) -> Option<Tid> {
        // Find highest priority non-empty queue
        for priority in (0..NUM_PRIORITIES).rev() {
            if let Some(tid) = self.ready_queues[priority].pop_front() {
                return Some(tid);
            }
        }
        None
    }
}

static SCHEDULER: IrqMutex<Scheduler> = IrqMutex::new(Scheduler::new());

/// Default time quantum (in timer ticks)
const DEFAULT_QUANTUM: u32 = 6;

/// Initialize scheduler
pub fn init() {
    // Nothing to do yet
}

/// Start the scheduler (mark as active, allowing preemption)
/// Sets the boot thread (TID 0) as the current thread so its context
/// is saved properly on the first preemptive switch.
pub fn start() {
    let mut sched = SCHEDULER.lock();
    sched.active = true;
    sched.current_thread = Some(super::BOOT_TID);
    sched.quantum_remaining = DEFAULT_QUANTUM;
}

/// Check if scheduler is active
pub fn is_active() -> bool {
    SCHEDULER.lock().active
}

/// Add thread to ready queue
pub fn ready(tid: Tid) {
    if let Some(thread) = THREAD_TABLE.lock().get(&tid) {
        SCHEDULER.lock().add_thread(tid, thread.priority);
    }
}

/// Timer tick with preemption support
/// Returns true if a context switch should occur
pub fn tick_preemptive(ctx: &mut ExceptionContext) -> bool {
    // Only preempt user-mode (EL0) threads.
    // Kernel threads at EL1 share the kernel stack via SP_EL1, which the
    // exception frame save/restore does NOT switch, so preemptive context
    // switching between EL1 threads would corrupt the stack.
    // EL0 threads are safe because they use SP_EL0 (saved in ExceptionContext.sp).
    let spsr_el = ctx.pstate & 0x0F; // SPSR_EL1.M[3:0]
    if spsr_el != 0x0 {
        // Interrupted thread was at EL1 — do not preempt, but still wake sleepers
        wake_expired_sleepers();
        return false;
    }

    wake_expired_sleepers();

    let mut sched = SCHEDULER.lock();
    
    if !sched.active {
        return false;
    }
    
    // If no current thread or quantum at 0, try to schedule immediately
    if sched.current_thread.is_none() || sched.quantum_remaining == 0 {
        drop(sched);
        return schedule_preemptive(ctx);
    }
    
    sched.quantum_remaining -= 1;
    if sched.quantum_remaining == 0 {
        // Quantum expired, reschedule
        drop(sched);
        return schedule_preemptive(ctx);
    }
    false
}

/// Check the sleep queue and wake threads whose deadline has expired
fn wake_expired_sleepers() {
    let current_tick = *crate::hal::timer::SYSTEM_TICKS.lock();
    let mut sched = SCHEDULER.lock();
    
    // Drain expired entries from the sleep queue
    let mut i = 0;
    while i < sched.sleep_queue.len() {
        if sched.sleep_queue[i].wakeup_tick <= current_tick {
            let entry = sched.sleep_queue.swap_remove(i);
            // Mark thread as Ready and add to ready queue
            if let Some(thread) = THREAD_TABLE.lock().get_mut(&entry.tid) {
                if thread.state == ProcessState::Waiting {
                    thread.state = ProcessState::Ready;
                    sched.add_thread(entry.tid, thread.priority);
                }
            }
            // Don't increment i — swap_remove moved the last element here
        } else {
            i += 1;
        }
    }
}

/// Preemptive schedule - called from timer IRQ with ExceptionContext
/// Saves current thread's context and loads next thread's context into ctx
/// Returns true if a switch happened
pub fn schedule_preemptive(ctx: &mut ExceptionContext) -> bool {
    let mut sched = SCHEDULER.lock();
    
    // Save current thread's context
    if let Some(current_tid) = sched.current_thread {
        if let Some(thread) = THREAD_TABLE.lock().get_mut(&current_tid) {
            if thread.state == ProcessState::Running {
                // Save exception context to thread context
                save_ctx_to_thread(&mut thread.context, ctx);
                thread.state = ProcessState::Ready;
                sched.add_thread(current_tid, thread.priority);
            }
        }
    }
    
    // Select next thread
    if let Some(next_tid) = sched.select_next() {
        sched.current_thread = Some(next_tid);
        sched.quantum_remaining = DEFAULT_QUANTUM;
        
        // Load next thread's context and mark as running
        let next_pid;
        if let Some(thread) = THREAD_TABLE.lock().get_mut(&next_tid) {
            thread.state = ProcessState::Running;
            load_thread_to_ctx(ctx, &thread.context);
            next_pid = thread.pid;
        } else {
            return false;
        }
        
        // Switch address space to the next process's page tables
        let pgd = crate::process::get_process_pgd(next_pid);
        if pgd != 0 {
            crate::mm::virtual_mem::switch_page_tables(pgd);
        }
        
        return true;
    }
    
    false
}

/// Save ExceptionContext to ThreadContext
fn save_ctx_to_thread(thread: &mut ThreadContext, exc: &ExceptionContext) {
    thread.regs.copy_from_slice(&exc.regs);
    thread.sp = exc.sp;
    thread.pc = exc.pc;
    thread.pstate = exc.pstate;
}

/// Load ThreadContext into ExceptionContext (for context switch)
fn load_thread_to_ctx(exc: &mut ExceptionContext, thread: &ThreadContext) {
    exc.regs.copy_from_slice(&thread.regs);
    exc.sp = thread.sp;
    exc.pc = thread.pc;
    exc.pstate = thread.pstate;
}

/// Timer tick - called from timer interrupt (legacy, calls preemptive version)
pub fn tick() {
    // This is now a no-op for backwards compatibility
    // Real preemption happens via tick_preemptive() called from IRQ handler
}

/// Select next thread and switch (cooperative version for voluntary yields)
pub fn schedule() {
    let mut sched = SCHEDULER.lock();
    
    // Put current thread back in ready queue
    if let Some(current) = sched.current_thread {
        if let Some(thread) = THREAD_TABLE.lock().get(&current) {
            if thread.state == ProcessState::Running {
                sched.add_thread(current, thread.priority);
            }
        }
    }
    
    // Select next thread
    if let Some(next) = sched.select_next() {
        let old_tid = sched.current_thread;
        sched.current_thread = Some(next);
        sched.quantum_remaining = DEFAULT_QUANTUM;
        
        // Mark thread as running and get its process PID
        let next_pid;
        if let Some(thread) = THREAD_TABLE.lock().get_mut(&next) {
            thread.state = ProcessState::Running;
            next_pid = thread.pid;
        } else {
            return;
        }
        
        // Switch address space to the next process's page tables
        let pgd = crate::process::get_process_pgd(next_pid);
        if pgd != 0 {
            crate::mm::virtual_mem::switch_page_tables(pgd);
        }
        
        // For cooperative scheduling, we need to do actual context switch
        // This requires both threads to have valid contexts
        if let Some(old) = old_tid {
            if old != next {
                drop(sched);
                do_cooperative_switch(old, next);
            }
        }
    }
}

/// Perform cooperative context switch between two threads
fn do_cooperative_switch(old_tid: Tid, new_tid: Tid) {
    // Get raw pointers to thread contexts
    let table = THREAD_TABLE.lock();
    
    let old_ctx = match table.get(&old_tid) {
        Some(t) => &t.context as *const ThreadContext as *mut ThreadContext,
        None => return,
    };
    
    let new_ctx = match table.get(&new_tid) {
        Some(t) => &t.context as *const ThreadContext,
        None => return,
    };
    
    drop(table);
    
    // SAFETY: We've locked, gotten pointers, and dropped the lock.
    // context_switch will not access THREAD_TABLE.
    unsafe {
        super::thread::context_switch(old_ctx, new_ctx);
    }
    
    // When this thread is rescheduled and context_switch returns here,
    // interrupts may have been left masked (e.g. if we were switched out
    // inside an SVC handler where DAIF was set on exception entry).
    // Re-enable IRQs so timer ticks and UART interrupts continue to work.
    unsafe { core::arch::asm!("msr daifclr, #0xf"); }
}

/// Get current thread ID
pub fn current_tid() -> Option<Tid> {
    SCHEDULER.lock().current_thread
}

/// Yield current thread
pub fn yield_thread() {
    schedule();
}

/// Block current thread
pub fn block_current() {
    {
        let sched = SCHEDULER.lock();
        if let Some(tid) = sched.current_thread {
            if let Some(thread) = THREAD_TABLE.lock().get_mut(&tid) {
                thread.state = ProcessState::Waiting;
            }
        }
    }
    schedule();
}

/// Put the current thread to sleep for `ticks` timer ticks (each tick ≈ 10ms).
/// The thread is moved to Waiting state and added to the sleep queue.
/// It will be woken up by `wake_expired_sleepers()` in the timer IRQ.
pub fn sleep_current_ticks(ticks: u64) {
    {
        let current_tick = *crate::hal::timer::SYSTEM_TICKS.lock();
        let mut sched = SCHEDULER.lock();
        if let Some(tid) = sched.current_thread {
            if let Some(thread) = THREAD_TABLE.lock().get_mut(&tid) {
                thread.state = ProcessState::Waiting;
            }
            sched.sleep_queue.push(SleepEntry {
                tid,
                wakeup_tick: current_tick + ticks,
            });
        }
    }
    schedule();
}

/// Put the current thread to sleep for `ms` milliseconds.
pub fn sleep_current_ms(ms: u64) {
    // Each tick is TIMER_INTERVAL_MS (10ms)
    let ticks = (ms + 9) / 10; // round up
    if ticks == 0 {
        yield_thread();
    } else {
        sleep_current_ticks(ticks);
    }
}
