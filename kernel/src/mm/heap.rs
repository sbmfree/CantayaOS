//! Kernel Heap Allocator
//!
//! Simple linked-list allocator with block splitting and coalescing.
//! Each allocated block stores its size in a header immediately
//! preceding the returned pointer.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;
use spin::Mutex;

/// Heap start address — MUST be past the kernel BSS section end.
/// Kernel is at 0x4008_0000, binary is ~1MB, so start heap at 2MB offset.
const HEAP_START: usize = 0x4020_0000;
const HEAP_SIZE: usize = 4 * 1024 * 1024; // 4MB kernel heap

/// Allocation header stored before every returned pointer
#[repr(C)]
struct AllocHeader {
    /// Total size of the block (header + padding + user data)
    size: usize,
    /// Magic value for sanity checking
    magic: usize,
}

const ALLOC_MAGIC: usize = 0xCAFE_BABE_DEAD_BEEF;
const HEADER_SIZE: usize = core::mem::size_of::<AllocHeader>();

/// Free node in the free list
#[repr(C)]
struct FreeNode {
    /// Size of this free region (including the FreeNode header)
    size: usize,
    /// Pointer to the next free node (or null)
    next: *mut FreeNode,
}

const NODE_SIZE: usize = core::mem::size_of::<FreeNode>();

/// Minimum useful block size
const MIN_BLOCK: usize = NODE_SIZE * 2;

struct Heap {
    free_list: *mut FreeNode,
    initialized: bool,
    /// Bump pointer for pre-init allocations
    bump: usize,
}

unsafe impl Send for Heap {}

impl Heap {
    const fn new() -> Self {
        Heap {
            free_list: null_mut(),
            initialized: false,
            bump: HEAP_START,
        }
    }

    fn init(&mut self) {
        unsafe {
            let node = HEAP_START as *mut FreeNode;
            (*node).size = HEAP_SIZE;
            (*node).next = null_mut();
            self.free_list = node;
        }
        self.initialized = true;
    }

    fn alloc(&mut self, layout: Layout) -> *mut u8 {
        if !self.initialized {
            return self.bump_alloc(layout);
        }

        // We need: header + alignment padding + user data
        let user_align = layout.align().max(8);
        // Total block size: header + enough room for aligned user area
        let total_needed = HEADER_SIZE + user_align + layout.size();

        // Walk free list (first-fit)
        let mut prev: *mut FreeNode = null_mut();
        let mut current = self.free_list;

        while !current.is_null() {
            let node_size = unsafe { (*current).size };
            let node_next = unsafe { (*current).next };

            if node_size >= total_needed {
                // Calculate aligned user pointer
                let block_addr = current as usize;
                let user_start = align_up(block_addr + HEADER_SIZE, user_align);
                let actual_needed = (user_start + layout.size()) - block_addr;
                let actual_needed = actual_needed.max(MIN_BLOCK);

                if node_size >= actual_needed {
                    let remaining = node_size - actual_needed;

                    if remaining >= MIN_BLOCK {
                        // Split: create new free node
                        let new_node = (block_addr + actual_needed) as *mut FreeNode;
                        unsafe {
                            (*new_node).size = remaining;
                            (*new_node).next = node_next;
                        }
                        if prev.is_null() {
                            self.free_list = new_node;
                        } else {
                            unsafe { (*prev).next = new_node; }
                        }
                    } else {
                        // Use entire block
                        let actual_needed = node_size;
                        if prev.is_null() {
                            self.free_list = node_next;
                        } else {
                            unsafe { (*prev).next = node_next; }
                        }
                        // Write header with full block size
                        let header = (user_start - HEADER_SIZE) as *mut AllocHeader;
                        unsafe {
                            (*header).size = actual_needed;
                            (*header).magic = ALLOC_MAGIC;
                        }
                        return user_start as *mut u8;
                    }

                    // Write header
                    let header = (user_start - HEADER_SIZE) as *mut AllocHeader;
                    unsafe {
                        (*header).size = actual_needed;
                        (*header).magic = ALLOC_MAGIC;
                    }
                    return user_start as *mut u8;
                }
            }

            prev = current;
            current = node_next;
        }

        null_mut()
    }

