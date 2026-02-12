//! Boot code for AArch64

use core::arch::naked_asm;

/// Stack size per CPU core
const STACK_SIZE: usize = 4096 * 16; // 64KB

/// Boot stack
#[link_section = ".bss.stack"]
static mut BOOT_STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

/// Entry point from bootloader (must be position independent)
#[unsafe(naked)]
#[no_mangle]
#[link_section = ".text.boot"]
pub unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        // Check processor ID, hang if not primary core
        "mrs x0, mpidr_el1",
        "and x0, x0, #0xFF",
        "cbnz x0, .hang",
        
        // Setup stack
        "adrp x0, {stack}",
        "add x0, x0, :lo12:{stack}",
        "add x0, x0, {stack_size}",
        "mov sp, x0",
        
        // Clear BSS
        "adrp x0, __bss_start",
        "adrp x1, __bss_end",
        ".clear_bss:",
        "cmp x0, x1",
        "b.ge .bss_done",
        "str xzr, [x0], #8",
        "b .clear_bss",
        ".bss_done:",
        
        // Jump to Rust kernel
        "bl kernel_main",
        
        ".hang:",
        "wfi",
        "b .hang",
        
        stack = sym BOOT_STACK,
        stack_size = const STACK_SIZE,
    );
}
