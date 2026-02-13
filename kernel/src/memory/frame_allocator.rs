// Physical Frame Allocator
//
// Manages physical memory allocation at the granularity of 4 KiB frames.
// Uses a bitmap where each bit represents one physical frame:
//   - 0 = free
//   - 1 = allocated
//
// Why a bitmap?
//   - O(1) allocation/deallocation (with free-frame hint)
//   - Cache-friendly sequential scanning
//   - Fixed overhead: 1 bit per 4 KiB frame = 32 KiB per 1 GiB of RAM
//   - Simple to implement correctly (important for kernel code)
//
// Alternative considered: free-list (linked list of free frames)
//   - Pro: O(1) allocation
//   - Con: uses the free frames themselves for list nodes, which is fragile
//   - Con: no way to query if a specific frame is free
//
// In Windows NT, this is analogous to the PFN (Page Frame Number) database,
// which tracks the state of every physical page in the system.

use cantaya_shared::boot_info::MemoryMap;
use cantaya_shared::memory::MemoryRegionKind;
use super::PAGE_SIZE;
use spin::Mutex;

/// Maximum physical memory we support (16 GiB initially)
/// This limits the bitmap size to 512 KiB (16 GiB / 4 KiB / 8 bits per byte)
const MAX_PHYSICAL_MEMORY: u64 = 16 * 1024 * 1024 * 1024;
const MAX_FRAMES: usize = (MAX_PHYSICAL_MEMORY / PAGE_SIZE) as usize;
const BITMAP_SIZE: usize = MAX_FRAMES / 8;

/// The frame allocator bitmap — stored in BSS (zeroed at boot)
/// Each bit represents one 4 KiB physical frame.
/// We start with all frames marked as used (1), then mark free ones based on the memory map.
static FRAME_ALLOCATOR: Mutex<FrameAllocator> = Mutex::new(FrameAllocator::new());

struct FrameAllocator {
    /// Bitmap: bit=0 means free, bit=1 means allocated/reserved
    bitmap: [u8; BITMAP_SIZE],
    /// Total number of frames in the system
    total_frames: usize,
    /// Number of currently free frames
    free_frames: usize,
    /// Hint: start searching for free frames from this index (optimization)
    next_free_hint: usize,
}

impl FrameAllocator {
    const fn new() -> Self {
        Self {
            bitmap: [0xFF; BITMAP_SIZE], // All frames marked as used initially
            total_frames: 0,
            free_frames: 0,
            next_free_hint: 0,
        }
    }

    /// Mark a frame as free (clear its bit)
    fn mark_free(&mut self, frame_index: usize) {
        if frame_index >= MAX_FRAMES {
            return;
        }
        let byte = frame_index / 8;
        let bit = frame_index % 8;
        if self.bitmap[byte] & (1 << bit) != 0 {
            self.bitmap[byte] &= !(1 << bit);
            self.free_frames += 1;
        }
    }

    /// Mark a frame as allocated (set its bit)
    fn mark_allocated(&mut self, frame_index: usize) {
        if frame_index >= MAX_FRAMES {
            return;
        }
        let byte = frame_index / 8;
        let bit = frame_index % 8;
        if self.bitmap[byte] & (1 << bit) == 0 {
            self.bitmap[byte] |= 1 << bit;
            self.free_frames -= 1;
        }
    }

    /// Check if a frame is free
    fn is_free(&self, frame_index: usize) -> bool {
        if frame_index >= MAX_FRAMES {
            return false;
        }
        let byte = frame_index / 8;
        let bit = frame_index % 8;
        self.bitmap[byte] & (1 << bit) == 0
    }

    /// Allocate a single physical frame, returning its physical address.
    /// Returns None if no free frames are available.
    fn allocate_frame(&mut self) -> Option<u64> {
        // Start searching from the hint to avoid re-scanning used frames
        let start = self.next_free_hint;

        // Search from hint to end
        for i in start..self.total_frames {
            if self.is_free(i) {
                self.mark_allocated(i);
                self.next_free_hint = i + 1;
                return Some(i as u64 * PAGE_SIZE);
            }
        }

        // Wrap around: search from beginning to hint
        for i in 0..start {
            if self.is_free(i) {
                self.mark_allocated(i);
                self.next_free_hint = i + 1;
                return Some(i as u64 * PAGE_SIZE);
            }
        }

        None // Out of memory
    }

    /// Free a previously allocated frame.
    fn free_frame(&mut self, physical_addr: u64) {
        let frame_index = (physical_addr / PAGE_SIZE) as usize;
        self.mark_free(frame_index);
        // Update hint if this frame is before the current hint
        if frame_index < self.next_free_hint {
            self.next_free_hint = frame_index;
        }
    }
}

/// Initialize the frame allocator from the UEFI memory map.
///
/// We iterate over all memory regions and mark usable ones as free.
/// Regions that contain the kernel, firmware, or reserved areas stay marked as allocated.
pub fn init(memory_map: &MemoryMap) {
    let mut allocator = FRAME_ALLOCATOR.lock();

    for region in memory_map.iter() {
        // Only mark conventional (usable) memory as free
        if region.kind != MemoryRegionKind::Usable {
            continue;
        }

        // Skip the first 1 MiB — it contains legacy BIOS data structures,
        // real-mode IVT, BDA, etc. Even in UEFI mode, some firmware touches this area.
        if region.base < 0x100000 {
            continue;
        }

        let start_frame = (region.base / PAGE_SIZE) as usize;
        let end_frame = ((region.base + region.size) / PAGE_SIZE) as usize;

        for frame in start_frame..end_frame {
            if frame < MAX_FRAMES {
                allocator.mark_free(frame);
                if frame >= allocator.total_frames {
                    allocator.total_frames = frame + 1;
                }
            }
        }
    }
}

/// Allocate a single physical frame (4 KiB).
/// Returns the physical address of the frame, or None if out of memory.
pub fn allocate_frame() -> Option<u64> {
    FRAME_ALLOCATOR.lock().allocate_frame()
}

/// Allocate `count` contiguous physical frames.
/// Returns the physical address of the first frame, or None if not available.
///
/// Note: This is a simple linear scan. For production use, a buddy allocator
/// would be more efficient for contiguous allocations.
pub fn allocate_contiguous_frames(count: usize) -> Option<u64> {
    let mut allocator = FRAME_ALLOCATOR.lock();
    let total = allocator.total_frames;

    'outer: for start in 0..total {
        if start + count > total {
            return None;
        }

        // Check if `count` consecutive frames starting at `start` are all free
        for offset in 0..count {
            if !allocator.is_free(start + offset) {
                continue 'outer;
            }
        }

        // Found a run — allocate them all
        for offset in 0..count {
            allocator.mark_allocated(start + offset);
        }

        return Some(start as u64 * PAGE_SIZE);
    }

    None
}

/// Free a single physical frame
pub fn free_frame(physical_addr: u64) {
    FRAME_ALLOCATOR.lock().free_frame(physical_addr);
}

/// Get the number of free frames
pub fn free_frame_count() -> usize {
    FRAME_ALLOCATOR.lock().free_frames
}

/// Get the total number of tracked frames
pub fn total_frame_count() -> usize {
    FRAME_ALLOCATOR.lock().total_frames
}
