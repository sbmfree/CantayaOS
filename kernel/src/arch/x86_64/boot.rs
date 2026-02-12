//! Boot code for x86_64

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
        // Setup stack
        "lea rsp, [{stack} + {stack_size}]",
        
        // Clear BSS
        "lea rdi, [rip + __bss_start]",
        "lea rcx, [rip + __bss_end]",
        "sub rcx, rdi",
        "shr rcx, 3",  // Divide by 8 for qword count
        "xor rax, rax",
        "rep stosq",
        
        // Jump to Rust kernel
        "call kernel_main",
        
        ".hang:",
        "hlt",
        "jmp .hang",
        
        stack = sym BOOT_STACK,
        stack_size = const STACK_SIZE,
    );
}
