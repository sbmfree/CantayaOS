//! Memory Management Subsystem
//! 
//! Windows-like memory manager with virtual memory, paging, and heap

pub mod physical;
pub mod virtual_mem;
pub mod heap;

/// Memory region descriptor
#[derive(Clone, Copy, Debug)]
pub struct MemoryRegion {
    pub base: usize,
    pub size: usize,
    pub kind: MemoryKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MemoryKind {
    Available,
    Reserved,
    Kernel,
    PageTables,
}

/// Initialize memory management
pub fn init() {
    // 1. Physical frame allocator
    physical::init();
    
    // 2. Virtual memory (page tables, kernel mappings)
    virtual_mem::init();
    
    // 3. Enable MMU now that page tables are set up
    crate::arch::mmu::init();
    
    // 4. Kernel heap allocator (works now with MMU + page tables)
    heap::init();
}

/// Handle page fault. Returns `true` if resolved, `false` if unresolvable.
pub fn handle_page_fault(address: u64) -> bool {
    virtual_mem::handle_fault(address)
}
