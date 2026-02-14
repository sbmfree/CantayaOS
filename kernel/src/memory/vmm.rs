// Virtual Memory Manager (VMM)
//
// Manages the kernel's virtual address space and provides high-level
// allocation/deallocation of virtual regions backed by physical frames.
//
// The VMM sits above the page table code (paging.rs) and the frame
// allocator (frame_allocator.rs), combining them into a coherent API:
//
//   VMM::alloc_and_map(pages, flags)  →  virtual address
//   VMM::unmap_and_free(vaddr, pages) →  reclaims both virtual + physical
//
// Virtual Address Space Layout (kernel-half, 0xFFFF_8000_0000_0000+):
//
//   0xFFFF_FFFF_8000_0000  ..  kernel .text/.rodata/.data/.bss (bootloader)
//   0xFFFF_FFFF_C000_0000  ..  kernel heap (dynamic, grows upward)
//   0xFFFF_FFFF_A000_0000  ..  VMM dynamic region (vmalloc equivalent)
//   0xFFFF_8000_0000_0000  ..  direct physical map (future)
//
// Windows NT calls this MmAllocateContiguousMemory / MmMapIoSpace /
// MmAllocateNonPagedPool.
//
// Current implementation: A simple bump allocator for the dynamic virtual
// region, with a free-list for reclaimed regions. Good enough for a kernel
// that doesn't yet page to disk.

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::memory::frame_allocator;
use crate::memory::paging::{self, PageFlags};

// ── Virtual Address Space Layout ────────────────────────────────────────────

/// Start of the VMM dynamic allocation region.
/// This is the "vmalloc" area — variable-size kernel virtual allocations.
pub const VMM_REGION_START: u64 = 0xFFFF_FFFF_A000_0000;

/// End of the VMM dynamic allocation region (exclusive).
/// Leaves room before HEAP_START at 0xFFFF_FFFF_C000_0000.
pub const VMM_REGION_END: u64 = 0xFFFF_FFFF_C000_0000;

/// Start of the kernel heap virtual region.
pub const HEAP_REGION_START: u64 = 0xFFFF_FFFF_C000_0000;

/// Maximum size of the kernel heap (256 MiB).
pub const HEAP_REGION_MAX: u64 = 256 * 1024 * 1024;

// ── Virtual Region Tracking ─────────────────────────────────────────────────

/// A tracked virtual memory region.
#[derive(Debug, Clone, Copy)]
pub struct VirtualRegion {
    /// Starting virtual address.
    pub base: u64,
    /// Size in bytes (always a multiple of PAGE_SIZE).
    pub size: u64,
    /// Flags the region was mapped with.
    pub flags: PageFlags,
    /// What this region is used for.
    pub kind: RegionKind,
}

/// What a virtual region is used for (for debugging and stats).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    /// Kernel heap growth.
    Heap,
    /// Generic kernel allocation (vmalloc).
    KernelAlloc,
    /// Memory-mapped I/O device.
    Mmio,
    /// Per-process user-space mapping.
    UserMapping,
}

/// The kernel VMM state.
struct VmmState {
    /// Next free virtual address in the dynamic region (bump pointer).
    next_vaddr: u64,
    /// All active virtual regions (for diagnostics and cleanup).
    regions: Vec<VirtualRegion>,
    /// Free regions that can be reused (sorted by base address).
    free_regions: Vec<VirtualRegion>,
    /// Total bytes mapped through the VMM.
    total_mapped: u64,
}

static VMM: Mutex<Option<VmmState>> = Mutex::new(None);

/// The current top of the heap mapping (heap grows upward from HEAP_REGION_START).
static HEAP_TOP: AtomicU64 = AtomicU64::new(0);

// ── Initialization ──────────────────────────────────────────────────────────

/// Initialize the VMM.
///
/// Must be called after the frame allocator but before any virtual memory
/// allocations. Reads CR3 to capture the kernel's page table.
pub fn init() {
    use crate::hal::cpu;

    // Capture the kernel's PML4 from CR3
    let cr3 = cpu::read_cr3();
    // CR3 bits 12..51 are the PML4 physical address; bits 0..11 are flags
    let pml4_phys = cr3 & 0x000F_FFFF_FFFF_F000;
    paging::set_kernel_pml4(pml4_phys);

    // Initialize the VMM state
    let state = VmmState {
        next_vaddr: VMM_REGION_START,
        regions: Vec::with_capacity(64),
        free_regions: Vec::new(),
        total_mapped: 0,
    };
    *VMM.lock() = Some(state);

    HEAP_TOP.store(HEAP_REGION_START, Ordering::SeqCst);

    crate::serial_println!("[VMM] Initialized — PML4 at {:#x}", pml4_phys);
    crate::serial_println!(
        "[VMM] Dynamic region: {:#x} .. {:#x} ({} MiB)",
        VMM_REGION_START,
        VMM_REGION_END,
        (VMM_REGION_END - VMM_REGION_START) / (1024 * 1024)
    );
}

