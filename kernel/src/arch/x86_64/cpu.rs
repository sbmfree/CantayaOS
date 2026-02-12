//! CPU management for x86_64

use core::arch::asm;

/// Initialize CPU features
pub fn init() {
    // Enable SSE/SSE2 (required for x86_64)
    unsafe {
        let mut cr0: u64;
        asm!("mov {}, cr0", out(reg) cr0);
        cr0 &= !(1 << 2); // Clear CR0.EM (Emulation)
        cr0 |= 1 << 1;    // Set CR0.MP (Monitor Coprocessor)
        asm!("mov cr0, {}", in(reg) cr0);

        let mut cr4: u64;
        asm!("mov {}, cr4", out(reg) cr4);
        cr4 |= 1 << 9;    // Set CR4.OSFXSR (OS support for FXSAVE/FXRSTOR)
        cr4 |= 1 << 10;   // Set CR4.OSXMMEXCPT (OS support for unmasked SIMD exceptions)
        asm!("mov cr4, {}", in(reg) cr4);
    }
}

/// Halt the CPU until next interrupt
#[inline(always)]
pub fn halt() {
    unsafe {
        asm!("hlt");
    }
}

/// Disable interrupts
#[inline(always)]
pub fn disable_interrupts() {
    unsafe {
        asm!("cli");
    }
}

/// Enable interrupts
#[inline(always)]
pub fn enable_interrupts() {
    unsafe {
        asm!("sti");
    }
}

/// Save current RFLAGS state and disable interrupts.
/// Returns the old RFLAGS value for later restore.
#[inline(always)]
pub fn save_and_disable_interrupts() -> u64 {
    let rflags: u64;
    unsafe {
        asm!("pushfq; pop {}", out(reg) rflags);
        asm!("cli");
    }
    rflags
}

/// Restore RFLAGS state saved by `save_and_disable_interrupts`.
#[inline(always)]
pub fn restore_interrupts(rflags: u64) {
    unsafe {
        asm!("push {}; popfq", in(reg) rflags);
    }
}

/// Get current privilege level (CPL from CS segment selector)
pub fn current_el() -> u8 {
    let cs: u16;
    unsafe {
        asm!("mov {:x}, cs", out(reg) cs);
    }
    (cs & 0x3) as u8
}

/// Memory barrier - data synchronization (mfence)
#[inline(always)]
pub fn dsb() {
    unsafe {
        asm!("mfence");
    }
}

/// Memory barrier - instruction synchronization (serialize with cpuid)
#[inline(always)]
pub fn isb() {
    unsafe {
        // CPUID serializes execution
        // We need to save/restore ebx since it's reserved by LLVM
        asm!(
            "push rbx",
            "xor eax, eax",
            "cpuid",
            "pop rbx",
            out("eax") _,
            out("ecx") _,
            out("edx") _,
        );
    }
}
