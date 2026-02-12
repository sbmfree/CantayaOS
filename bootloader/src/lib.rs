//! CantayaOS Bootloader
//!
//! Minimal bootloader for AArch64 that:
//! - Initialises PL011 UART for early debug output
//! - Detects the current Exception Level
//! - Drops from EL2 → EL1 if needed
//! - Prints hardware info (DTB pointer, RAM)
//! - Jumps to the kernel entry point
//!
//! Note: This bootloader is only for ARM64. x86_64 uses direct kernel loading.

#![no_std]
#![no_main]

#[cfg(target_arch = "aarch64")]
use core::panic::PanicInfo;
#[cfg(target_arch = "aarch64")]
use core::arch::asm;

// x86_64 stub - not used
#[cfg(target_arch = "x86_64")]
pub fn dummy() {}

#[cfg(target_arch = "aarch64")]
/// Kernel load address (must match linker.ld ORIGIN)
const KERNEL_LOAD_ADDR: usize = 0x40080000;

#[cfg(target_arch = "aarch64")]
/// Bootloader entry point
/// On QEMU virt, x0 = DTB address on entry
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Save DTB pointer passed by QEMU (in x0)
    let dtb_ptr: u64;
    unsafe { asm!("mov {}, x0", out(reg) dtb_ptr) };

    // Initialize UART for debug output
    init_uart();

    print_str("\r\n");
    print_str("========================================\r\n");
    print_str("  CantayaOS Bootloader v0.1.0\r\n");
    print_str("  Architecture: AArch64 (ARM64)\r\n");
    print_str("========================================\r\n");

    // Show current exception level
    let el = current_el();
    print_str("[BOOT] Current EL: ");
    print_dec(el as u64);
    print_str("\r\n");

    // Print DTB address
    print_str("[BOOT] DTB address: 0x");
    print_hex(dtb_ptr);
    print_str("\r\n");

    // Print RAM info (QEMU virt: 128 MB starting at 0x4000_0000)
    print_str("[BOOT] RAM: 0x40000000 - 0x48000000 (128 MB)\r\n");
    print_str("[BOOT] Kernel at: 0x");
    print_hex(KERNEL_LOAD_ADDR as u64);
    print_str("\r\n");

    // Drop to EL1 if we are at EL2
    if el == 2 {
        print_str("[BOOT] Dropping from EL2 to EL1...\r\n");
        drop_to_el1();
        print_str("[BOOT] Now at EL1\r\n");
    }

    // Jump to kernel
    print_str("[BOOT] Jumping to kernel entry...\r\n");
    print_str("========================================\r\n\r\n");

    let kernel_entry: extern "C" fn() -> ! = unsafe {
        core::mem::transmute(KERNEL_LOAD_ADDR)
    };
    kernel_entry();
}

#[cfg(target_arch = "aarch64")]
/// Get current exception level
fn current_el() -> u8 {
    let el: u64;
    unsafe {
        asm!("mrs {}, CurrentEL", out(reg) el);
    }
    ((el >> 2) & 0x3) as u8
}

#[cfg(target_arch = "aarch64")]
/// Drop from EL2 to EL1
fn drop_to_el1() {
    unsafe {
        asm!(
            // Disable EL2 MMU
            "mov x0, #0",
            "msr sctlr_el1, x0",

            // Enable AArch64 in EL1
            "mov x0, #(1 << 31)",   // RW=1 (AArch64)
            "orr x0, x0, #(1 << 1)", // SWIO hardwired
            "msr hcr_el2, x0",

            // Don't trap FP/SIMD to EL2
            "mov x0, #0x33ff",
            "msr cptr_el2, x0",

            // Setup SPSR for EL1h with interrupts masked
            "mov x0, #0x3c5",       // D,A,I,F masked, EL1h
            "msr spsr_el2, x0",

            // Set EL1 entry point
            "adr x0, 1f",
            "msr elr_el2, x0",

            // Return to EL1
            "eret",
            "1:",
            out("x0") _,
        );
    }
}

// -------------------------------------------------------------------------
// PL011 UART driver (QEMU virt machine)
// -------------------------------------------------------------------------

#[cfg(target_arch = "aarch64")]
const UART_BASE: usize = 0x0900_0000;
#[cfg(target_arch = "aarch64")]
const UART_DR: *mut u8 = UART_BASE as *mut u8;
#[cfg(target_arch = "aarch64")]
const UART_FR: *const u32 = (UART_BASE + 0x18) as *const u32;

#[cfg(target_arch = "aarch64")]
fn init_uart() {
    // UART is pre-initialized by QEMU for the virt machine
    // Just make sure the FIFO is drained
}

#[cfg(target_arch = "aarch64")]
fn print_char(c: u8) {
    unsafe {
        // Wait until TX FIFO is not full (bit 5 of FR)
        while (UART_FR.read_volatile() & (1 << 5)) != 0 {}
        UART_DR.write_volatile(c);
    }
}

#[cfg(target_arch = "aarch64")]
fn print_str(s: &str) {
    for c in s.bytes() {
        print_char(c);
    }
}

#[cfg(target_arch = "aarch64")]
/// Print a u64 in decimal
fn print_dec(mut n: u64) {
    if n == 0 {
        print_char(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        print_char(buf[i]);
    }
}

#[cfg(target_arch = "aarch64")]
/// Print a u64 in hexadecimal
fn print_hex(n: u64) {
    let hex = b"0123456789abcdef";
    // Print 16 hex digits (64-bit)
    let mut started = false;
    for shift in (0..16).rev() {
        let digit = ((n >> (shift * 4)) & 0xF) as usize;
        if digit != 0 || started || shift == 0 {
            print_char(hex[digit]);
            started = true;
        }
    }
}

/// Panic handler
#[cfg(target_arch = "aarch64")]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    print_str("\r\n!!! BOOTLOADER PANIC !!!\r\n");
    // Can't easily print PanicInfo without fmt, just halt
    loop {
        unsafe { asm!("wfi"); }
    }
}

// x86_64 stub - provide a panic handler for when this crate is included
#[cfg(target_arch = "x86_64")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
