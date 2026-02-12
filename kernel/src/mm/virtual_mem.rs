//! Virtual Memory Manager
//!
//! Implements 4-level AArch64 page tables (PGD → PUD → PMD → PTE)
//! with identity mapping for devices, kernel high-half mapping,
//! and user-space address space management.

use core::sync::atomic::{AtomicUsize, Ordering};
use crate::arch::mmu::{PAGE_SIZE, PageFlags, invalidate_tlb};

/// Virtual address space regions (Windows NT-like layout)
pub const KERNEL_BASE: usize = 0xFFFF_0000_0000_0000;
pub const USER_BASE: usize = 0x0000_0000_0001_0000;
pub const USER_STACK_TOP: usize = 0x0000_7FFF_FFFF_0000;
pub const DEVICE_BASE: usize = 0x0000_0000_0000_0000;

/// Page table index extraction helpers
#[allow(dead_code)]
const VA_BITS: usize = 48;
#[allow(dead_code)]
const PAGE_SHIFT: usize = 12;
#[allow(dead_code)]
const TABLE_SHIFT: usize = 9;
const ENTRIES_PER_TABLE: usize = 512;

#[inline]
fn pgd_index(va: usize) -> usize {
    (va >> 39) & 0x1FF
}

#[inline]
fn pud_index(va: usize) -> usize {
    (va >> 30) & 0x1FF
}

#[inline]
fn pmd_index(va: usize) -> usize {
    (va >> 21) & 0x1FF
}

#[inline]
fn pte_index(va: usize) -> usize {
    (va >> 12) & 0x1FF
}

/// Page table entry
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    pub const fn empty() -> Self {
        PageTableEntry(0)
    }

    pub fn is_valid(&self) -> bool {
        (self.0 & 1) != 0
    }

    pub fn is_table(&self) -> bool {
        self.is_valid() && (self.0 & 0b10) != 0
    }

    pub fn address(&self) -> usize {
        (self.0 & 0x0000_FFFF_FFFF_F000) as usize
    }

    pub fn flags(&self) -> u64 {
        self.0 & !0x0000_FFFF_FFFF_F000
    }

    pub fn set(&mut self, addr: usize, flags: PageFlags) {
        self.0 = (addr as u64 & 0x0000_FFFF_FFFF_F000) | flags.bits();
    }

    pub fn set_raw(&mut self, value: u64) {
        self.0 = value;
    }

    pub fn clear(&mut self) {
        self.0 = 0;
    }
}

/// 4-level page table (4KB aligned)
#[repr(C, align(4096))]
pub struct PageTable {
    pub entries: [PageTableEntry; ENTRIES_PER_TABLE],
}

impl PageTable {
    pub const fn new() -> Self {
        PageTable {
            entries: [PageTableEntry::empty(); ENTRIES_PER_TABLE],
        }
    }
}

/// Static page table pool for early boot (before heap is available)
/// We pre-allocate a set of page tables in BSS
const MAX_EARLY_TABLES: usize = 96;

#[repr(C, align(4096))]
struct EarlyTablePool {
    tables: [PageTable; MAX_EARLY_TABLES],
}

impl EarlyTablePool {
    const fn new() -> Self {
        const EMPTY_TABLE: PageTable = PageTable::new();
        EarlyTablePool {
            tables: [EMPTY_TABLE; MAX_EARLY_TABLES],
        }
    }
}

static mut EARLY_TABLES: EarlyTablePool = EarlyTablePool::new();
static NEXT_EARLY_TABLE: AtomicUsize = AtomicUsize::new(0);

/// Kernel PGD (Level 0 page table)
static mut KERNEL_PGD: PageTable = PageTable::new();

/// Allocate a page table from early pool or physical allocator
fn alloc_page_table() -> *mut PageTable {
    let idx = NEXT_EARLY_TABLE.load(Ordering::SeqCst);
    if idx < MAX_EARLY_TABLES {
        return alloc_early_table();
    }
    // Post-boot: allocate a zeroed 4KB-aligned physical frame
    if let Some(frame) = crate::mm::physical::alloc_frame() {
        unsafe {
            core::ptr::write_bytes(frame as *mut u8, 0, PAGE_SIZE);
        }
        frame as *mut PageTable
    } else {
        panic!("Out of memory for page table allocation");
    }
}

