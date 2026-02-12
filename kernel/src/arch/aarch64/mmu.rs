//! Memory Management Unit for AArch64

use core::arch::asm;
use bitflags::bitflags;

/// Page size: 4KB
pub const PAGE_SIZE: usize = 4096;
pub const PAGE_SHIFT: usize = 12;

bitflags! {
    /// Page table entry flags
    #[derive(Clone, Copy, Debug)]
    pub struct PageFlags: u64 {
        const VALID = 1 << 0;
        const TABLE = 1 << 1;
        const PAGE = 1 << 1;
        const ACCESSED = 1 << 10;
        const NOT_GLOBAL = 1 << 11;
        const READ_ONLY = 1 << 7;
        const USER = 1 << 6;
        const INNER_SHAREABLE = 0b11 << 8;
        const OUTER_SHAREABLE = 0b10 << 8;
        const EXECUTE_NEVER = 1 << 54;
        const PRIVILEGED_EXECUTE_NEVER = 1 << 53;

        /// MAIR attribute index 0 — Device-nGnRnE (strongly ordered, no caching)
        const ATTR_DEVICE = 0b000 << 2;
        /// MAIR attribute index 1 — Normal, Non-Cacheable
        const ATTR_NORMAL_NC = 0b001 << 2;
        /// MAIR attribute index 2 — Normal, Write-Back, Read/Write Allocate
        const ATTR_NORMAL_WB = 0b010 << 2;
    }
}

/// Initialize MMU
pub fn init() {
    setup_mair();
    setup_tcr();
    // Note: TTBR0_EL1 must be loaded before enabling the MMU.
    // virtual_mem::init() handles loading TTBR0_EL1.
    enable_mmu();
}

/// Setup Memory Attribute Indirection Register
fn setup_mair() {
    // Attr0 = 0x00: Device-nGnRnE (strongly ordered, no caching)
    // Attr1 = 0x44: Normal, Inner/Outer Non-Cacheable
    // Attr2 = 0xFF: Normal, Inner/Outer Write-Back, Read/Write Allocate
    let mair: u64 = 0xFF_44_00;
    unsafe {
        asm!("msr mair_el1, {}", in(reg) mair);
    }
}

/// Setup Translation Control Register
fn setup_tcr() {
    let tcr: u64 = 
        (16 << 0) |    // T0SZ: 48-bit VA for TTBR0
        (16 << 16) |   // T1SZ: 48-bit VA for TTBR1
        (0b00 << 14) | // TG0: 4KB granule for TTBR0
        (0b10 << 30) | // TG1: 4KB granule for TTBR1
        (0b01 << 8) |  // IRGN0: Inner Write-back
        (0b01 << 10) | // ORGN0: Outer Write-back
        (0b11 << 12);  // SH0: Inner shareable
    
    unsafe {
        asm!("msr tcr_el1, {}", in(reg) tcr);
    }
}

/// Enable MMU
fn enable_mmu() {
    unsafe {
        asm!(
            "mrs x0, sctlr_el1",
            "orr x0, x0, #1",      // Enable MMU
            "orr x0, x0, #(1<<2)", // Enable data cache
            "orr x0, x0, #(1<<12)", // Enable instruction cache
            "msr sctlr_el1, x0",
            "isb",
            out("x0") _,
        );
    }
}

/// Invalidate TLB
pub fn invalidate_tlb() {
    unsafe {
        asm!("tlbi vmalle1is");
        asm!("dsb ish");
        asm!("isb");
    }
}
