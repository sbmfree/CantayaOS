// Preemptive Task Scheduler
//
// Implements preemptive multitasking for CantayaOS with:
//   - Priority-based scheduling (5 levels with quantum multipliers)
//   - FPU/SSE context saving via FXSAVE/FXRSTOR (512 bytes per task)
//   - Sleep/wake support with tick-based deadlines
//   - Deferred stack free to prevent use-after-free on task exit
//   - Stack guard page support
//
// Modeled after the Windows NT kernel dispatcher — each task represents
// a kernel thread with its own stack and saved CPU context.
//
// Context switching works by leveraging the timer ISR:
//   1. PIT fires IRQ0 → CPU pushes SS, RSP, RFLAGS, CS, RIP
//   2. Timer handler pushes all 15 GPRs (RAX..R15)
//   3. Handler calls timer_tick(current_rsp) → scheduler saves RSP into current task
//   4. Scheduler picks next Ready task, returns its saved RSP
//   5. Handler sets RSP to returned value, pops GPRs, IRETQ → now in new task
//
// Each spawned task gets its own stack allocated from the frame allocator.
// When a task is first created, a synthetic interrupt frame is placed on its stack
// so that the first context switch into it will IRETQ directly to its entry point.

use crate::hal::gdt::{KERNEL_CODE_SELECTOR, KERNEL_DATA_SELECTOR};
use crate::memory::frame_allocator;
use spin::Mutex;

// ---------------------------------------------------------------------------
// Deferred stack free — avoids use-after-free in exit_current_task
// ---------------------------------------------------------------------------

/// Deferred stack free: (stack_base_phys, page_count)
/// Set by exit_current_task(), consumed by timer_tick() after switching away.
static DEFERRED_FREE: Mutex<Option<(u64, usize)>> = Mutex::new(None);

/// Process any deferred stack frees. Called at the start of timer_tick.
fn process_deferred_free() {
    let mut deferred = DEFERRED_FREE.lock();
    if let Some((base, pages)) = deferred.take() {
        for p in 0..pages {
            frame_allocator::free_frame(base + (p as u64) * 4096);
        }
    }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum number of concurrent tasks
const MAX_TASKS: usize = 64;

/// Stack size per task: 5 pages = 20 KiB (4 usable + 1 guard)
const TASK_STACK_PAGES: usize = 5;
const TASK_STACK_SIZE: usize = TASK_STACK_PAGES * 4096;

/// Number of registers saved by the ISR stub (RAX..R15 = 15)
const SAVED_REGS: usize = 15;

/// Number of values in the CPU interrupt frame (RIP, CS, RFLAGS, RSP, SS = 5)
const IRET_FRAME: usize = 5;

/// Total u64 slots on the stack per context switch
const CONTEXT_SLOTS: usize = SAVED_REGS + IRET_FRAME; // 20

/// Base quantum in ticks (at Normal priority = 10ms at 1000 Hz)
const BASE_QUANTUM: u64 = 10;

/// Size of the FXSAVE area (512 bytes, must be 16-byte aligned)
const FXSAVE_SIZE: usize = 512;

// ---------------------------------------------------------------------------
// Task Priority
// ---------------------------------------------------------------------------

/// Task priority levels (inspired by Windows thread priorities)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum TaskPriority {
    /// Lowest priority — only runs when nothing else is ready
    Idle = 0,
    /// Below-normal priority
    Low = 1,
    /// Default priority for most tasks
    Normal = 2,
    /// Elevated priority for important tasks
    High = 3,
    /// Highest priority — preempts everything (use sparingly)
    Realtime = 4,
}

impl TaskPriority {
    /// Get the quantum multiplier for this priority level.
    /// Higher priority tasks get longer time slices.
    pub fn quantum(&self) -> u64 {
        match self {
            TaskPriority::Idle => 2,
            TaskPriority::Low => 5,
            TaskPriority::Normal => BASE_QUANTUM,
            TaskPriority::High => 20,
            TaskPriority::Realtime => 50,
        }
    }

