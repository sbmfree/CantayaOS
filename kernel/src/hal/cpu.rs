// CPU Entry Point and Low-Level CPU Operations
//
// This module contains the naked _start function — the very first code that
// executes when the bootloader jumps to the kernel.
//
// It also provides wrappers for privileged CPU instructions that the rest
// of the kernel uses through safe Rust interfaces.

/// Kernel entry point — called directly by the bootloader.
///
/// This is a naked function because we need full control over the stack frame.
/// The bootloader passes the BootInfo pointer in RDI (System V x86_64 ABI).
///
/// We do minimal setup here:
///   1. Set up a proper kernel stack (defined in linker.ld)
///   2. Call kernel_main with the BootInfo pointer
///
/// SAFETY: This function is called exactly once by the bootloader.
/// The stack pointer set by the bootloader is temporary.

// Linker-script-defined symbol at the top of the 64 KiB bootstrap stack
extern "C" {
    static __kernel_stack_top: u8;
}

#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        // RDI already contains the BootInfo pointer (from bootloader)
        // Set up the kernel stack (__kernel_stack_top defined in linker.ld)
        "lea rsp, [{stack_top}]",
        // Align stack to 16 bytes (required by System V ABI)
        "and rsp, ~0xF",
        // Call the Rust kernel_main function
        "call {kernel_main}",
        // If kernel_main somehow returns, halt
        "2:",
        "cli",
        "hlt",
        "jmp 2b",
        stack_top = sym __kernel_stack_top,
        kernel_main = sym crate::kernel_main,
    );
}

/// Read the CR2 register (contains the faulting address on a page fault)
#[inline]
pub fn read_cr2() -> u64 {
    let value: u64;
    unsafe {
        core::arch::asm!("mov {}, cr2", out(reg) value);
    }
    value
}

/// Read the CR3 register (contains the physical address of the PML4 page table)
#[inline]
pub fn read_cr3() -> u64 {
    let value: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) value);
    }
    value
}

/// Write to the CR3 register (switch page tables)
///
/// SAFETY: The new PML4 must be valid and identity-map the currently executing code.
#[inline]
pub unsafe fn write_cr3(value: u64) {
    core::arch::asm!("mov cr3, {}", in(reg) value);
}

/// Invalidate a TLB entry for the given virtual address
#[inline]
pub fn invlpg(addr: u64) {
    unsafe {
        core::arch::asm!("invlpg [{}]", in(reg) addr);
    }
}

/// Read the RFLAGS register
#[inline]
pub fn read_rflags() -> u64 {
    let flags: u64;
    unsafe {
        core::arch::asm!("pushfq; pop {}", out(reg) flags);
    }
    flags
}

/// Check if interrupts are currently enabled
#[inline]
pub fn interrupts_enabled() -> bool {
    read_rflags() & (1 << 9) != 0
}

/// Read CR0 register
#[inline]
pub fn read_cr0() -> u64 {
    let value: u64;
    unsafe { core::arch::asm!("mov {}, cr0", out(reg) value); }
    value
}

/// Write CR0 register
#[inline]
pub unsafe fn write_cr0(value: u64) {
    core::arch::asm!("mov cr0, {}", in(reg) value);
}

/// Read CR4 register
#[inline]
pub fn read_cr4() -> u64 {
    let value: u64;
    unsafe { core::arch::asm!("mov {}, cr4", out(reg) value); }
    value
}

/// Write CR4 register
#[inline]
pub unsafe fn write_cr4(value: u64) {
    core::arch::asm!("mov cr4, {}", in(reg) value);
}

/// Enable SSE/SSE2 support in the CPU.
///
/// Required for FXSAVE/FXRSTOR and floating-point operations.
/// Sets CR0.MP, clears CR0.EM, sets CR4.OSFXSR and CR4.OSXMMEXCPT.
pub fn enable_sse() {
    unsafe {
        let mut cr0 = read_cr0();
        cr0 &= !(1 << 2);  // Clear CR0.EM (Emulation) — must be clear for SSE
        cr0 |= 1 << 1;      // Set CR0.MP (Monitor Coprocessor)
        write_cr0(cr0);

        let mut cr4 = read_cr4();
        cr4 |= 1 << 9;      // Set CR4.OSFXSR — OS supports FXSAVE/FXRSTOR
        cr4 |= 1 << 10;     // Set CR4.OSXMMEXCPT — OS handles SIMD exceptions
        write_cr4(cr4);
    }
    log::info!("SSE/SSE2 enabled (CR0.EM cleared, CR4.OSFXSR set)");
}

/// Read a Model Specific Register (MSR)
#[inline]
pub unsafe fn rdmsr(msr: u32) -> u64 {
    let (low, high): (u32, u32);
    core::arch::asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") low,
        out("edx") high,
    );
    (low as u64) | ((high as u64) << 32)
}

/// Write a Model Specific Register (MSR)
#[inline]
pub unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") low,
        in("edx") high,
    );
}