/// Allocate a page table from the early pool (boot-time only)
fn alloc_early_table() -> *mut PageTable {
    let idx = NEXT_EARLY_TABLE.fetch_add(1, Ordering::SeqCst);
    if idx >= MAX_EARLY_TABLES {
        panic!("Early page table pool exhausted");
    }
    unsafe { &raw mut EARLY_TABLES.tables[idx] }
}

/// Walk page tables to find or create the PTE for a given VA.
/// Creates intermediate tables as needed.
unsafe fn walk_page_tables(pgd: *mut PageTable, va: usize, create: bool) -> Option<*mut PageTableEntry> {
    let pgd_ref = &mut *pgd;
    let l0_idx = pgd_index(va);

    // Level 0 → Level 1 (PUD)
    let pud = if pgd_ref.entries[l0_idx].is_valid() {
        pgd_ref.entries[l0_idx].address() as *mut PageTable
    } else if create {
        let table = alloc_page_table();
        pgd_ref.entries[l0_idx].set(
            table as usize,
            PageFlags::VALID | PageFlags::TABLE,
        );
        table
    } else {
        return None;
    };

    // Level 1 → Level 2 (PMD)
    let pud_ref = &mut *pud;
    let l1_idx = pud_index(va);

    let pmd = if pud_ref.entries[l1_idx].is_valid() {
        pud_ref.entries[l1_idx].address() as *mut PageTable
    } else if create {
        let table = alloc_page_table();
        pud_ref.entries[l1_idx].set(
            table as usize,
            PageFlags::VALID | PageFlags::TABLE,
        );
        table
    } else {
        return None;
    };

    // Level 2 → Level 3 (PTE table)
    let pmd_ref = &mut *pmd;
    let l2_idx = pmd_index(va);

    let pt = if pmd_ref.entries[l2_idx].is_valid() {
        pmd_ref.entries[l2_idx].address() as *mut PageTable
    } else if create {
        let table = alloc_page_table();
        pmd_ref.entries[l2_idx].set(
            table as usize,
            PageFlags::VALID | PageFlags::TABLE,
        );
        table
    } else {
        return None;
    };

    // Level 3: return pointer to the PTE
    let pt_ref = &mut *pt;
    let l3_idx = pte_index(va);
    Some(&mut pt_ref.entries[l3_idx] as *mut PageTableEntry)
}

/// Initialize virtual memory
pub fn init() {
    setup_kernel_mappings();
    load_kernel_page_tables();
}