    /// Parse a priority name string
    pub fn from_name(s: &str) -> Option<Self> {
        // Manual case-insensitive comparison for no_std
        let bytes = s.as_bytes();
        if bytes.len() == 4
            && (bytes[0] == b'i' || bytes[0] == b'I')
            && (bytes[1] == b'd' || bytes[1] == b'D')
            && (bytes[2] == b'l' || bytes[2] == b'L')
            && (bytes[3] == b'e' || bytes[3] == b'E')
        {
            return Some(TaskPriority::Idle);
        }
        if bytes.len() == 3
            && (bytes[0] == b'l' || bytes[0] == b'L')
            && (bytes[1] == b'o' || bytes[1] == b'O')
            && (bytes[2] == b'w' || bytes[2] == b'W')
        {
            return Some(TaskPriority::Low);
        }
        if bytes.len() == 6
            && (bytes[0] == b'n' || bytes[0] == b'N')
            && (bytes[1] == b'o' || bytes[1] == b'O')
            && (bytes[2] == b'r' || bytes[2] == b'R')
            && (bytes[3] == b'm' || bytes[3] == b'M')
            && (bytes[4] == b'a' || bytes[4] == b'A')
            && (bytes[5] == b'l' || bytes[5] == b'L')
        {
            return Some(TaskPriority::Normal);
        }
        if bytes.len() == 4
            && (bytes[0] == b'h' || bytes[0] == b'H')
            && (bytes[1] == b'i' || bytes[1] == b'I')
            && (bytes[2] == b'g' || bytes[2] == b'G')
            && (bytes[3] == b'h' || bytes[3] == b'H')
        {
            return Some(TaskPriority::High);
        }
        if bytes.len() == 8
            && (bytes[0] == b'r' || bytes[0] == b'R')
            && (bytes[1] == b'e' || bytes[1] == b'E')
            && (bytes[2] == b'a' || bytes[2] == b'A')
            && (bytes[3] == b'l' || bytes[3] == b'L')
            && (bytes[4] == b't' || bytes[4] == b'T')
            && (bytes[5] == b'i' || bytes[5] == b'I')
            && (bytes[6] == b'm' || bytes[6] == b'M')
            && (bytes[7] == b'e' || bytes[7] == b'E')
        {
            return Some(TaskPriority::Realtime);
        }
        if bytes.len() == 2
            && (bytes[0] == b'r' || bytes[0] == b'R')
            && (bytes[1] == b't' || bytes[1] == b'T')
        {
            return Some(TaskPriority::Realtime);
        }
        None
    }

    /// Get the name of this priority level
    pub fn name(&self) -> &'static str {
        match self {
            TaskPriority::Idle => "Idle",
            TaskPriority::Low => "Low",
            TaskPriority::Normal => "Normal",
            TaskPriority::High => "High",
            TaskPriority::Realtime => "Realtime",
        }
    }
}

// ---------------------------------------------------------------------------
// Task State
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// Slot is unused
    Empty,
    /// Task is ready to run (in the run queue)
    Ready,
    /// Task is currently executing on the CPU
    Running,
    /// Task is blocked/sleeping (waiting for I/O, sleep, etc.)
    Blocked,
    /// Task has terminated; resources can be reclaimed
    Exited,
}

// ---------------------------------------------------------------------------
// Task Control Block (TCB)
// ---------------------------------------------------------------------------

/// FPU/SSE context save area (512 bytes for FXSAVE)
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct FxSaveArea {
    pub data: [u8; FXSAVE_SIZE],
}

impl FxSaveArea {
    const fn new() -> Self {
        Self { data: [0u8; FXSAVE_SIZE] }
    }
}

/// Per-task metadata — analogous to Windows KTHREAD/ETHREAD
#[derive(Clone, Copy)]
pub struct Task {
    /// Unique task ID
    pub id: u32,
    /// Current scheduling state
    pub state: TaskState,
    /// Human-readable name (fixed-size for no_std simplicity)
    pub name: [u8; 32],
    /// Length of the name string
    pub name_len: usize,
    /// Saved RSP — points into the task's kernel stack where registers are saved
    pub saved_rsp: u64,
    /// Base address of the allocated stack (for deallocation on exit)
    pub stack_base: u64,
    /// Stack size in bytes
    pub stack_size: usize,
    /// Task priority level
    pub priority: TaskPriority,
    /// Remaining time-slice ticks before preemption
    pub quantum_remaining: u64,
    /// Total number of context switches into this task
    pub switches: u64,
    /// Total CPU ticks consumed
    pub cpu_ticks: u64,
    /// Wake time in system ticks (for sleep). 0 = not sleeping.
    pub wake_tick: u64,
    /// FPU/SSE context save area
    pub fxsave: FxSaveArea,
    /// Whether FPU context has been initialized for this task
    pub fpu_initialized: bool,
}

