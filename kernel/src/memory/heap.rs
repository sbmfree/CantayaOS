// Kernel Heap Allocator
//
// This provides `alloc::Box`, `alloc::Vec`, and other standard allocation types
// for use within the kernel.
//
// We use a linked-list free-list allocator:
//   - The heap is a contiguous region of virtual memory
//   - Free blocks are linked together in a sorted list
//   - Allocation searches the list for a best-fit block
//   - Adjacent free blocks are coalesced to reduce fragmentation
//
// Why a linked-list allocator?
//   - Simple to implement and debug
//   - Good enough for early kernel bring-up
//   - Can be replaced with a slab allocator later for better performance
//
// In Windows NT, the kernel heap is implemented as "pool memory":
//   - NonPagedPool: always in physical memory (like our heap)
//   - PagedPool: can be paged out to disk (future feature)
//
// The heap is backed by physical frames from the frame allocator.
// Initial size: 1 MiB. It can grow dynamically.

use super::frame_allocator;
use super::PAGE_SIZE;
use core::alloc::{GlobalAlloc, Layout};
use spin::Mutex;

/// Initial heap size: 1 MiB
pub const HEAP_SIZE: usize = 1024 * 1024;

/// Growth increment: 256 KiB at a time
const GROW_INCREMENT: usize = 256 * 1024;

/// Minimum pages to grow by (64 pages = 256 KiB)
const GROW_MIN_PAGES: usize = 64;

/// Virtual address where the kernel heap starts.
/// Placed in the higher-half space, after the kernel image.
/// Must not overlap with the kernel code/data.
const HEAP_START: usize = 0xFFFF_FFFF_C000_0000;

/// The global allocator instance — Rust requires exactly one #[global_allocator]
#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap(Mutex::new(Heap::empty()));

/// A wrapper that provides the GlobalAlloc trait with mutex protection
struct LockedHeap(Mutex<Heap>);

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.0
            .lock()
            .allocate(layout)
            .unwrap_or(core::ptr::null_mut())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.0.lock().deallocate(ptr, layout);
    }
}

/// A free block in the heap's free list
///
/// Free blocks form a linked list sorted by address.
/// Each block header is embedded in the free space itself.
struct FreeBlock {
    size: usize,
    next: Option<*mut FreeBlock>,
}

impl FreeBlock {
    /// Minimum block size — must fit the FreeBlock header
    const MIN_SIZE: usize = core::mem::size_of::<FreeBlock>();
}

/// The heap allocator state
struct Heap {
    /// Head of the free block linked list
    free_list: Option<*mut FreeBlock>,
    /// Total heap size in bytes
    total_size: usize,
    /// Currently allocated bytes (for statistics)
    allocated_bytes: usize,
}

// Safety: The Mutex around Heap ensures single-threaded access.
unsafe impl Send for Heap {}

impl Heap {
    /// Create an empty (uninitialized) heap
    const fn empty() -> Self {
        Self {
            free_list: None,
            total_size: 0,
            allocated_bytes: 0,
        }
    }

    /// Initialize the heap with a memory region.
    ///
    /// `start` is the virtual address of the heap region.
    /// `size` is the total size in bytes.
    ///
    /// SAFETY: The memory region [start, start+size) must be valid and mapped.
    unsafe fn init(&mut self, start: usize, size: usize) {
        // Create the initial free block spanning the entire heap
        let block = start as *mut FreeBlock;
        (*block).size = size;
        (*block).next = None;
        self.free_list = Some(block);
        self.total_size = size;
        self.allocated_bytes = 0;
    }

    /// Allocate memory with the given layout.
    ///
    /// Uses a first-fit strategy: finds the first free block large enough
    /// to satisfy the allocation, splitting it if necessary.
    /// If no block is large enough, tries to grow the heap first.
    fn allocate(&mut self, layout: Layout) -> Option<*mut u8> {
        // Try to allocate from existing free list first
        if let Some(ptr) = self.try_allocate(layout) {
            return Some(ptr);
        }

        // Allocation failed — try to grow the heap
        let needed = layout.size().max(FreeBlock::MIN_SIZE);
        let grow_size = (needed + GROW_INCREMENT - 1) & !(GROW_INCREMENT - 1);
        if self.grow(grow_size) {
            // Retry allocation after growing
            self.try_allocate(layout)
        } else {
            None
        }
    }

