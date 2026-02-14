// Page Table Management — x86_64 4-Level Paging
//
// Implements the x86_64 paging structures used for virtual-to-physical address
// translation. The CPU walks four levels of page tables (PML4 → PDP → PD → PT)
// to resolve a virtual address into a physical frame.
//
// Virtual address format (48-bit canonical):
//   Bits 47..39  → PML4 index   (9 bits, 512 entries)
//   Bits 38..30  → PDP index    (9 bits, 512 entries)
//   Bits 29..21  → PD index     (9 bits, 512 entries)
//   Bits 20..12  → PT index     (9 bits, 512 entries)
//   Bits 11..0   → Page offset  (12 bits, 4 KiB)
//
// Each level is a 4 KiB-aligned table of 512 × 8-byte entries.
//
// Windows NT equivalent: MiMapPageTable / MiReserveSystemPtes
//
// SAFETY: Page table operations are inherently unsafe because incorrect
// mappings can crash the kernel. All public functions that modify page tables
// document their safety requirements.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::hal::cpu;
use crate::memory::frame_allocator;

// ── Constants ────────────────────────────────────────────────────────────────

/// Number of entries in each page table level.
pub const ENTRIES_PER_TABLE: usize = 512;

/// Size of a standard page (4 KiB).
pub const PAGE_SIZE: u64 = 4096;

/// Size of a huge page at PD level (2 MiB).
pub const HUGE_PAGE_SIZE: u64 = 2 * 1024 * 1024;

/// Size of a 1 GiB page at PDP level.
#[allow(dead_code)]
pub const GIANT_PAGE_SIZE: u64 = 1024 * 1024 * 1024;

// ── Page Table Entry Flags ──────────────────────────────────────────────────

bitflags::bitflags! {
    /// Flags for a page table entry (common across all four levels).
    ///
    /// These correspond to the x86_64 page table entry bit layout:
    ///   Bit 0   — Present
    ///   Bit 1   — Read/Write
    ///   Bit 2   — User/Supervisor
    ///   Bit 3   — Page-level Write-Through
    ///   Bit 4   — Page-level Cache Disable
    ///   Bit 5   — Accessed
    ///   Bit 6   — Dirty (leaf entries only)
    ///   Bit 7   — Huge Page / PAT (PD level = 2 MiB, PDP level = 1 GiB)
    ///   Bit 8   — Global
    ///   Bit 63  — No Execute
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PageFlags: u64 {
        /// Page is present in physical memory.
        const PRESENT       = 1 << 0;
        /// Page is writable (otherwise read-only).
        const WRITABLE      = 1 << 1;
        /// Page is accessible from user mode (Ring 3).
        const USER          = 1 << 2;
        /// Write-through caching.
        const WRITE_THROUGH = 1 << 3;
        /// Disable caching for this page.
        const NO_CACHE      = 1 << 4;
        /// CPU has accessed this page (set by hardware).
        const ACCESSED      = 1 << 5;
        /// CPU has written to this page (set by hardware, leaf only).
        const DIRTY         = 1 << 6;
        /// This entry maps a huge page (2 MiB at PD level, 1 GiB at PDP).
        const HUGE_PAGE     = 1 << 7;
        /// Page is global (not flushed on CR3 switch).
        const GLOBAL        = 1 << 8;
        /// Instruction fetches are not allowed from this page.
        const NO_EXECUTE    = 1 << 63;
    }
}

/// Mask to extract the 4 KiB-aligned physical address from a PTE.
/// Bits 12..51 hold the physical frame number.
const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

// ── Page Table Entry ────────────────────────────────────────────────────────