/// Setup identity and kernel mappings
fn setup_kernel_mappings() {
    // Identity map first 1GB for devices (MMIO regions, UART, GIC, etc.)
    // Using 4KB pages for the first 512MB
    let device_region_end: usize = 0x4000_0000; // 1GB  
    let _step = 2 * 1024 * 1024; // Map in 2MB increments for efficiency

    // Map device MMIO region — only map the specific device ranges we use:
    //   0x0000_0000           : 1 page  (misc device)
    //   0x0800_0000..0800_3FFF: 4 pages (GIC Distributor — GICD)
    //   0x080A_0000..080B_FFFF: 32 pages (GIC Redistributor — GICR, RD+SGI 128KB)
    //   0x0900_0000..0900_3FFF: 4 pages (PL011 UART)
    //   0x0902_0000..0902_0FFF: 1 page  (fw-cfg MMIO)
    //   0x0A00_0000..0A00_3FFF: 16 pages (virtio-mmio transports, 32 slots × 0x200)
    let mut addr: usize = 0;
    while addr < device_region_end {
        map_page_internal(
            addr,
            addr,
            PageFlags::VALID | PageFlags::PAGE | PageFlags::ACCESSED
                | PageFlags::ATTR_DEVICE,
        );
        addr += PAGE_SIZE;
        // Only map key device pages to save table space
        if addr == PAGE_SIZE {
            // Jump to GIC Distributor
            addr = 0x0800_0000;
        } else if addr == 0x0800_0000 + PAGE_SIZE * 4 {
            // Jump to GIC Redistributor (RD_base 64KB + SGI_base 64KB = 128KB)
            addr = 0x080A_0000;
        } else if addr == 0x080A_0000 + PAGE_SIZE * 32 {
            // Jump to UART
            addr = 0x0900_0000;
        } else if addr == 0x0900_0000 + PAGE_SIZE * 4 {
            // Jump to PL031 RTC
            addr = 0x0901_0000;
        } else if addr == 0x0901_0000 + PAGE_SIZE {
            // Jump to fw-cfg
            addr = 0x0902_0000;
        } else if addr == 0x0902_0000 + PAGE_SIZE {
            // Jump to virtio-mmio transports
            addr = 0x0A00_0000;
        } else if addr == 0x0A00_0000 + PAGE_SIZE * 16 {
            // Jump to RAM start (end of device region)
            addr = 0x4000_0000;
        }
    }

    // Identity map initial RAM region (0x4000_0000 .. 0x4400_0000) = 64MB
    // Covers kernel code/data, BSS, heap (4MB), and working memory.
    // The rest of the 2GB is demand-paged via the fault handler.
    let ram_start: usize = 0x4000_0000;
    let ram_end: usize = 0x4400_0000; // 64MB
    addr = ram_start;
    while addr < ram_end {
        map_page_internal(
            addr,
            addr,
            PageFlags::VALID | PageFlags::PAGE | PageFlags::ACCESSED
                | PageFlags::INNER_SHAREABLE | PageFlags::ATTR_NORMAL_WB,
        );
        addr += PAGE_SIZE;
    }
}

/// Internal mapping function used during init
fn map_page_internal(virt: usize, phys: usize, flags: PageFlags) {
    unsafe {
        let pgd = &raw mut KERNEL_PGD;
        if let Some(pte) = walk_page_tables(pgd, virt, true) {
            (*pte).set(phys, flags | PageFlags::VALID | PageFlags::PAGE | PageFlags::ACCESSED);
        }
    }
}

/// Load kernel page tables into page table register
fn load_kernel_page_tables() {
    unsafe {
        let pgd_addr = &raw const KERNEL_PGD as u64;
        #[cfg(target_arch = "aarch64")]
        core::arch::asm!(
            "msr ttbr0_el1, {pgd}",
            "isb",
            "tlbi vmalle1is",
            "dsb ish",
            "isb",
            pgd = in(reg) pgd_addr,
        );
        
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "mov cr3, {pgd}",
            pgd = in(reg) pgd_addr,
        );
    }
}

/// Map a virtual address to physical address
pub fn map_page(virt: usize, phys: usize, flags: PageFlags) {
    unsafe {
        let pgd = &raw mut KERNEL_PGD;
        if let Some(pte) = walk_page_tables(pgd, virt, true) {
            (*pte).set(phys, flags | PageFlags::VALID | PageFlags::PAGE | PageFlags::ACCESSED);
        }
    }
    invalidate_tlb();
}

/// Unmap a virtual address
pub fn unmap_page(virt: usize) {
    unsafe {
        let pgd = &raw mut KERNEL_PGD;
        if let Some(pte) = walk_page_tables(pgd, virt, false) {
            (*pte).clear();
        }
    }
    invalidate_tlb();
}

/// Translate virtual address to physical
pub fn virt_to_phys(virt: usize) -> Option<usize> {
    unsafe {
        let pgd = &raw mut KERNEL_PGD;
        if let Some(pte) = walk_page_tables(pgd, virt, false) {
            if (*pte).is_valid() {
                let page_offset = virt & (PAGE_SIZE - 1);
                return Some((*pte).address() | page_offset);
            }
        }
    }
    None
}

