// Page Table Setup for Kernel Handoff
//
// This module creates 4-level x86_64 page tables that map:
//   1. Identity map: physical address X → virtual address X (for the transition)
//   2. Higher-half: physical address of kernel → virtual 0xFFFFFFFF80000000+
//   3. Framebuffer: identity mapped for display output
//
// x86_64 Paging Primer:
//   Virtual addresses are translated through 4 levels of page tables:
//     PML4 (Page Map Level 4) → PDP (Page Directory Pointer) → PD (Page Directory) → PT (Page Table)
//   Each table has 512 entries, each entry is 8 bytes (64 bits).
//   A 4 KiB page requires all 4 levels.
//   A 2 MiB "huge page" only requires PML4 → PDP → PD (PD entry has PS bit set).
//
// We use 2 MiB huge pages for simplicity in the bootloader.
// The kernel can set up 4 KiB pages later for fine-grained control.
//
// Why higher-half?
//   The kernel lives at virtual addresses starting at 0xFFFFFFFF80000000.
//   This is the last 2 GiB of the 48-bit canonical address space.
//   User-mode processes get the lower half. The kernel is mapped into every
//   process's address space, so system calls don't require a page table switch.

use cantaya_shared::boot_info::FramebufferInfo;
use crate::loader::{KernelLoadInfo, PF_X, PF_W};
use log::info;
use uefi::boot;
use uefi::mem::memory_map::MemoryType;

/// Page table entry flags
const PRESENT: u64 = 1 << 0;
const WRITABLE: u64 = 1 << 1;
const HUGE_PAGE: u64 = 1 << 7;  // 2 MiB page (in PD entry)
const NO_EXECUTE: u64 = 1 << 63; // NX/XD bit — requires IA32_EFER.NXE

/// Size constants
const HUGE_PAGE_SIZE: u64 = 2 * 1024 * 1024; // 2 MiB
const PAGE_SIZE: u64 = 4096;                  // 4 KiB

/// The virtual base address for the higher-half kernel (must match linker.ld)
const KERNEL_VADDR_BASE: u64 = 0xFFFFFFFF80000000;

/// IA32_EFER MSR address (Extended Feature Enable Register)
const IA32_EFER: u32 = 0xC000_0080;
/// NXE bit within IA32_EFER (bit 11): enables NO_EXECUTE page protection
const EFER_NXE: u64 = 1 << 11;

// ============================================================================
// Page Table Allocation Tracking
//
// We track every page allocated for page tables so `main.rs` can retag
// those physical pages in the memory map as `PageTables`.
// ============================================================================

static mut PT_PAGES: [u64; 128] = [0u64; 128];
static mut PT_PAGE_COUNT: usize = 0;

/// Returns the list of physical addresses of all page table pages allocated.
pub fn page_table_pages() -> &'static [u64] {
    unsafe { &PT_PAGES[..PT_PAGE_COUNT] }
}

/// Allocate a zeroed 4 KiB page for use as a page table.
///
/// Page tables must be page-aligned (4 KiB) and zeroed.
/// We use UEFI's page allocator for this.
fn allocate_page_table() -> u64 {
    let addr = boot::allocate_pages(
        uefi::boot::AllocateType::AnyPages,
        MemoryType::LOADER_DATA,
        1,
    )
    .expect("Failed to allocate page table page")
    .as_ptr() as u64;

    // Zero the page table
    unsafe {
        core::ptr::write_bytes(addr as *mut u8, 0, 4096);
    }

    // Track this allocation for later memory-map tagging
    unsafe {
        if PT_PAGE_COUNT < 128 {
            PT_PAGES[PT_PAGE_COUNT] = addr;
            PT_PAGE_COUNT += 1;
        }
    }

    addr
}

/// Get or create a next-level page table entry.
///
/// If the entry at `index` in `table` already points to a table, return that table's address.
/// Otherwise, allocate a new table, install it in the entry, and return its address.
fn get_or_create_table(table: u64, index: usize) -> u64 {
    let entry_ptr = (table + (index as u64) * 8) as *mut u64;
    let entry = unsafe { *entry_ptr };

    if entry & PRESENT != 0 {
        // Entry already exists — extract the physical address (bits 12..51)
        entry & 0x000F_FFFF_FFFF_F000
    } else {
        // Allocate a new table
        let new_table = allocate_page_table();
        unsafe {
            *entry_ptr = new_table | PRESENT | WRITABLE;
        }
        new_table
    }
}