    /// Internal allocation logic (no growth retry).
    fn try_allocate(&mut self, layout: Layout) -> Option<*mut u8> {
        let size = layout.size().max(FreeBlock::MIN_SIZE);
        let align = layout.align();

        // Search the free list for a suitable block
        let mut prev: Option<*mut FreeBlock> = None;
        let mut current = self.free_list;

        while let Some(block_ptr) = current {
            let block = unsafe { &mut *block_ptr };
            let block_addr = block_ptr as usize;
            let block_size = block.size;

            // Calculate the aligned start address within this block
            // We need space for the allocation size header (8 bytes before the returned pointer)
            let header_size = core::mem::size_of::<usize>();
            let data_start = block_addr + header_size;
            let aligned_start = (data_start + align - 1) & !(align - 1);
            // Ensure total_needed is 8-byte aligned so the remainder block is properly aligned
            let total_needed_raw = (aligned_start - block_addr) + size;
            let total_needed = (total_needed_raw + 7) & !7;

            if block_size >= total_needed {
                // This block is large enough — remove it from the free list
                let next = block.next;

                if let Some(prev_ptr) = prev {
                    unsafe { (*prev_ptr).next = next };
                } else {
                    self.free_list = next;
                }

                // If the block is significantly larger than needed, split it
                let remaining = block_size - total_needed;
                if remaining > FreeBlock::MIN_SIZE * 2 {
                    // Create a new free block from the remainder
                    let new_block = (block_addr + total_needed) as *mut FreeBlock;
                    unsafe {
                        (*new_block).size = remaining;
                        (*new_block).next = self.free_list;
                    }
                    self.free_list = Some(new_block);

                    // Store the actual allocation size in the header
                    let header_ptr = (aligned_start - header_size) as *mut usize;
                    unsafe { *header_ptr = total_needed };
                } else {
                    // Use the entire block (avoid tiny fragments)
                    let header_ptr = (aligned_start - header_size) as *mut usize;
                    unsafe { *header_ptr = block_size };
                }

                self.allocated_bytes += total_needed;
                return Some(aligned_start as *mut u8);
            }

            prev = current;
            current = block.next;
        }

        None // No suitable block found
    }