/// Handle page fault. Returns `true` if resolved, `false` if unresolvable.
pub fn handle_fault(address: u64) -> bool {
    let addr = address as usize;

    // Check if this is a valid kernel address that just needs mapping
    if addr >= 0x4000_0000 && addr < 0xC000_0000 {
        // Demand-page kernel RAM: allocate a frame and map it
        if let Some(frame) = crate::mm::physical::alloc_frame() {
            map_page(
                addr & !(PAGE_SIZE - 1),
                frame,
                PageFlags::VALID | PageFlags::PAGE | PageFlags::ACCESSED
                    | PageFlags::INNER_SHAREABLE | PageFlags::ATTR_NORMAL_WB,
            );
            return true;
        }
    }

    false
}

/// Virtual Address Descriptor (VAD) - Windows-like memory region tracking
#[derive(Clone, Copy, Debug)]
pub struct VadEntry {
    pub base: usize,
    pub size: usize,
    pub flags: VadFlags,
}

#[derive(Clone, Copy, Debug)]
pub enum VadFlags {
    ReadOnly,
    ReadWrite,
    ReadExecute,
    ReadWriteExecute,
    Guard,
    NoAccess,
}

/// Allocate a range of virtual memory (NtAllocateVirtualMemory implementation).
/// Maps into the currently active process's address space (TTBR0_EL1).
pub fn allocate_virtual_memory(base: usize, size: usize) -> Option<usize> {
    let pgd = current_pgd_phys();
    let aligned_size = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let base_aligned = if base == 0 {
        // Find free region - simple bump for now
        static NEXT_USER_VA: AtomicUsize = AtomicUsize::new(0x0000_0000_1000_0000);
        NEXT_USER_VA.fetch_add(aligned_size, Ordering::SeqCst)
    } else {
        base & !(PAGE_SIZE - 1)
    };

    // Map pages into the active process's page tables
    let mut offset = 0;
    while offset < aligned_size {
        if let Some(frame) = crate::mm::physical::alloc_frame() {
            map_page_in(
                pgd,
                base_aligned + offset,
                frame,
                PageFlags::VALID | PageFlags::PAGE | PageFlags::ACCESSED
                    | PageFlags::USER | PageFlags::INNER_SHAREABLE
                    | PageFlags::ATTR_NORMAL_WB,
            );
        } else {
            return None; // Out of memory
        }
        offset += PAGE_SIZE;
    }

    Some(base_aligned)
}

/// Free a range of virtual memory in the currently active address space.
pub fn free_virtual_memory(base: usize, size: usize) {
    let pgd = current_pgd_phys();
    let aligned_size = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let base_aligned = base & !(PAGE_SIZE - 1);

    let mut offset = 0;
    while offset < aligned_size {
        let va = base_aligned + offset;
        if let Some(pa) = virt_to_phys_in(pgd, va) {
            unmap_page_in(pgd, va);
            crate::mm::physical::free_frame(pa & !(PAGE_SIZE - 1));
        }
        offset += PAGE_SIZE;
    }
}

// ---------------------------------------------------------------------------
// Per-process address space support
// ---------------------------------------------------------------------------

/// Get the physical address of the kernel PGD (used for PID 0 / System process)
pub fn kernel_pgd_phys() -> usize {
    (&raw const KERNEL_PGD) as usize
}