// ── Core Allocation API ─────────────────────────────────────────────────────

/// Allocate `page_count` pages of virtual address space, back them with
/// physical frames, and return the starting virtual address.
///
/// Each 4 KiB page gets its own freshly allocated physical frame.
/// The pages are mapped into the kernel's PML4 with the given flags.
///
/// # Returns
/// `Some(vaddr)` on success, `None` if out of virtual or physical memory.
pub fn alloc_and_map(page_count: usize, flags: PageFlags, kind: RegionKind) -> Option<u64> {
    if page_count == 0 {
        return None;
    }

    let size = (page_count as u64) * paging::PAGE_SIZE;
    let pml4 = paging::kernel_pml4();

    // Find a virtual address range
    let vaddr = alloc_virtual_range(size, kind)?;

    // Allocate and map each page
    for i in 0..page_count {
        let frame = match frame_allocator::allocate_frame() {
            Some(f) => f,
            None => {
                // Roll back: unmap and free what we've done so far
                for j in 0..i {
                    let addr = vaddr + (j as u64) * paging::PAGE_SIZE;
                    unsafe {
                        if let Some(phys) = paging::unmap_page(pml4, addr) {
                            frame_allocator::free_frame(phys);
                        }
                    }
                }
                free_virtual_range(vaddr, size);
                return None;
            }
        };

        let page_vaddr = vaddr + (i as u64) * paging::PAGE_SIZE;
        unsafe {
            if !paging::map_page(pml4, page_vaddr, frame, flags) {
                // Map failed — roll back including this frame
                frame_allocator::free_frame(frame);
                for j in 0..i {
                    let addr = vaddr + (j as u64) * paging::PAGE_SIZE;
                    if let Some(phys) = paging::unmap_page(pml4, addr) {
                        frame_allocator::free_frame(phys);
                    }
                }
                free_virtual_range(vaddr, size);
                return None;
            }
        }
    }

    // Record the region
    if let Some(ref mut state) = *VMM.lock() {
        state.regions.push(VirtualRegion {
            base: vaddr,
            size,
            flags,
            kind,
        });
        state.total_mapped += size;
    }

    Some(vaddr)
}

/// Unmap `page_count` pages starting at `vaddr` and free the underlying
/// physical frames.
///
/// # Safety
/// The caller must ensure no code or data references the unmapped region
/// after this call.
pub unsafe fn unmap_and_free(vaddr: u64, page_count: usize) {
    let pml4 = paging::kernel_pml4();
    let size = (page_count as u64) * paging::PAGE_SIZE;

    for i in 0..page_count {
        let addr = vaddr + (i as u64) * paging::PAGE_SIZE;
        if let Some(phys) = paging::unmap_page(pml4, addr) {
            frame_allocator::free_frame(phys);
        }
    }

    // Remove from tracked regions and add to free list
    if let Some(ref mut state) = *VMM.lock() {
        state.regions.retain(|r| r.base != vaddr);
        state.total_mapped = state.total_mapped.saturating_sub(size);
    }
    free_virtual_range(vaddr, size);
}

/// Map a range of physical addresses into kernel virtual address space.
///
/// Used for MMIO device registers. The physical frames are NOT allocated
/// by the frame allocator — they're hardware-owned.
///
/// # Returns
/// The virtual address where the range was mapped.
pub fn map_mmio(phys_base: u64, size: u64) -> Option<u64> {
    let page_count = (size + paging::PAGE_SIZE - 1) / paging::PAGE_SIZE;
    let aligned_phys = phys_base & !0xFFF;
    let phys_offset = phys_base - aligned_phys;
    let total_size = page_count * paging::PAGE_SIZE;

    let vaddr = alloc_virtual_range(total_size, RegionKind::Mmio)?;
    let pml4 = paging::kernel_pml4();

    let flags = PageFlags::WRITABLE | PageFlags::NO_CACHE | PageFlags::NO_EXECUTE;

    for i in 0..page_count {
        let offset = i * paging::PAGE_SIZE;
        unsafe {
            if !paging::map_page(pml4, vaddr + offset, aligned_phys + offset, flags) {
                // Roll back
                for j in 0..i {
                    let addr = vaddr + j * paging::PAGE_SIZE;
                    paging::unmap_page(pml4, addr);
                }
                free_virtual_range(vaddr, total_size);
                return None;
            }
        }
    }

    if let Some(ref mut state) = *VMM.lock() {
        state.regions.push(VirtualRegion {
            base: vaddr,
            size: total_size,
            flags,
            kind: RegionKind::Mmio,
        });
        state.total_mapped += total_size;
    }

    Some(vaddr + phys_offset)
}

