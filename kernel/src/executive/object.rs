// Object Manager (Ob)
//
// The Object Manager provides a unified namespace and handle-based access
// to all kernel resources (processes, threads, files, devices, etc.)
//
// In Windows NT, every kernel resource is an "object" with:
//   - A type (Process, Thread, File, Event, Mutex, etc.)
//   - A reference count
//   - An optional name (in the \ObjectManager namespace)
//   - A security descriptor
//   - A handle table entry for user-mode access
//
// User-mode code never gets raw pointers to kernel objects.
// Instead, they get "handles" — opaque integers that the Object Manager
// translates to kernel pointers. This provides:
//   1. Security: handles are validated before use
//   2. Abstraction: user code doesn't know about kernel memory layout
//   3. Lifecycle management: objects are ref-counted and cleaned up automatically
//
// Current status: Stub
// We define the core types here; implementation comes with user-mode support.

/// Handle type — an opaque reference to a kernel object
///
/// Handles are process-local. Handle 5 in process A refers to a completely
/// different object than handle 5 in process B.
pub type Handle = u32;

/// Special handle values
pub const INVALID_HANDLE: Handle = 0;

/// Kernel object types
///
/// Every kernel resource has a type that determines what operations
/// are valid on it and how it's cleaned up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectType {
    /// A process
    Process,
    /// A thread
    Thread,
    /// An open file
    File,
    /// A directory in the namespace
    Directory,
    /// A synchronization event
    Event,
    /// A mutual exclusion semaphore
    Mutex,
    /// A kernel timer
    Timer,
    /// A device object (for drivers)
    Device,
    /// A section object (memory-mapped file/shared memory)
    Section,
}

/// A kernel object header — present at the start of every kernel object.
///
/// This provides common functionality (reference counting, naming, security)
/// without each object type having to implement it from scratch.
#[derive(Debug)]
pub struct ObjectHeader {
    /// Type of this object
    pub object_type: ObjectType,
    /// Reference count — object is freed when this reaches 0
    pub reference_count: u32,
    /// Optional name for the object (for named objects in the namespace)
    pub name: Option<&'static str>,
}

impl ObjectHeader {
    /// Create a new object header
    pub fn new(object_type: ObjectType) -> Self {
        Self {
            object_type,
            reference_count: 1,
            name: None,
        }
    }

    /// Increment the reference count
    pub fn add_ref(&mut self) -> u32 {
        self.reference_count += 1;
        self.reference_count
    }

    /// Decrement the reference count. Returns true if the object should be deleted.
    pub fn release(&mut self) -> bool {
        self.reference_count -= 1;
        self.reference_count == 0
    }
}