/// Clone the kernel page tables for a new process.
///
/// Strategy: **share** kernel subtrees so that new kernel mappings (heap
/// growth, demand-paging) are automatically visible in every process.
///
///   PGD level — shallow-copy all entries (share PUD subtrees).
///   PGD\[0\] is special: it covers both kernel RAM (PUD\[1\]: 0x4000_0000)
///   and user space (PUD\[0\]).  We allocate a per-process PUD for PGD\[0\]
///   and a per-process PMD for PUD\[0\] so that `map_page_in` can create
///   user PTE tables without modifying shared kernel structures.
///   PUD\[1+\] entries inside the per-process PUD still point to the
///   kernel's PMD/PTE trees — kernel writes are visible everywhere.
pub fn clone_kernel_tables() -> usize {
    unsafe {
        let kernel_pgd = &*(&raw const KERNEL_PGD);

        // 1. New PGD — shallow-copy all 512 entries
        let new_pgd = &mut *alloc_page_table();
        for i in 0..ENTRIES_PER_TABLE {
            new_pgd.entries[i] = kernel_pgd.entries[i];
        }

        // 2. PGD[0] contains mixed kernel/user VAs → per-process PUD
        if kernel_pgd.entries[0].is_valid() && kernel_pgd.entries[0].is_table() {
            let kernel_pud = &*(kernel_pgd.entries[0].address() as *const PageTable);
            let new_pud = &mut *alloc_page_table();

            // Shallow-copy all PUD entries (shares kernel PMD/PTE trees)
            for j in 0..ENTRIES_PER_TABLE {
                new_pud.entries[j] = kernel_pud.entries[j];
            }

            // PUD[0] covers 0x0–0x4000_0000 (devices + future user code).
            // Give the process its own PMD so user PTE tables don't leak.
            if kernel_pud.entries[0].is_valid() && kernel_pud.entries[0].is_table() {
                let kernel_pmd = &*(kernel_pud.entries[0].address() as *const PageTable);
                let new_pmd = &mut *alloc_page_table();

                // Copy existing device-range PMD entries (they share PTE
                // tables with the kernel — device mappings are immutable).
                for k in 0..ENTRIES_PER_TABLE {
                    new_pmd.entries[k] = kernel_pmd.entries[k];
                }

                new_pud.entries[0].set(
                    new_pmd as *mut PageTable as usize,
                    PageFlags::VALID | PageFlags::TABLE,
                );
            }

            new_pgd.entries[0].set(
                new_pud as *mut PageTable as usize,
                PageFlags::VALID | PageFlags::TABLE,
            );
        }

        new_pgd as *mut PageTable as usize
    }
}

/// Map a page in a specific process's page table (identified by PGD phys addr).
pub fn map_page_in(pgd_phys: usize, virt: usize, phys: usize, flags: PageFlags) {
    unsafe {
        let pgd = pgd_phys as *mut PageTable;
        if let Some(pte) = walk_page_tables(pgd, virt, true) {
            (*pte).set(phys, flags | PageFlags::VALID | PageFlags::PAGE | PageFlags::ACCESSED);
        }
    }
    invalidate_tlb();
}

/// Switch the active address space by writing a new PGD physical address
/// to the page table register and flushing the TLB.
pub fn switch_page_tables(pgd_phys: usize) {
    unsafe {
        #[cfg(target_arch = "aarch64")]
        core::arch::asm!(
            "msr ttbr0_el1, {pgd}",
            "isb",
            "tlbi vmalle1is",
            "dsb ish",
            "isb",
            pgd = in(reg) pgd_phys as u64,
        );
        
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "mov cr3, {pgd}",
            pgd = in(reg) pgd_phys as u64,
        );
    }
}

/// Read the currently active PGD physical address from the page table register.
pub fn current_pgd_phys() -> usize {
    let pgd: u64;
    unsafe {
        #[cfg(target_arch = "aarch64")]
        core::arch::asm!("mrs {}, ttbr0_el1", out(reg) pgd);
        
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("mov {}, cr3", out(reg) pgd);
    }
    (pgd & 0x0000_FFFF_FFFF_F000) as usize
}

/// Translate a virtual address using a specific process's page tables.
pub fn virt_to_phys_in(pgd_phys: usize, virt: usize) -> Option<usize> {
    unsafe {
        let pgd = pgd_phys as *mut PageTable;
        if let Some(pte) = walk_page_tables(pgd, virt, false) {
            if (*pte).is_valid() {
                let page_offset = virt & (PAGE_SIZE - 1);
                return Some((*pte).address() | page_offset);
            }
        }
    }
    None
}

/// Unmap a page in a specific process's page tables.
pub fn unmap_page_in(pgd_phys: usize, virt: usize) {
    unsafe {
        let pgd = pgd_phys as *mut PageTable;
        if let Some(pte) = walk_page_tables(pgd, virt, false) {
            (*pte).clear();
        }
    }
    invalidate_tlb();
}

