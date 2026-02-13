// Process Manager (Ps)
//
// Manages processes and threads — the fundamental units of execution.
//
// In CantayaOS (inspired by Windows NT):
//   - A Process is a container: address space, handles, security context
//   - A Thread is a unit of execution: register state, stack, scheduling priority
//   - One process can have multiple threads
//   - All threads in a process share the same address space
//
// Current status: Stub
// This module defines the data structures. Actual process creation/management
// will be implemented when we have virtual memory and syscalls working.
//
// Windows NT equivalent:
//   - EPROCESS (executive process block)
//   - ETHREAD (executive thread block)
//   - PsCreateProcess / PsCreateThread

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

/// Process ID type
pub type ProcessId = u32;

/// Thread ID type  
pub type ThreadId = u32;

/// Process state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Process is being created
    Creating,
    /// Process is active (has running or ready threads)
    Active,
    /// Process is being terminated
    Terminating,
    /// Process has exited
    Exited,
}

/// A process — container for threads and resources.
///
/// Each process has its own:
///   - Virtual address space (page tables)
///   - Open handles to kernel objects
///   - Security token
///   - One or more threads
#[derive(Debug)]
pub struct Process {
    /// Unique process ID
    pub id: ProcessId,
    /// Human-readable name (for debugging)
    pub name: String,
    /// Current state
    pub state: ProcessState,
    /// Page table root physical address (CR3 value for this process)
    pub page_table: u64,
    /// Thread IDs belonging to this process
    pub threads: Vec<ThreadId>,
    /// Exit code (valid only when state == Exited)
    pub exit_code: i32,
    /// Parent process ID (0 for the initial system process)
    pub parent_id: ProcessId,
}

/// Thread state (from the executive's perspective, not the scheduler's)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    /// Thread is being initialized
    Initializing,
    /// Thread is ready/running (managed by the scheduler)
    Running,
    /// Thread is waiting on a kernel object
    Waiting,
    /// Thread has terminated
    Terminated,
}

/// A thread — unit of CPU execution within a process.
#[derive(Debug)]
pub struct Thread {
    /// Unique thread ID
    pub id: ThreadId,
    /// Process this thread belongs to
    pub process_id: ProcessId,
    /// Current state
    pub state: ThreadState,
    /// Kernel stack base (allocated per-thread)
    pub kernel_stack_base: u64,
    /// Kernel stack size
    pub kernel_stack_size: u64,
    /// User stack base (in the process's address space)
    pub user_stack_base: u64,
    /// User stack size
    pub user_stack_size: u64,
    /// Scheduling priority (0-31, higher = more important)
    pub priority: u8,
}

// ============================================================================
// Process Manager API (stubs for now)
// ============================================================================

/// Create a new process. Returns the process ID.
///
/// Future: This will set up a new address space, create an initial thread,
/// and load the executable image.
pub fn create_process(_name: &str) -> Result<ProcessId, &'static str> {
    // TODO: Implement when virtual memory is ready
    Err("Process creation not yet implemented")
}

/// Terminate a process and all its threads.
pub fn terminate_process(_pid: ProcessId, _exit_code: i32) -> Result<(), &'static str> {
    // TODO: Implement when process management is ready
    Err("Process termination not yet implemented")
}

/// Create a new thread in the given process.
pub fn create_thread(
    _process_id: ProcessId,
    _entry_point: u64,
    _stack_size: u64,
) -> Result<ThreadId, &'static str> {
    // TODO: Implement when scheduling and address spaces are ready
    Err("Thread creation not yet implemented")
}