/// A single page table entry (8 bytes). Used at all four levels.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    /// An empty (not present) entry.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Create an entry from raw bits.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Get the raw 64-bit value.
    pub const fn raw(&self) -> u64 {
        self.0
    }

    /// Check if this entry is present.
    pub fn is_present(&self) -> bool {
        self.0 & PageFlags::PRESENT.bits() != 0
    }

    /// Check if this entry maps a huge page (2 MiB or 1 GiB).
    pub fn is_huge(&self) -> bool {
        self.0 & PageFlags::HUGE_PAGE.bits() != 0
    }

    /// Get the physical address this entry points to (table or frame).
    pub fn address(&self) -> u64 {
        self.0 & ADDR_MASK
    }

    /// Get the flags on this entry.
    pub fn flags(&self) -> PageFlags {
        PageFlags::from_bits_truncate(self.0)
    }

    /// Set this entry to point to the given physical address with flags.
    pub fn set(&mut self, address: u64, flags: PageFlags) {
        debug_assert!(address & !ADDR_MASK == 0, "Address not page-aligned");
        self.0 = (address & ADDR_MASK) | flags.bits();
    }

    /// Clear this entry (mark as not present).
    pub fn clear(&mut self) {
        self.0 = 0;
    }
}

impl core::fmt::Debug for PageTableEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_present() {
            write!(
                f,
                "PTE({:#x}, {:?}{})",
                self.address(),
                self.flags(),
                if self.is_huge() { ", HUGE" } else { "" }
            )
        } else {
            write!(f, "PTE(empty)")
        }
    }
}

// ── Page Table ──────────────────────────────────────────────────────────────

/// A page table — 512 entries, 4 KiB, used at every level of the hierarchy.
///
/// The same structure represents PML4, PDP, PD, and PT. The interpretation
/// of entries depends on the level:
///   - PML4 entry → points to a PDP table
///   - PDP entry  → points to a PD table (or maps 1 GiB huge page)
///   - PD entry   → points to a PT table (or maps 2 MiB huge page)
///   - PT entry   → maps a 4 KiB page
#[repr(C, align(4096))]
pub struct PageTable {
    pub entries: [PageTableEntry; ENTRIES_PER_TABLE],
}

impl PageTable {
    /// Create a page table with all entries empty (not present).
    pub const fn empty() -> Self {
        const EMPTY: PageTableEntry = PageTableEntry::empty();
        Self {
            entries: [EMPTY; ENTRIES_PER_TABLE],
        }
    }

    /// Zero out all entries.
    pub fn clear(&mut self) {
        for entry in self.entries.iter_mut() {
            entry.clear();
        }
    }
}

// ── Virtual Address Decomposition ───────────────────────────────────────────

/// Extract the PML4 index (bits 39..47) from a virtual address.
#[inline]
pub fn pml4_index(vaddr: u64) -> usize {
    ((vaddr >> 39) & 0x1FF) as usize
}

/// Extract the PDP index (bits 30..38) from a virtual address.
#[inline]
pub fn pdp_index(vaddr: u64) -> usize {
    ((vaddr >> 30) & 0x1FF) as usize
}

/// Extract the PD index (bits 21..29) from a virtual address.
#[inline]
pub fn pd_index(vaddr: u64) -> usize {
    ((vaddr >> 21) & 0x1FF) as usize
}

/// Extract the PT index (bits 12..20) from a virtual address.
#[inline]
pub fn pt_index(vaddr: u64) -> usize {
    ((vaddr >> 12) & 0x1FF) as usize
}

/// Extract the page offset (bits 0..11) from a virtual address.
#[inline]
pub fn page_offset(vaddr: u64) -> u64 {
    vaddr & 0xFFF
}

// ── Kernel PML4 Tracking ────────────────────────────────────────────────────

/// Physical address of the kernel's PML4 page table, read from CR3 at boot.
static KERNEL_PML4_PHYS: AtomicU64 = AtomicU64::new(0);

/// Store the kernel's PML4 physical address (called once during memory init).
pub fn set_kernel_pml4(phys: u64) {
    KERNEL_PML4_PHYS.store(phys, Ordering::SeqCst);
}

/// Get the kernel's PML4 physical address.
pub fn kernel_pml4() -> u64 {
    KERNEL_PML4_PHYS.load(Ordering::SeqCst)
}

// ── Page Table Operations ───────────────────────────────────────────────────