/// Map a 2 MiB huge page: virtual_addr → physical_addr.
///
/// Both addresses MUST be 2 MiB-aligned. Used for the identity map.
fn map_huge_page(pml4: u64, virtual_addr: u64, physical_addr: u64) {
    let pml4_idx = ((virtual_addr >> 39) & 0x1FF) as usize;
    let pdp_idx = ((virtual_addr >> 30) & 0x1FF) as usize;
    let pd_idx = ((virtual_addr >> 21) & 0x1FF) as usize;

    let pdp = get_or_create_table(pml4, pml4_idx);
    let pd = get_or_create_table(pdp, pdp_idx);

    let entry_ptr = (pd + (pd_idx as u64) * 8) as *mut u64;
    unsafe {
        *entry_ptr = physical_addr | PRESENT | WRITABLE | HUGE_PAGE;
    }
}

/// Map a single 4 KiB page: virtual_addr → physical_addr.
///
/// Used for the kernel mapping where the physical memory may not be 2 MiB-aligned.
fn map_4k_page(pml4: u64, virtual_addr: u64, physical_addr: u64) {
    map_4k_page_flags(pml4, virtual_addr, physical_addr, PRESENT | WRITABLE);
}

/// Map a single 4 KiB page with explicit flags.
///
/// The `flags` argument sets the leaf PTE flags (PRESENT, WRITABLE, NO_EXECUTE, etc.).
/// Intermediate page table entries always use PRESENT | WRITABLE.
fn map_4k_page_flags(pml4: u64, virtual_addr: u64, physical_addr: u64, flags: u64) {
    let pml4_idx = ((virtual_addr >> 39) & 0x1FF) as usize;
    let pdp_idx = ((virtual_addr >> 30) & 0x1FF) as usize;
    let pd_idx = ((virtual_addr >> 21) & 0x1FF) as usize;
    let pt_idx = ((virtual_addr >> 12) & 0x1FF) as usize;

    let pdp = get_or_create_table(pml4, pml4_idx);
    let pd = get_or_create_table(pdp, pdp_idx);
    let pt = get_or_create_table(pd, pd_idx);

    let entry_ptr = (pt + (pt_idx as u64) * 8) as *mut u64;
    unsafe {
        *entry_ptr = physical_addr | flags;
    }
}

/// Enable the NX/XD (No-Execute) bit via the IA32_EFER MSR.
///
/// This must be done before page tables with NO_EXECUTE entries are activated.
/// On CPUs without NX support, this is a no-op (CPUID check first).
pub fn enable_nx_bit() -> bool {
    unsafe {
        // Check CPUID for NX support: CPUID EAX=0x80000001, EDX bit 20
        let edx: u32;
        core::arch::asm!(
            "push rbx",
            "mov eax, 0x80000001",
            "cpuid",
            "pop rbx",
            lateout("edx") edx,
            lateout("eax") _,
            lateout("ecx") _,
        );
        if edx & (1 << 20) == 0 {
            info!("CPU does not support NX/XD bit");
            return false;
        }

        // Read current EFER
        let lo: u32;
        let hi: u32;
        core::arch::asm!("rdmsr", in("ecx") IA32_EFER, out("eax") lo, out("edx") hi);
        let efer = ((hi as u64) << 32) | (lo as u64);

        // Set NXE bit
        let new_efer = efer | EFER_NXE;
        let new_lo = new_efer as u32;
        let new_hi = (new_efer >> 32) as u32;
        core::arch::asm!("wrmsr", in("ecx") IA32_EFER, in("eax") new_lo, in("edx") new_hi);

        info!("NX/XD bit enabled (EFER: {:#X} → {:#X})", efer, new_efer);
        true
    }
}

