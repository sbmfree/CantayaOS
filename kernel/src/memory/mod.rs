// Memory Management Subsystem
//
// This module implements the kernel's memory management system, organized in layers:
//
//   1. Frame Allocator — manages physical memory pages (4 KiB frames)
//      Uses a bitmap to track which frames are free/used.
//
//   2. Heap Allocator — provides dynamic allocation (alloc::Box, Vec, etc.)
//      Uses a linked-list free-list allocator on top of the frame allocator.
//
//   3. Paging — x86_64 4-level page table manipulation (map/unmap/translate)
//
//   4. VMM (Virtual Memory Manager) — virtual address space management,
//      tracks allocated regions, provides alloc_and_map / unmap_and_free.
//
// In Windows NT, this is handled by the Memory Manager (Mm) component:
//   - MmInitializeMemoryManager() sets up the PFN database (our frame allocator)
//   - Pool allocator provides kernel heap (NonPagedPool/PagedPool)
//   - VAD tree tracks per-process virtual address ranges
//
// Initialization order:
//   1. Initialize frame allocator from the UEFI memory map
//   2. Allocate initial heap pages using the frame allocator (identity-mapped)
//   3. Initialize the heap allocator over those pages
//   4. Initialize the VMM (captures CR3, sets up virtual region tracking)
//   5. (Future) Re-map the heap to HEAP_REGION_START via VMM

pub mod frame_allocator;
pub mod heap;
pub mod paging;
pub mod vmm;

use cantaya_shared::boot_info::BootInfo;

/// Size of a single memory page/frame (4 KiB)
pub const PAGE_SIZE: u64 = 4096;

/// Initialize the memory management subsystem.
///
/// This must be called after HAL init but before anything that allocates memory.
pub fn init(boot_info: &'static BootInfo) {
    // Step 1: Initialize the physical frame allocator
    frame_allocator::init(&boot_info.memory_map);
    log::info!(
        "Frame allocator: {} free frames ({} MiB)",
        frame_allocator::free_frame_count(),
        frame_allocator::free_frame_count() * 4 / 1024
    );

    // Step 2: Initialize the kernel heap (uses identity-mapped physical frames)
    heap::init();
    log::info!("Kernel heap initialized");

    // Step 3: Initialize the VMM (captures CR3, prepares virtual region tracking)
    vmm::init();
    log::info!("Virtual memory manager initialized");
}