/// Translate a virtual address to a physical address by walking the page tables.
///
/// Returns `None` if the address is not mapped.
///
/// SAFETY: Requires that the page tables referenced by `pml4_phys` are
/// identity-mapped (i.e., we can access them at their physical address).
pub fn translate(pml4_phys: u64, vaddr: u64) -> Option<u64> {
    let pml4 = unsafe { &*(pml4_phys as *const PageTable) };
    let pml4e = &pml4.entries[pml4_index(vaddr)];
    if !pml4e.is_present() {
        return None;
    }

    let pdp = unsafe { &*(pml4e.address() as *const PageTable) };
    let pdpe = &pdp.entries[pdp_index(vaddr)];
    if !pdpe.is_present() {
        return None;
    }
    // 1 GiB huge page?
    if pdpe.is_huge() {
        return Some(pdpe.address() | (vaddr & (GIANT_PAGE_SIZE - 1)));
    }

    let pd = unsafe { &*(pdpe.address() as *const PageTable) };
    let pde = &pd.entries[pd_index(vaddr)];
    if !pde.is_present() {
        return None;
    }
    // 2 MiB huge page?
    if pde.is_huge() {
        return Some(pde.address() | (vaddr & (HUGE_PAGE_SIZE - 1)));
    }

    let pt = unsafe { &*(pde.address() as *const PageTable) };
    let pte = &pt.entries[pt_index(vaddr)];
    if !pte.is_present() {
        return None;
    }

    Some(pte.address() | page_offset(vaddr))
}

/// Map a single 4 KiB page: virtual address `vaddr` → physical frame `phys`.
///
/// Allocates intermediate page tables (PDP, PD, PT) as needed using the
/// frame allocator.
///
/// # Arguments
/// - `pml4_phys` — Physical address of the PML4 to modify.
/// - `vaddr` — Virtual address to map (must be 4 KiB-aligned).
/// - `phys` — Physical frame address to map to (must be 4 KiB-aligned).
/// - `flags` — Page flags (PRESENT is always set automatically).
///
/// # Returns
/// `true` if the mapping was created, `false` if it failed (out of frames
/// for intermediate tables, or already mapped).
///
/// # Safety
/// - `pml4_phys` must point to a valid, identity-mapped page table.
/// - `vaddr` and `phys` must be 4 KiB-aligned.
/// - Creating conflicting mappings can cause undefined behavior.
pub unsafe fn map_page(pml4_phys: u64, vaddr: u64, phys: u64, flags: PageFlags) -> bool {
    debug_assert!(vaddr & 0xFFF == 0, "vaddr not page-aligned: {:#x}", vaddr);
    debug_assert!(phys & 0xFFF == 0, "phys not page-aligned: {:#x}", phys);

    let full_flags = flags | PageFlags::PRESENT;

    // Walk/create PML4 → PDP
    let pml4 = &mut *(pml4_phys as *mut PageTable);
    let pml4e = &mut pml4.entries[pml4_index(vaddr)];
    if !pml4e.is_present() {
        let frame = match frame_allocator::allocate_frame() {
            Some(f) => f,
            None => return false,
        };
        zero_frame(frame);
        pml4e.set(frame, PageFlags::PRESENT | PageFlags::WRITABLE | (flags & PageFlags::USER));
    }

    // Walk/create PDP → PD
    let pdp = &mut *(pml4e.address() as *mut PageTable);
    let pdpe = &mut pdp.entries[pdp_index(vaddr)];
    if !pdpe.is_present() {
        let frame = match frame_allocator::allocate_frame() {
            Some(f) => f,
            None => return false,
        };
        zero_frame(frame);
        pdpe.set(frame, PageFlags::PRESENT | PageFlags::WRITABLE | (flags & PageFlags::USER));
    } else if pdpe.is_huge() {
        // Already a 1 GiB huge page — can't map 4 KiB within it
        return false;
    }

    // Walk/create PD → PT
    let pd = &mut *(pdpe.address() as *mut PageTable);
    let pde = &mut pd.entries[pd_index(vaddr)];
    if !pde.is_present() {
        let frame = match frame_allocator::allocate_frame() {
            Some(f) => f,
            None => return false,
        };
        zero_frame(frame);
        pde.set(frame, PageFlags::PRESENT | PageFlags::WRITABLE | (flags & PageFlags::USER));
    } else if pde.is_huge() {
        // Already a 2 MiB huge page — can't map 4 KiB within it
        return false;
    }

    // Set the final PT entry
    let pt = &mut *(pde.address() as *mut PageTable);
    let pte = &mut pt.entries[pt_index(vaddr)];
    if pte.is_present() {
        // Already mapped — don't silently overwrite
        return false;
    }
    pte.set(phys, full_flags);

    // Invalidate the TLB for this address
    cpu::invlpg(vaddr);

    true
}