/// Create page tables for the kernel with both identity mapping and higher-half mapping.
///
/// Returns the physical address of the PML4 table (to be loaded into CR3).
pub fn setup_kernel_page_tables(kernel: &KernelLoadInfo, fb: &FramebufferInfo, nx_enabled: bool) -> u64 {
    let pml4 = allocate_page_table();
    info!("PML4 at physical: {:#X}", pml4);

    // 1. Identity map the first 4 GiB of physical memory using 2 MiB pages.
    //    This is needed so the CPU can continue executing after we switch page tables,
    //    since our code is currently running at its physical address.
    //    We do NOT set NX here because the bootloader code runs from identity-mapped
    //    addresses between the CR3 switch and the kernel jump.
    let four_gib = 4u64 * 1024 * 1024 * 1024;
    let mut addr: u64 = 0;
    while addr < four_gib {
        map_huge_page(pml4, addr, addr);
        addr += HUGE_PAGE_SIZE;
    }
    info!("Identity mapped first 4 GiB");

    // 2. Map kernel segments to the higher half with per-segment permissions.
    //    Each PT_LOAD segment gets the correct NX/W flags from ELF p_flags.
    //    This provides W^X (write XOR execute) protection for the kernel.
    if kernel.segment_count > 0 && nx_enabled {
        // Per-segment mapping with correct permissions
        for i in 0..kernel.segment_count {
            let seg = &kernel.segments[i];
            let seg_start = seg.offset_from_base;
            let seg_pages = (seg.size + PAGE_SIZE - 1) / PAGE_SIZE;

            // Compute page table flags from ELF p_flags
            let mut flags = PRESENT;
            if seg.flags & PF_W != 0 {
                flags |= WRITABLE;
            }
            if seg.flags & PF_X == 0 {
                flags |= NO_EXECUTE;
            }

            let flag_desc = [
                if seg.flags & PF_X != 0 { 'X' } else { '-' },
                if seg.flags & PF_W != 0 { 'W' } else { '-' },
            ];
            info!(
                "  Segment {}: {} pages, flags={}{}, base_offset={:#X}",
                i, seg_pages, flag_desc[0], flag_desc[1], seg_start
            );

            for p in 0..seg_pages {
                let phys = kernel.physical_base + seg_start + p * PAGE_SIZE;
                let virt = KERNEL_VADDR_BASE + seg_start + p * PAGE_SIZE;
                map_4k_page_flags(pml4, virt, phys, flags);
            }
        }
        info!("Higher-half mapped {} kernel segments with NX permissions", kernel.segment_count);
    } else {
        // Fallback: map all kernel pages as RW (no NX support or no segment info)
        let kernel_4k_pages = (kernel.size + PAGE_SIZE - 1) / PAGE_SIZE;
        for i in 0..kernel_4k_pages {
            let phys = kernel.physical_base + i * PAGE_SIZE;
            let virt = KERNEL_VADDR_BASE + i * PAGE_SIZE;
            map_4k_page(pml4, virt, phys);
        }
        info!(
            "Higher-half mapped {} pages for kernel ({} KiB) [no NX]",
            kernel_4k_pages,
            kernel_4k_pages * 4
        );
    }

    // 3. Identity map the framebuffer (so the kernel can write to it directly).
    let fb_pages = (fb.size + HUGE_PAGE_SIZE - 1) / HUGE_PAGE_SIZE;
    let fb_base_aligned = fb.address & !(HUGE_PAGE_SIZE - 1);
    for i in 0..fb_pages {
        let addr = fb_base_aligned + i * HUGE_PAGE_SIZE;
        map_huge_page(pml4, addr, addr);
    }
    info!("Identity mapped framebuffer ({} pages)", fb_pages);

    info!("Page tables allocated: {} pages ({} KiB)",
        unsafe { PT_PAGE_COUNT },
        unsafe { PT_PAGE_COUNT } * 4
    );

    pml4
}

/// Activate the new page tables and jump to the kernel entry point.
///
/// SAFETY: This function never returns. It:
///   1. Loads our page tables into CR3
///   2. Sets RSP to `stack_top` (should be the kernel's __kernel_stack_top)
///   3. Jumps to the kernel entry point with the BootInfo pointer in RDI
///
/// The kernel's `_start` immediately loads its own stack from the linker symbol,
/// so `stack_top` only needs to be a valid mapped address that survives the `jmp`.
pub unsafe fn activate_and_jump_to_kernel(
    pml4_addr: u64,
    kernel_entry: u64,
    boot_info_addr: u64,
    stack_top: u64,
) -> ! {
    core::arch::asm!(
        // Load our page tables
        "mov cr3, {pml4}",
        // Set RSP to end of kernel image (near __kernel_stack_top)
        "mov rsp, {stack}",
        // Jump to kernel entry with boot_info pointer in rdi
        "mov rdi, {boot_info}",
        "jmp {entry}",
        pml4 = in(reg) pml4_addr,
        stack = in(reg) stack_top,
        boot_info = in(reg) boot_info_addr,
        entry = in(reg) kernel_entry,
        options(noreturn)
    );
}