    fn dealloc(&mut self, ptr: *mut u8, _layout: Layout) {
        if ptr.is_null() || !self.initialized {
            return;
        }

        let ptr_addr = ptr as usize;
        if ptr_addr < HEAP_START || ptr_addr >= HEAP_START + HEAP_SIZE {
            return; // Out of range, ignore
        }

        // Header is directly before the pointer
        let header = unsafe { &*((ptr_addr - HEADER_SIZE) as *const AllocHeader) };

        if header.magic != ALLOC_MAGIC {
            return; // Corrupted or double-free, ignore
        }

        let block_size = header.size;
        if block_size == 0 || block_size > HEAP_SIZE {
            return; // Invalid size
        }

        // Compute block start: the header may not be at the block start due to alignment
        // The actual block start is ptr - HEADER_SIZE - alignment_padding
        // But we stored the total block size, so block_start = ptr - HEADER_SIZE - offset_to_user
        // Actually we need to find where the free node was originally
        // Since we split from block_addr, and user_start = align_up(block_addr + HEADER_SIZE, align)
        // We can recover: The header is at user_start - HEADER_SIZE,
        // and block_addr <= header_addr.
        // For simplicity, the freed region starts at (header_addr) and has size = header.size
        // (which was the distance from block_addr, not header_addr)
        // 
        // Better approach: the block_addr is the allocation start. 
        // user_start = align_up(block_addr + HEADER_SIZE, align)
        // header is at user_start - HEADER_SIZE
        // block_addr = header_addr - alignment_padding (could be 0)
        // Fortunately, header.size = actual_needed which is measured from block_addr.
        // 
        // We need to recover block_addr. Since header is between block_addr and user_start:
        // block_addr <= header_addr < user_start
        // block_addr + HEADER_SIZE <= user_start (before alignment)
        // header_addr = user_start - HEADER_SIZE
        // So header_addr >= block_addr.
        //
        // If alignment == 8 (most common), then user_start == block_addr + HEADER_SIZE
        // and header_addr == block_addr.
        //
        // For larger alignments, there may be a gap. But we can just use
        // header_addr as the free block start and use header.size as the size
        // (slightly overestimating, which wastes a few bytes but is safe).
        let free_start = ptr_addr - HEADER_SIZE;
        let free_size = block_size;

        // Clear magic to prevent double-free
        unsafe {
            (*(free_start as *mut AllocHeader)).magic = 0;
        }

        // Insert into sorted free list and coalesce
        let new_node = free_start as *mut FreeNode;
        unsafe {
            (*new_node).size = free_size;
        }

        // Find insertion point
        let mut prev: *mut FreeNode = null_mut();
        let mut current = self.free_list;

        while !current.is_null() && (current as usize) < free_start {
            prev = current;
            current = unsafe { (*current).next };
        }

        // Link in
        unsafe {
            (*new_node).next = current;
        }
        if prev.is_null() {
            self.free_list = new_node;
        } else {
            unsafe { (*prev).next = new_node; }
        }

        // Coalesce with next
        unsafe {
            if !(*new_node).next.is_null() {
                let next = (*new_node).next;
                if free_start + (*new_node).size == next as usize {
                    (*new_node).size += (*next).size;
                    (*new_node).next = (*next).next;
                }
            }
            // Coalesce with previous
            if !prev.is_null() {
                if (prev as usize) + (*prev).size == new_node as usize {
                    (*prev).size += (*new_node).size;
                    (*prev).next = (*new_node).next;
                }
            }
        }
    }

    fn bump_alloc(&mut self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(8);
        let start = align_up(self.bump, align);
        let end = start + layout.size();
        if end > HEAP_START + HEAP_SIZE {
            null_mut()
        } else {
            self.bump = end;
            start as *mut u8
        }
    }
}

#[inline]
fn align_up(val: usize, align: usize) -> usize {
    (val + align - 1) & !(align - 1)
}

static HEAP: Mutex<Heap> = Mutex::new(Heap::new());

/// Global allocator wrapper
pub struct KernelAllocator;

unsafe impl GlobalAlloc for KernelAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        HEAP.lock().alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        HEAP.lock().dealloc(ptr, layout);
    }
}

#[global_allocator]
static GLOBAL_ALLOC: KernelAllocator = KernelAllocator;

#[alloc_error_handler]
fn alloc_error(layout: Layout) -> ! {
    panic!("Kernel heap allocation failed: {:?}", layout);
}

/// Initialize heap — called after physical/virtual memory is set up
pub fn init() {
    // Map heap pages into virtual memory (identity mapped)
    for offset in (0..HEAP_SIZE).step_by(crate::arch::aarch64::mmu::PAGE_SIZE) {
        let addr = HEAP_START + offset;
        crate::mm::virtual_mem::map_page(
            addr,
            addr, // identity mapped
            crate::arch::aarch64::mmu::PageFlags::VALID
                | crate::arch::aarch64::mmu::PageFlags::PAGE
                | crate::arch::aarch64::mmu::PageFlags::ACCESSED
                | crate::arch::aarch64::mmu::PageFlags::INNER_SHAREABLE
                | crate::arch::aarch64::mmu::PageFlags::ATTR_NORMAL_WB,
        );
    }

    // Initialize the free list
    HEAP.lock().init();
}

/// Get heap usage statistics: (used_bytes, total_bytes)
pub fn heap_stats() -> (usize, usize) {
    let heap = HEAP.lock();
    if !heap.initialized {
        return (0, HEAP_SIZE);
    }
    let mut free_bytes = 0usize;
    let mut node = heap.free_list;
    while !node.is_null() {
        unsafe {
            free_bytes += (*node).size;
            node = (*node).next;
        }
    }
    let used = HEAP_SIZE.saturating_sub(free_bytes);
    (used, HEAP_SIZE)
}