/// Unmap a single 4 KiB page and return the physical frame that was mapped.
///
/// Does NOT free the physical frame — the caller decides whether to return
/// it to the frame allocator.
///
/// # Returns
/// `Some(physical_address)` if the page was mapped, `None` if it wasn't.
///
/// # Safety
/// - `pml4_phys` must point to a valid, identity-mapped page table.
/// - `vaddr` must be 4 KiB-aligned.
/// - Unmapping pages that are in active use will cause page faults.
pub unsafe fn unmap_page(pml4_phys: u64, vaddr: u64) -> Option<u64> {
    debug_assert!(vaddr & 0xFFF == 0, "vaddr not page-aligned: {:#x}", vaddr);

    let pml4 = &mut *(pml4_phys as *mut PageTable);
    let pml4e = &pml4.entries[pml4_index(vaddr)];
    if !pml4e.is_present() {
        return None;
    }

    let pdp = &mut *(pml4e.address() as *mut PageTable);
    let pdpe = &pdp.entries[pdp_index(vaddr)];
    if !pdpe.is_present() || pdpe.is_huge() {
        return None;
    }

    let pd = &mut *(pdpe.address() as *mut PageTable);
    let pde = &pd.entries[pd_index(vaddr)];
    if !pde.is_present() || pde.is_huge() {
        return None;
    }

    let pt = &mut *(pde.address() as *mut PageTable);
    let pte = &mut pt.entries[pt_index(vaddr)];
    if !pte.is_present() {
        return None;
    }

    let phys = pte.address();
    pte.clear();
    cpu::invlpg(vaddr);

    Some(phys)
}

/// Map a contiguous range of 4 KiB pages.
///
/// Maps `count` pages starting at `vaddr` to physical frames starting at `phys`.
/// Both virtual and physical ranges must be contiguous.
///
/// # Returns
/// The number of pages successfully mapped. If this is less than `count`,
/// mapping failed partway through (out of frame allocator memory for tables).
///
/// # Safety
/// Same requirements as `map_page`.
pub unsafe fn map_range(
    pml4_phys: u64,
    vaddr: u64,
    phys: u64,
    count: usize,
    flags: PageFlags,
) -> usize {
    for i in 0..count {
        let offset = (i as u64) * PAGE_SIZE;
        if !map_page(pml4_phys, vaddr + offset, phys + offset, flags) {
            return i;
        }
    }
    count
}

/// Unmap a contiguous range of 4 KiB pages.
///
/// Unmaps `count` pages starting at `vaddr`. Does NOT free physical frames.
///
/// # Returns  
/// A vector of physical addresses that were unmapped (for the caller to free).
///
/// # Safety
/// Same requirements as `unmap_page`.
pub unsafe fn unmap_range(pml4_phys: u64, vaddr: u64, count: usize) -> alloc::vec::Vec<u64> {
    extern crate alloc;
    let mut frames = alloc::vec::Vec::with_capacity(count);

    for i in 0..count {
        let offset = (i as u64) * PAGE_SIZE;
        if let Some(phys) = unmap_page(pml4_phys, vaddr + offset) {
            frames.push(phys);
        }
    }

    frames
}