/// Unmap a previously mapped MMIO region.
///
/// Unlike `unmap_and_free`, this does NOT free physical frames (they
/// belong to hardware).
///
/// # Safety
/// The caller must ensure no code references the unmapped region.
pub unsafe fn unmap_mmio(vaddr: u64, size: u64) {
    let pml4 = paging::kernel_pml4();
    let aligned_vaddr = vaddr & !0xFFF;
    let page_count = (size + paging::PAGE_SIZE - 1) / paging::PAGE_SIZE;

    for i in 0..page_count {
        let addr = aligned_vaddr + i * paging::PAGE_SIZE;
        paging::unmap_page(pml4, addr);
    }

    if let Some(ref mut state) = *VMM.lock() {
        state.regions.retain(|r| r.base != aligned_vaddr);
        state.total_mapped = state.total_mapped.saturating_sub(page_count * paging::PAGE_SIZE);
    }
    free_virtual_range(aligned_vaddr, page_count * paging::PAGE_SIZE);
}

// ── Heap Support ────────────────────────────────────────────────────────────

/// Extend the kernel heap by mapping new pages at the current heap top.
///
/// Called by the heap allocator when it needs to grow.
///
/// # Returns
/// `Some((vaddr, size))` — the virtual address and size of the new region.
/// `None` if the heap has reached its maximum size or allocation failed.
pub fn extend_heap(min_bytes: usize) -> Option<(u64, usize)> {
    let page_count = (min_bytes + paging::PAGE_SIZE as usize - 1) / paging::PAGE_SIZE as usize;
    let size = (page_count as u64) * paging::PAGE_SIZE;

    let current_top = HEAP_TOP.load(Ordering::SeqCst);
    let new_top = current_top + size;

    // Check we don't exceed the heap region
    if new_top > HEAP_REGION_START + HEAP_REGION_MAX {
        return None;
    }

    let pml4 = paging::kernel_pml4();
    let flags = PageFlags::WRITABLE | PageFlags::NO_EXECUTE;

    // Allocate and map each page
    for i in 0..page_count {
        let frame = frame_allocator::allocate_frame()?;
        let vaddr = current_top + (i as u64) * paging::PAGE_SIZE;
        unsafe {
            if !paging::map_page(pml4, vaddr, frame, flags) {
                // Roll back
                frame_allocator::free_frame(frame);
                for j in 0..i {
                    let addr = current_top + (j as u64) * paging::PAGE_SIZE;
                    if let Some(phys) = paging::unmap_page(pml4, addr) {
                        frame_allocator::free_frame(phys);
                    }
                }
                return None;
            }
        }
    }

    HEAP_TOP.store(new_top, Ordering::SeqCst);

    // Track it
    if let Some(ref mut state) = *VMM.lock() {
        state.regions.push(VirtualRegion {
            base: current_top,
            size,
            flags,
            kind: RegionKind::Heap,
        });
        state.total_mapped += size;
    }

    Some((current_top as u64, size as usize))
}

/// Map the initial kernel heap at HEAP_REGION_START.
///
/// Called during memory initialization to set up the heap at its proper
/// virtual address instead of relying on the bootloader's identity map.
///
/// # Returns
/// `Some(vaddr)` on success, `None` on failure.
pub fn map_initial_heap(page_count: usize) -> Option<u64> {
    let pml4 = paging::kernel_pml4();
    let flags = PageFlags::WRITABLE | PageFlags::NO_EXECUTE;
    let vaddr = HEAP_REGION_START;
    let size = (page_count as u64) * paging::PAGE_SIZE;

    for i in 0..page_count {
        let frame = match frame_allocator::allocate_frame() {
            Some(f) => f,
            None => {
                // Roll back
                for j in 0..i {
                    let addr = vaddr + (j as u64) * paging::PAGE_SIZE;
                    unsafe {
                        if let Some(phys) = paging::unmap_page(pml4, addr) {
                            frame_allocator::free_frame(phys);
                        }
                    }
                }
                return None;
            }
        };

        unsafe {
            if !paging::map_page(pml4, vaddr + (i as u64) * paging::PAGE_SIZE, frame, flags) {
                frame_allocator::free_frame(frame);
                for j in 0..i {
                    let addr = vaddr + (j as u64) * paging::PAGE_SIZE;
                    if let Some(phys) = paging::unmap_page(pml4, addr) {
                        frame_allocator::free_frame(phys);
                    }
                }
                return None;
            }
        }
    }

    // Update heap top
    HEAP_TOP.store(vaddr + size, Ordering::SeqCst);

    crate::serial_println!(
        "[VMM] Mapped initial heap: {:#x} .. {:#x} ({} KiB)",
        vaddr,
        vaddr + size,
        size / 1024
    );

    Some(vaddr)
}

