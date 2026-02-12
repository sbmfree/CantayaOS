//! CPU management for AArch64

use core::arch::asm;

/// Initialize CPU features
pub fn init() {
    // Enable FPU/SIMD
    unsafe {
        asm!(
            "mrs x0, cpacr_el1",
            "orr x0, x0, #(3 << 20)",
            "msr cpacr_el1, x0",
            "isb",
            out("x0") _,
        );
    }
}

/// Halt the CPU until next interrupt
#[inline(always)]
pub fn halt() {
    unsafe {
        asm!("wfi");
    }
}

/// Disable interrupts
#[inline(always)]
pub fn disable_interrupts() {
    unsafe {
        asm!("msr daifset, #0xf");
    }
}

/// Enable interrupts
#[inline(always)]
pub fn enable_interrupts() {
    unsafe {
        asm!("msr daifclr, #0xf");
    }
}

/// Save current DAIF state and disable all interrupts.
/// Returns the old DAIF value for later restore.
#[inline(always)]
pub fn save_and_disable_interrupts() -> u64 {
    let daif: u64;
    unsafe {
        asm!("mrs {}, daif", out(reg) daif);
        asm!("msr daifset, #0xf");
    }
    daif
}

/// Restore DAIF state saved by `save_and_disable_interrupts`.
#[inline(always)]
pub fn restore_interrupts(daif: u64) {
    unsafe {
        asm!("msr daif, {}", in(reg) daif);
    }
}

/// Get current exception level
pub fn current_el() -> u8 {
    let el: u64;
    unsafe {
        asm!("mrs {}, CurrentEL", out(reg) el);
    }
    ((el >> 2) & 0x3) as u8
}

/// Memory barrier - data synchronization
#[inline(always)]
pub fn dsb() {
    unsafe {
        asm!("dsb sy");
    }
}

/// Memory barrier - instruction synchronization
#[inline(always)]
pub fn isb() {
    unsafe {
        asm!("isb");
    }
}