impl Task {
    const fn empty() -> Self {
        Self {
            id: 0,
            state: TaskState::Empty,
            name: [0u8; 32],
            name_len: 0,
            saved_rsp: 0,
            stack_base: 0,
            stack_size: 0,
            priority: TaskPriority::Normal,
            quantum_remaining: 0,
            switches: 0,
            cpu_ticks: 0,
            wake_tick: 0,
            fxsave: FxSaveArea::new(),
            fpu_initialized: false,
        }
    }

    /// Set the task name from a string slice.
    fn set_name(&mut self, n: &str) {
        let bytes = n.as_bytes();
        let len = bytes.len().min(32);
        self.name[..len].copy_from_slice(&bytes[..len]);
        self.name_len = len;
    }

    /// Get the task name as a &str.
    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("?")
    }
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

struct Scheduler {
    /// All task slots
    tasks: [Task; MAX_TASKS],
    /// Index of the currently running task
    current: usize,
    /// Next task ID to assign
    next_id: u32,
    /// Total context switches performed
    context_switches: u64,
    /// Whether the scheduler has been initialized
    initialized: bool,
}

impl Scheduler {
    const fn new() -> Self {
        Self {
            tasks: [Task::empty(); MAX_TASKS],
            current: 0,
            next_id: 1,
            context_switches: 0,
            initialized: false,
        }
    }

    /// Find the next runnable task using priority-based round-robin.
    ///
    /// Strategy: find the highest-priority Ready task. Among equal-priority
    /// tasks, use round-robin (start from current+1 and wrap around).
    fn pick_next(&self) -> Option<usize> {
        let mut best_priority = TaskPriority::Idle;
        let mut found_any = false;

        // First pass: find the highest priority among ready tasks
        for i in 0..MAX_TASKS {
            if self.tasks[i].state == TaskState::Ready && self.tasks[i].priority >= best_priority {
                best_priority = self.tasks[i].priority;
                found_any = true;
            }
        }

        if !found_any {
            return None;
        }

        // Second pass: among tasks at that priority, pick round-robin
        for offset in 1..=MAX_TASKS {
            let idx = (self.current + offset) % MAX_TASKS;
            if self.tasks[idx].state == TaskState::Ready
                && self.tasks[idx].priority == best_priority
            {
                return Some(idx);
            }
        }

        None
    }

    /// Wake any sleeping tasks whose deadline has passed
    fn wake_sleeping_tasks(&mut self, current_tick: u64) {
        for i in 0..MAX_TASKS {
            if self.tasks[i].state == TaskState::Blocked && self.tasks[i].wake_tick > 0 {
                if current_tick >= self.tasks[i].wake_tick {
                    self.tasks[i].state = TaskState::Ready;
                    self.tasks[i].wake_tick = 0;
                    self.tasks[i].quantum_remaining = self.tasks[i].priority.quantum();
                }
            }
        }
    }
}

static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

// ---------------------------------------------------------------------------
// FPU Context Save/Restore
// ---------------------------------------------------------------------------

/// Save the current FPU/SSE state into the given buffer
#[inline]
unsafe fn fxsave(area: &mut FxSaveArea) {
    core::arch::asm!(
        "fxsave [{}]",
        in(reg) area.data.as_mut_ptr(),
        options(nostack)
    );
}

/// Restore FPU/SSE state from the given buffer
#[inline]
unsafe fn fxrstor(area: &FxSaveArea) {
    core::arch::asm!(
        "fxrstor [{}]",
        in(reg) area.data.as_ptr(),
        options(nostack)
    );
}