// ── Translation API ─────────────────────────────────────────────────────────

/// Translate a kernel virtual address to its physical address.
///
/// Returns `None` if the address is not mapped.
pub fn virt_to_phys(vaddr: u64) -> Option<u64> {
    paging::translate(paging::kernel_pml4(), vaddr)
}

// ── Diagnostics ─────────────────────────────────────────────────────────────

/// VMM statistics for debugging and shell commands.
#[derive(Debug, Clone)]
pub struct VmmStats {
    pub total_mapped_bytes: u64,
    pub region_count: usize,
    pub heap_used_bytes: u64,
    pub vmm_region_used: u64,
    pub next_alloc_addr: u64,
    pub free_region_count: usize,
}

/// Get current VMM statistics.
pub fn stats() -> Option<VmmStats> {
    let state = VMM.lock();
    let state = state.as_ref()?;

    let heap_top = HEAP_TOP.load(Ordering::SeqCst);

    Some(VmmStats {
        total_mapped_bytes: state.total_mapped,
        region_count: state.regions.len(),
        heap_used_bytes: heap_top - HEAP_REGION_START,
        vmm_region_used: state.next_vaddr - VMM_REGION_START,
        next_alloc_addr: state.next_vaddr,
        free_region_count: state.free_regions.len(),
    })
}

/// Get all active virtual regions (for diagnostic output).
pub fn active_regions() -> Vec<VirtualRegion> {
    let state = VMM.lock();
    match state.as_ref() {
        Some(s) => s.regions.clone(),
        None => Vec::new(),
    }
}

// ── Internal Helpers ────────────────────────────────────────────────────────

/// Allocate a range of virtual addresses from the dynamic region.
fn alloc_virtual_range(size: u64, _kind: RegionKind) -> Option<u64> {
    let mut state = VMM.lock();
    let state = state.as_mut()?;

    // First, try to reuse a free region that's large enough
    let mut best_idx = None;
    let mut best_waste = u64::MAX;
    for (i, region) in state.free_regions.iter().enumerate() {
        if region.size >= size {
            let waste = region.size - size;
            if waste < best_waste {
                best_waste = waste;
                best_idx = Some(i);
            }
        }
    }

    if let Some(idx) = best_idx {
        let region = state.free_regions[idx];
        let vaddr = region.base;

        if region.size == size {
            state.free_regions.remove(idx);
        } else {
            // Split: keep the remainder
            state.free_regions[idx].base += size;
            state.free_regions[idx].size -= size;
        }

        return Some(vaddr);
    }

    // No free region available — bump allocate
    let vaddr = state.next_vaddr;
    let new_next = vaddr + size;

    if new_next > VMM_REGION_END {
        return None; // Out of virtual address space
    }

    state.next_vaddr = new_next;
    Some(vaddr)
}

/// Return a virtual range to the free list.
fn free_virtual_range(vaddr: u64, size: u64) {
    let mut state = VMM.lock();
    if let Some(ref mut state) = *state {
        // Insert in sorted order by base address
        let pos = state
            .free_regions
            .iter()
            .position(|r| r.base > vaddr)
            .unwrap_or(state.free_regions.len());

        state.free_regions.insert(
            pos,
            VirtualRegion {
                base: vaddr,
                size,
                flags: PageFlags::empty(),
                kind: RegionKind::KernelAlloc,
            },
        );

        // Coalesce with neighbors
        // Merge with next
        if pos + 1 < state.free_regions.len() {
            let current = state.free_regions[pos];
            let next = state.free_regions[pos + 1];
            if current.base + current.size == next.base {
                state.free_regions[pos].size += next.size;
                state.free_regions.remove(pos + 1);
            }
        }
        // Merge with previous
        if pos > 0 {
            let prev = state.free_regions[pos - 1];
            let current = state.free_regions[pos];
            if prev.base + prev.size == current.base {
                state.free_regions[pos - 1].size += current.size;
                state.free_regions.remove(pos);
            }
        }
    }
}