/// Free per-process page table structures allocated by `clone_kernel_tables`.
///
/// This frees:
///   1. User-range PTE tables and their backing physical frames (PGD[0] / PUD[0])
///   2. Any PTE tables / physical frames at other PGD indices that only the
///      process uses (e.g. user stack at PGD[255])
///   3. The per-process PMD (PGD[0]/PUD[0])
///   4. The per-process PUD (PGD[0])
///   5. The PGD itself
///
/// **Must not** free shared kernel subtrees (PUD[1+] inside PGD[0], or
/// PGD entries that point directly into the kernel's PUD/PMD trees).
///
/// `pgd_phys` must NOT be `kernel_pgd_phys()`.
pub fn free_process_page_tables(pgd_phys: usize) {
    let kernel = kernel_pgd_phys();
    if pgd_phys == kernel || pgd_phys == 0 {
        return; // never free the kernel's own tables
    }
    unsafe {
        let pgd = &mut *(pgd_phys as *mut PageTable);
        let kernel_pgd = &*(&raw const KERNEL_PGD);

        for i in 0..ENTRIES_PER_TABLE {
            if !pgd.entries[i].is_valid() || !pgd.entries[i].is_table() {
                continue;
            }
            let pud_addr = pgd.entries[i].address();

            // If this PGD entry points to the same PUD as the kernel, it is
            // a shared subtree — skip it entirely.
            if kernel_pgd.entries[i].is_valid()
                && kernel_pgd.entries[i].address() == pud_addr
            {
                continue;
            }

            // This PUD is per-process (e.g. PGD[0]'s new PUD, or a PGD
            // entry created by map_page_in for user-only ranges like
            // PGD[255]).
            let pud = &mut *(pud_addr as *mut PageTable);
            let kernel_pud = if kernel_pgd.entries[i].is_valid() {
                Some(&*(kernel_pgd.entries[i].address() as *const PageTable))
            } else {
                None
            };

            for j in 0..ENTRIES_PER_TABLE {
                if !pud.entries[j].is_valid() || !pud.entries[j].is_table() {
                    continue;
                }
                let pmd_addr = pud.entries[j].address();

                // If the kernel's PUD has the same PMD pointer, it's shared.
                if let Some(kpud) = kernel_pud {
                    if kpud.entries[j].is_valid()
                        && kpud.entries[j].address() == pmd_addr
                    {
                        continue; // shared kernel PMD — don't free
                    }
                }

                // Per-process PMD — walk PTEs
                let pmd = &mut *(pmd_addr as *mut PageTable);
                let kernel_pmd = kernel_pud.and_then(|kpud| {
                    if kpud.entries[j].is_valid() {
                        Some(&*(kpud.entries[j].address() as *const PageTable))
                    } else {
                        None
                    }
                });

                for k in 0..ENTRIES_PER_TABLE {
                    if !pmd.entries[k].is_valid() || !pmd.entries[k].is_table() {
                        continue;
                    }
                    let pte_tbl_addr = pmd.entries[k].address();

                    // Skip PTE tables shared with the kernel (device entries)
                    if let Some(kpmd) = kernel_pmd {
                        if kpmd.entries[k].is_valid()
                            && kpmd.entries[k].address() == pte_tbl_addr
                        {
                            continue;
                        }
                    }

                    // Per-process PTE table — free leaf physical frames
                    let pte_tbl = &*(pte_tbl_addr as *mut PageTable);
                    for l in 0..ENTRIES_PER_TABLE {
                        if pte_tbl.entries[l].is_valid() {
                            let frame = pte_tbl.entries[l].address();
                            crate::mm::physical::free_frame(frame);
                        }
                    }
                    // Free the PTE table page itself
                    free_page_table(pte_tbl_addr);
                }
                // Free the PMD page
                free_page_table(pmd_addr);
            }
            // Free the PUD page
            free_page_table(pud_addr);
        }
        // Free the PGD page
        free_page_table(pgd_phys);
    }
}

/// Free a page-table page back to the physical allocator.
/// Pages from the early boot pool are never freed (they live in BSS).
fn free_page_table(addr: usize) {
    let pool_start = (&raw const EARLY_TABLES) as usize;
    let pool_end = pool_start + core::mem::size_of::<EarlyTablePool>();
    if addr >= pool_start && addr < pool_end {
        return; // early-pool page — cannot free
    }
    crate::mm::physical::free_frame(addr);
}