/// Initialize the x87 FPU to default state
#[inline]
unsafe fn fninit() {
    core::arch::asm!("fninit", options(nostack));
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Initialize the scheduler. Creates task 0 for the current (kernel/shell) execution.
/// Must be called before enabling interrupts.
pub fn init() {
    let mut sched = SCHEDULER.lock();

    // Task 0 = the kernel boot thread (currently running)
    sched.tasks[0] = Task {
        id: 0,
        state: TaskState::Running,
        name: [0u8; 32],
        name_len: 0,
        saved_rsp: 0,     // will be filled by first timer_tick
        stack_base: 0,     // boot stack — not managed by us
        stack_size: 0,
        priority: TaskPriority::Normal,
        quantum_remaining: TaskPriority::Normal.quantum(),
        switches: 0,
        cpu_ticks: 0,
        wake_tick: 0,
        fxsave: FxSaveArea::new(),
        fpu_initialized: true, // Boot thread inherits the boot FPU state
    };
    sched.tasks[0].set_name("kernel");
    sched.next_id = 1;
    sched.initialized = true;

    // Save the initial FPU state for task 0
    unsafe {
        fxsave(&mut sched.tasks[0].fxsave);
    }
}

/// Timer tick handler — called from the timer ISR with the current RSP.
///
/// Saves the current task's RSP and FPU context, decrements quantum,
/// wakes sleeping tasks, and if quantum expired picks the next ready task.
/// Returns the RSP to restore (may be a different task's RSP).
///
/// Uses `try_lock` to avoid deadlock if the scheduler lock is already held.
pub fn timer_tick(current_rsp: u64) -> u64 {
    // Process any deferred stack frees from exit_current_task.
    process_deferred_free();

    // Get current tick for sleep/wake checks
    let current_tick = crate::shell::ticks();

    // try_lock avoids deadlock if someone holds the scheduler lock
    let mut sched = match SCHEDULER.try_lock() {
        Some(s) => s,
        None => return current_rsp,
    };

    if !sched.initialized {
        return current_rsp;
    }

    // Wake any sleeping tasks whose deadline has passed
    sched.wake_sleeping_tasks(current_tick);

    let cur = sched.current;

    // Save the current task's RSP
    sched.tasks[cur].saved_rsp = current_rsp;

    // Save FPU context for the current task
    if sched.tasks[cur].state != TaskState::Exited && sched.tasks[cur].state != TaskState::Empty {
        unsafe { fxsave(&mut sched.tasks[cur].fxsave); }
        sched.tasks[cur].fpu_initialized = true;
    }

    // Track CPU usage
    sched.tasks[cur].cpu_ticks += 1;

    // Decrement quantum
    if sched.tasks[cur].quantum_remaining > 0 {
        sched.tasks[cur].quantum_remaining -= 1;
    }

    // Check if we need to switch
    let needs_switch = sched.tasks[cur].quantum_remaining == 0
        || sched.tasks[cur].state == TaskState::Exited
        || sched.tasks[cur].state == TaskState::Blocked;

    if needs_switch {
        // Handle exited task
        if sched.tasks[cur].state == TaskState::Exited {
            sched.tasks[cur] = Task::empty();
        } else if sched.tasks[cur].state == TaskState::Running {
            sched.tasks[cur].state = TaskState::Ready;
            sched.tasks[cur].quantum_remaining = sched.tasks[cur].priority.quantum();
        }
        // Blocked tasks stay blocked (handled by wake_sleeping_tasks or explicit wake)

        // Pick next task
        if let Some(next) = sched.pick_next() {
            sched.tasks[next].state = TaskState::Running;
            sched.tasks[next].quantum_remaining = sched.tasks[next].priority.quantum();
            sched.tasks[next].switches += 1;
            sched.context_switches += 1;
            sched.current = next;

            // Restore FPU context for the new task
            if sched.tasks[next].fpu_initialized {
                unsafe { fxrstor(&sched.tasks[next].fxsave); }
            } else {
                unsafe { fninit(); }
                sched.tasks[next].fpu_initialized = true;
            }

            return sched.tasks[next].saved_rsp;
        } else {
            // No other task ready
            if sched.tasks[cur].state == TaskState::Empty {
                // Fall back to kernel task 0
                if sched.tasks[0].state == TaskState::Ready || sched.tasks[0].state == TaskState::Running {
                    sched.tasks[0].state = TaskState::Running;
                    sched.current = 0;
                    if sched.tasks[0].fpu_initialized {
                        unsafe { fxrstor(&sched.tasks[0].fxsave); }
                    }
                    return sched.tasks[0].saved_rsp;
                }
            } else if sched.tasks[cur].state != TaskState::Blocked {
                sched.tasks[cur].state = TaskState::Running;
                sched.tasks[cur].quantum_remaining = sched.tasks[cur].priority.quantum();
            }
        }
    }

    current_rsp
}

/// Spawn a new kernel task.
///
/// Allocates a stack (with guard page at bottom), sets up a synthetic
/// interrupt frame so the first context switch into it will IRETQ to `entry_point`.
///
/// Returns the new task's ID, or None if no slots / out of memory.
pub fn spawn(name: &str, entry_point: fn() -> !) -> Option<u32> {
    spawn_with_priority(name, entry_point, TaskPriority::Normal)
}

/// Spawn a new kernel task with a specific priority.
pub fn spawn_with_priority(name: &str, entry_point: fn() -> !, priority: TaskPriority) -> Option<u32> {
    let mut sched = SCHEDULER.lock();

    // Find an empty slot
    let slot = (0..MAX_TASKS).find(|&i| sched.tasks[i].state == TaskState::Empty)?;

    // Allocate stack frames (includes 1 guard page at the bottom)
    let stack_phys = frame_allocator::allocate_contiguous_frames(TASK_STACK_PAGES)?;
    let stack_base = stack_phys;

    // Guard page: the bottom page of the stack is not used for data.
    // In a full VMM, we'd unmap it. For now, we just don't use it.
    let usable_stack_top = stack_base + TASK_STACK_SIZE as u64;

    // Build a synthetic interrupt frame on the new stack.
    let frame_base = usable_stack_top - (CONTEXT_SLOTS as u64) * 8;
    let frame_ptr = frame_base as *mut u64;

    unsafe {
        // GPRs (offsets 0..14 from frame_base) — R15, R14, ..., RAX
        for i in 0..SAVED_REGS {
            frame_ptr.add(i).write(0u64);
        }

        // IRET frame (offsets 15..19)
        frame_ptr.add(15).write(entry_point as u64);             // RIP
        frame_ptr.add(16).write(KERNEL_CODE_SELECTOR as u64);    // CS
        frame_ptr.add(17).write(0x200u64);                       // RFLAGS (IF=1)
        frame_ptr.add(18).write(usable_stack_top);               // RSP after iret
        frame_ptr.add(19).write(KERNEL_DATA_SELECTOR as u64);    // SS
    }

    let task_id = sched.next_id;
    sched.next_id += 1;

    sched.tasks[slot] = Task {
        id: task_id,
        state: TaskState::Ready,
        name: [0u8; 32],
        name_len: 0,
        saved_rsp: frame_base,
        stack_base,
        stack_size: TASK_STACK_SIZE,
        priority,
        quantum_remaining: priority.quantum(),
        switches: 0,
        cpu_ticks: 0,
        wake_tick: 0,
        fxsave: FxSaveArea::new(),
        fpu_initialized: false, // Will be initialized on first switch
    };
    sched.tasks[slot].set_name(name);

    log::info!("Spawned task {} '{}' (slot {}, priority={})", task_id, name, slot, priority.name());

    Some(task_id)
}

/// Terminate the currently running task.
/// Defers stack deallocation until after the scheduler switches away.
pub fn exit_current_task() -> ! {
    {
        let mut sched = SCHEDULER.lock();
        let cur = sched.current;
        sched.tasks[cur].state = TaskState::Exited;

        // Schedule deferred free
        if sched.tasks[cur].stack_base != 0 {
            let base = sched.tasks[cur].stack_base;
            let pages = sched.tasks[cur].stack_size / 4096;
            *DEFERRED_FREE.lock() = Some((base, pages));
        }
    }

    loop {
        unsafe { core::arch::asm!("hlt"); }
    }
}

/// Kill a task by ID. Returns true if the task was found and killed.
/// Cannot kill task 0 (the kernel task).
pub fn kill(task_id: u32) -> bool {
    if task_id == 0 {
        return false;
    }

    let mut sched = SCHEDULER.lock();
    for i in 0..MAX_TASKS {
        if sched.tasks[i].id == task_id
            && sched.tasks[i].state != TaskState::Empty
            && sched.tasks[i].state != TaskState::Exited
        {
            sched.tasks[i].state = TaskState::Exited;

            // Free the stack
            if sched.tasks[i].stack_base != 0 {
                let base = sched.tasks[i].stack_base;
                let pages = sched.tasks[i].stack_size / 4096;
                for p in 0..pages {
                    frame_allocator::free_frame(base + (p as u64) * 4096);
                }
            }

            sched.tasks[i] = Task::empty();
            log::info!("Killed task {}", task_id);
            return true;
        }
    }
    false
}

/// Put the current task to sleep for `ms` milliseconds.
///
/// The task is marked Blocked with a wake_tick deadline.
/// When timer_tick() sees the deadline has passed, it marks the task Ready.
pub fn sleep_ms(ms: u64) {
    let current_tick = crate::shell::ticks();
    let wake_at = current_tick + ms; // 1 tick = 1ms at 1000 Hz

    {
        let mut sched = SCHEDULER.lock();
        let cur = sched.current;
        sched.tasks[cur].state = TaskState::Blocked;
        sched.tasks[cur].wake_tick = wake_at;
        sched.tasks[cur].quantum_remaining = 0; // Force reschedule
    }

    // Wait for timer to wake us and schedule us back
    loop {
        if let Some(sched) = SCHEDULER.try_lock() {
            if sched.tasks[sched.current].state == TaskState::Running {
                break;
            }
        }
        unsafe { core::arch::asm!("hlt"); }
    }
}

/// Set the priority of a task by ID. Returns true if successful.
pub fn set_priority(task_id: u32, priority: TaskPriority) -> bool {
    let mut sched = SCHEDULER.lock();
    for i in 0..MAX_TASKS {
        if sched.tasks[i].id == task_id
            && sched.tasks[i].state != TaskState::Empty
            && sched.tasks[i].state != TaskState::Exited
        {
            sched.tasks[i].priority = priority;
            sched.tasks[i].quantum_remaining = priority.quantum();
            log::info!("Task {} priority set to {}", task_id, priority.name());
            return true;
        }
    }
    false
}

/// Get the currently running task's ID.
pub fn current_task_id() -> u32 {
    match SCHEDULER.try_lock() {
        Some(sched) => sched.tasks[sched.current].id,
        None => 0,
    }
}

/// Get a snapshot of all active tasks for display purposes.
/// Returns (id, state, name, switches, priority, cpu_ticks).
pub fn task_list() -> alloc::vec::Vec<(u32, TaskState, alloc::string::String, u64, TaskPriority, u64)> {
    let sched = SCHEDULER.lock();
    let mut list = alloc::vec::Vec::new();

    for i in 0..MAX_TASKS {
        let t = &sched.tasks[i];
        match t.state {
            TaskState::Empty | TaskState::Exited => continue,
            _ => {
                list.push((
                    t.id,
                    t.state,
                    alloc::string::String::from(t.name_str()),
                    t.switches,
                    t.priority,
                    t.cpu_ticks,
                ));
            }
        }
    }
    list
}

/// Get the total number of context switches.
pub fn context_switch_count() -> u64 {
    match SCHEDULER.try_lock() {
        Some(sched) => sched.context_switches,
        None => 0,
    }
}

/// Get the number of active (non-empty, non-exited) tasks.
pub fn active_task_count() -> usize {
    match SCHEDULER.try_lock() {
        Some(sched) => {
            sched.tasks.iter().filter(|t| {
                t.state != TaskState::Empty && t.state != TaskState::Exited
            }).count()
        }
        None => 0,
    }
}

/// Yield the current task's remaining quantum, allowing an immediate reschedule.
pub fn yield_now() {
    if let Some(mut sched) = SCHEDULER.try_lock() {
        let cur = sched.current;
        sched.tasks[cur].quantum_remaining = 0;
    }
}