/// Update the flags on an existing 4 KiB page mapping.
///
/// # Returns
/// `true` if the page was present and flags were updated, `false` otherwise.
///
/// # Safety
/// Same as `map_page`.
pub unsafe fn update_flags(pml4_phys: u64, vaddr: u64, new_flags: PageFlags) -> bool {
    debug_assert!(vaddr & 0xFFF == 0);

    let pml4 = &mut *(pml4_phys as *mut PageTable);
    let pml4e = &pml4.entries[pml4_index(vaddr)];
    if !pml4e.is_present() {
        return false;
    }

    let pdp = &mut *(pml4e.address() as *mut PageTable);
    let pdpe = &pdp.entries[pdp_index(vaddr)];
    if !pdpe.is_present() || pdpe.is_huge() {
        return false;
    }

    let pd = &mut *(pdpe.address() as *mut PageTable);
    let pde = &pd.entries[pd_index(vaddr)];
    if !pde.is_present() || pde.is_huge() {
        return false;
    }

    let pt = &mut *(pde.address() as *mut PageTable);
    let pte = &mut pt.entries[pt_index(vaddr)];
    if !pte.is_present() {
        return false;
    }

    let phys = pte.address();
    pte.set(phys, new_flags | PageFlags::PRESENT);
    cpu::invlpg(vaddr);

    true
}

/// Create a new, empty PML4 page table for a user-mode process.
///
/// The kernel-half of the address space (entries 256..511) is shared by
/// copying PML4 entries from the kernel's PML4. This means all processes
/// see the same kernel mappings without duplication.
///
/// # Returns
/// Physical address of the new PML4, or `None` if allocation failed.
pub fn create_user_page_table() -> Option<u64> {
    let frame = frame_allocator::allocate_frame()?;
    unsafe {
        zero_frame(frame);

        let new_pml4 = &mut *(frame as *mut PageTable);
        let kernel_pml4 = &*(kernel_pml4() as *const PageTable);

        // Copy kernel-space PML4 entries (upper half: indices 256..512)
        // This shares the same PDP/PD/PT hierarchy — no deep copy needed
        for i in 256..512 {
            new_pml4.entries[i] = kernel_pml4.entries[i];
        }
    }

    Some(frame)
}

/// Free a user-mode page table hierarchy.
///
/// Frees the PML4 and all user-space page table pages (PDP, PD, PT at
/// indices 0..255). Does NOT free the physical frames that were mapped —
/// only the page table structure itself.
///
/// Does NOT touch kernel-space entries (256..511) since those are shared.
///
/// # Safety
/// - The page table must not be currently active (in CR3).
/// - No thread may be using this address space.
pub unsafe fn free_user_page_table(pml4_phys: u64) {
    let pml4 = &*(pml4_phys as *const PageTable);

    // Only free user-space entries (0..255)
    for i in 0..256 {
        let pml4e = &pml4.entries[i];
        if !pml4e.is_present() {
            continue;
        }

        let pdp = &*(pml4e.address() as *const PageTable);
        for j in 0..512 {
            let pdpe = &pdp.entries[j];
            if !pdpe.is_present() || pdpe.is_huge() {
                continue;
            }

            let pd = &*(pdpe.address() as *const PageTable);
            for k in 0..512 {
                let pde = &pd.entries[k];
                if !pde.is_present() || pde.is_huge() {
                    continue;
                }
                // Free the PT itself
                frame_allocator::free_frame(pde.address());
            }
            // Free the PD
            frame_allocator::free_frame(pdpe.address());
        }
        // Free the PDP
        frame_allocator::free_frame(pml4e.address());
    }

    // Free the PML4 itself
    frame_allocator::free_frame(pml4_phys);
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Zero out a physical frame (4 KiB).
///
/// # Safety
/// Requires the frame to be identity-mapped.
unsafe fn zero_frame(phys: u64) {
    let ptr = phys as *mut u8;
    core::ptr::write_bytes(ptr, 0, PAGE_SIZE as usize);
}
