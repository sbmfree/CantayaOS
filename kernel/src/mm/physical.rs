//! Physical Memory Manager
//! 
//! Bitmap-based page frame allocator

use spin::Mutex;
use crate::arch::mmu::PAGE_SIZE;

/// Maximum physical memory (2GB)
const MAX_MEMORY: usize = 2 * 1024 * 1024 * 1024;
const MAX_PAGES: usize = MAX_MEMORY / PAGE_SIZE;
const BITMAP_SIZE: usize = MAX_PAGES / 64;

static ALLOCATOR: Mutex<PhysicalAllocator> = Mutex::new(PhysicalAllocator::new());

struct PhysicalAllocator {
    bitmap: [u64; BITMAP_SIZE],
    free_pages: usize,
    total_pages: usize,
}

impl PhysicalAllocator {
    const fn new() -> Self {
        PhysicalAllocator {
            bitmap: [0; BITMAP_SIZE],
            free_pages: 0,
            total_pages: 0,
        }
    }
    
    fn mark_used(&mut self, page: usize) {
        let idx = page / 64;
        let bit = page % 64;
        if idx < BITMAP_SIZE {
            self.bitmap[idx] |= 1 << bit;
        }
    }
    
    fn mark_free(&mut self, page: usize) {
        let idx = page / 64;
        let bit = page % 64;
        if idx < BITMAP_SIZE {
            self.bitmap[idx] &= !(1 << bit);
        }
    }
    
    #[allow(dead_code)]
    fn is_free(&self, page: usize) -> bool {
        let idx = page / 64;
        let bit = page % 64;
        if idx < BITMAP_SIZE {
            (self.bitmap[idx] & (1 << bit)) == 0
        } else {
            false
        }
    }
    
    fn allocate(&mut self) -> Option<usize> {
        for i in 0..BITMAP_SIZE {
            if self.bitmap[i] != !0u64 {
                for bit in 0..64 {
                    if (self.bitmap[i] & (1 << bit)) == 0 {
                        let page = i * 64 + bit;
                        if page < self.total_pages {
                            self.bitmap[i] |= 1 << bit;
                            self.free_pages -= 1;
                            return Some(page * PAGE_SIZE);
                        }
                    }
                }
            }
        }
        None
    }
    
    fn deallocate(&mut self, addr: usize) {
        let page = addr / PAGE_SIZE;
        self.mark_free(page);
        self.free_pages += 1;
    }

    /// Allocate `count` contiguous physical frames
    fn allocate_contiguous(&mut self, count: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        if count == 1 {
            return self.allocate();
        }
        
        // Scan bitmap for `count` consecutive free pages
        let mut start_page = 0usize;
        let mut found = 0usize;
        
        for page in 0..self.total_pages {
            let idx = page / 64;
            let bit = page % 64;
            
            if idx >= BITMAP_SIZE {
                break;
            }
            
            let is_free = (self.bitmap[idx] & (1 << bit)) == 0;
            
            if is_free {
                if found == 0 {
                    start_page = page;
                }
                found += 1;
                if found == count {
                    // Found enough! Mark them all as used
                    for p in start_page..start_page + count {
                        self.mark_used(p);
                        self.free_pages -= 1;
                    }
                    return Some(start_page * PAGE_SIZE);
                }
            } else {
                found = 0;
            }
        }
        
        None
    }
}

/// Initialize physical memory allocator
pub fn init() {
    let mut alloc = ALLOCATOR.lock();
    
    // 2GB RAM starting at 0x4000_0000 (QEMU virt)
    let ram_size = 2 * 1024 * 1024 * 1024;
    
    alloc.total_pages = ram_size / PAGE_SIZE;
    alloc.free_pages = alloc.total_pages;
    
    // Mark kernel + heap region as used
    // Kernel: 0x4008_0000..~0x401A_0000 (up to 2MB for safety)
    // Heap:   0x4020_0000..0x4060_0000 (4MB)
    // Total reserved: 6MB from RAM_BASE
    let reserved_pages = (6 * 1024 * 1024) / PAGE_SIZE;
    for page in 0..reserved_pages {
        alloc.mark_used(page);
        alloc.free_pages -= 1;
    }
}

/// RAM base address on QEMU virt machine
const RAM_BASE: usize = 0x4000_0000;

/// Allocate a physical page frame
pub fn alloc_frame() -> Option<usize> {
    ALLOCATOR.lock().allocate().map(|offset| RAM_BASE + offset)
}

/// Allocate `count` contiguous physical page frames
pub fn alloc_contiguous_frames(count: usize) -> Option<usize> {
    ALLOCATOR.lock().allocate_contiguous(count).map(|offset| RAM_BASE + offset)
}

/// Free a physical page frame
pub fn free_frame(addr: usize) {
    if addr >= RAM_BASE {
        ALLOCATOR.lock().deallocate(addr - RAM_BASE);
    }
}

/// Get free memory in bytes
pub fn free_memory() -> usize {
    ALLOCATOR.lock().free_pages * PAGE_SIZE
}

/// Get total memory in bytes
pub fn total_memory() -> usize {
    ALLOCATOR.lock().total_pages * PAGE_SIZE
}