    /// Grow the heap by allocating additional physical frames.
    ///
    /// Returns true if growth was successful.
    fn grow(&mut self, min_bytes: usize) -> bool {
        let pages = (min_bytes + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
        let pages = pages.max(GROW_MIN_PAGES); // Grow by at least 256 KiB

        if let Some(phys_addr) = frame_allocator::allocate_contiguous_frames(pages) {
            let new_region = phys_addr as usize;
            let new_size = pages * PAGE_SIZE as usize;

            // Add the new region as a free block
            let new_block = new_region as *mut FreeBlock;
            unsafe {
                (*new_block).size = new_size;
                (*new_block).next = None;
            }

            // Insert into the free list in sorted order and coalesce
            let mut prev: Option<*mut FreeBlock> = None;
            let mut current = self.free_list;

            while let Some(block_ptr) = current {
                if block_ptr as usize > new_region {
                    break;
                }
                prev = current;
                current = unsafe { (*block_ptr).next };
            }

            unsafe {
                (*new_block).next = current;
                if let Some(prev_ptr) = prev {
                    (*prev_ptr).next = Some(new_block);

                    // Coalesce with previous if adjacent
                    let prev_end = prev_ptr as usize + (*prev_ptr).size;
                    if prev_end == new_region {
                        (*prev_ptr).size += (*new_block).size;
                        (*prev_ptr).next = (*new_block).next;
                        // new_block is now absorbed into prev_ptr
                        // Try to coalesce with next block too
                        if let Some(next_ptr) = (*prev_ptr).next {
                            let our_end = prev_ptr as usize + (*prev_ptr).size;
                            if our_end == next_ptr as usize {
                                (*prev_ptr).size += (*next_ptr).size;
                                (*prev_ptr).next = (*next_ptr).next;
                            }
                        }
                    } else {
                        // Coalesce with next if adjacent
                        if let Some(next_ptr) = (*new_block).next {
                            let our_end = new_region + (*new_block).size;
                            if our_end == next_ptr as usize {
                                (*new_block).size += (*next_ptr).size;
                                (*new_block).next = (*next_ptr).next;
                            }
                        }
                    }
                } else {
                    self.free_list = Some(new_block);
                    // Coalesce with next if adjacent
                    if let Some(next_ptr) = (*new_block).next {
                        let our_end = new_region + (*new_block).size;
                        if our_end == next_ptr as usize {
                            (*new_block).size += (*next_ptr).size;
                            (*new_block).next = (*next_ptr).next;
                        }
                    }
                }
            }

            self.total_size += new_size;
            log::info!("Heap grew by {} KiB (now {} KiB total)", new_size / 1024, self.total_size / 1024);
            true
        } else {
            log::error!("Heap growth failed: could not allocate {} pages", pages);
            false
        }
    }

    /// Deallocate previously allocated memory.
    ///
    /// Returns the block to the free list and coalesces adjacent free blocks.
    fn deallocate(&mut self, ptr: *mut u8, _layout: Layout) {
        let data_addr = ptr as usize;
        let header_size = core::mem::size_of::<usize>();
        let header_ptr = (data_addr - header_size) as *const usize;
        let total_size = unsafe { *header_ptr };

        let block_addr = data_addr - header_size;

        // Create a free block at this location
        let new_free = block_addr as *mut FreeBlock;
        unsafe {
            (*new_free).size = total_size;
        }

        // Insert into the free list in sorted order (by address)
        // This enables coalescing adjacent blocks
        let mut prev: Option<*mut FreeBlock> = None;
        let mut current = self.free_list;

        while let Some(block_ptr) = current {
            if block_ptr as usize > block_addr {
                break;
            }
            prev = current;
            current = unsafe { (*block_ptr).next };
        }

        // Insert the block
        unsafe {
            (*new_free).next = current;
            if let Some(prev_ptr) = prev {
                (*prev_ptr).next = Some(new_free);
            } else {
                self.free_list = Some(new_free);
            }

            // Coalesce with the next block if adjacent
            if let Some(next_ptr) = (*new_free).next {
                if block_addr + (*new_free).size == next_ptr as usize {
                    (*new_free).size += (*next_ptr).size;
                    (*new_free).next = (*next_ptr).next;
                }
            }

            // Coalesce with the previous block if adjacent
            if let Some(prev_ptr) = prev {
                let prev_end = prev_ptr as usize + (*prev_ptr).size;
                if prev_end == block_addr {
                    (*prev_ptr).size += (*new_free).size;
                    (*prev_ptr).next = (*new_free).next;
                }
            }
        }

        self.allocated_bytes = self.allocated_bytes.saturating_sub(total_size);
    }
}

/// Initialize the kernel heap.
///
/// Allocates physical frames and maps them to the heap virtual address range.
/// For now, we identity-map these pages (since the bootloader already set up
/// an identity mapping for the first 4 GiB).
///
/// Future: Use proper virtual memory mapping.
pub fn init() {
    let heap_pages = HEAP_SIZE / PAGE_SIZE as usize;

    // Allocate contiguous physical frames for the heap
    let phys_addr = frame_allocator::allocate_contiguous_frames(heap_pages)
        .expect("Failed to allocate frames for kernel heap");

    // For now, we use the physical address directly (it's identity-mapped by the bootloader).
    // Future: Map these frames to HEAP_START virtual address via the kernel's page tables.
    let heap_addr = phys_addr as usize;

    unsafe {
        ALLOCATOR.0.lock().init(heap_addr, HEAP_SIZE);
    }

    log::info!(
        "Heap initialized: {} KiB at {:#X}",
        HEAP_SIZE / 1024,
        heap_addr
    );
}
