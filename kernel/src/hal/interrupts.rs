// Interrupt Management
//
// This module provides high-level control over hardware interrupts,
// including PIC (Programmable Interrupt Controller) initialization.
//
// Modern systems use the APIC (Advanced PIC), but we start with the legacy
// 8259 PIC for simplicity. The PIC is two cascaded chips (master + slave)
// that multiplex 15 hardware interrupt lines into CPU interrupt vectors.
//
// PIC Remapping:
//   By default, the PIC maps IRQ 0-7 to vectors 8-15, which conflicts with
//   CPU exceptions. We remap them to vectors 32-47 to avoid conflicts.

use super::port::{inb, outb};

/// Master PIC I/O ports
const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;

/// Slave PIC I/O ports  
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

/// ICW1: Initialization Command Word 1 — start initialization sequence
const ICW1_INIT: u8 = 0x10;
const ICW1_ICW4: u8 = 0x01; // ICW4 needed

/// ICW4: 8086 mode
const ICW4_8086: u8 = 0x01;

/// Vector offsets for remapped PIC interrupts
const PIC1_OFFSET: u8 = 32; // IRQ 0-7 → vectors 32-39
const PIC2_OFFSET: u8 = 40; // IRQ 8-15 → vectors 40-47

/// Initialize the 8259 PIC with remapped vectors.
///
/// This sends the initialization command words (ICW1-ICW4) to both PICs.
/// After this, IRQ0 = vector 32, IRQ1 = vector 33, etc.
fn init_pic() {
    unsafe {
        // Save current masks
        let _mask1 = inb(PIC1_DATA);
        let _mask2 = inb(PIC2_DATA);

        // ICW1: Start initialization (cascade mode, ICW4 needed)
        outb(PIC1_COMMAND, ICW1_INIT | ICW1_ICW4);
        io_wait();
        outb(PIC2_COMMAND, ICW1_INIT | ICW1_ICW4);
        io_wait();

        // ICW2: Vector offset (remap IRQs away from CPU exceptions)
        outb(PIC1_DATA, PIC1_OFFSET);
        io_wait();
        outb(PIC2_DATA, PIC2_OFFSET);
        io_wait();

        // ICW3: Cascading — master has slave on IRQ2, slave has cascade identity 2
        outb(PIC1_DATA, 4); // Bit 2 = IRQ2 has slave
        io_wait();
        outb(PIC2_DATA, 2); // Cascade identity = 2
        io_wait();

        // ICW4: 8086 mode
        outb(PIC1_DATA, ICW4_8086);
        io_wait();
        outb(PIC2_DATA, ICW4_8086);
        io_wait();

        // Mask all interrupts except IRQ0 (timer), IRQ1 (keyboard), and IRQ2 (cascade)
        // Bit = 1 means masked (disabled)
        // IRQ2 must be unmasked on master for slave PIC interrupts to reach the CPU
        outb(PIC1_DATA, 0b1111_1000); // Enable IRQ0 (timer), IRQ1 (keyboard), IRQ2 (cascade)
        outb(PIC2_DATA, 0b1110_1111); // Enable IRQ12 (mouse) = slave IRQ4
    }
}

/// Small I/O delay — some old hardware needs a brief pause between PIC commands
#[inline]
fn io_wait() {
    unsafe {
        // Writing to port 0x80 (unused POST diagnostic port) creates a ~1μs delay
        outb(0x80, 0);
    }
}

/// Whether the PIC has been initialized (avoid re-init on every `enable()` call)
static mut PIC_INITIALIZED: bool = false;

/// Enable hardware interrupts (set IF flag in RFLAGS)
pub fn enable() {
    unsafe {
        if !PIC_INITIALIZED {
            init_pic();
            PIC_INITIALIZED = true;
        }
        core::arch::asm!("sti");
    }
}

/// Disable hardware interrupts (clear IF flag)
pub fn disable() {
    unsafe {
        core::arch::asm!("cli");
    }
}

/// Execute a closure with interrupts disabled, then restore the previous state.
///
/// This is the safe way to create critical sections in the kernel.
/// It handles nested disable/enable correctly by checking the previous IF state.
pub fn without_interrupts<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let was_enabled = super::cpu::interrupts_enabled();

    if was_enabled {
        disable();
    }

    let result = f();

    if was_enabled {
        enable();
    }

    result
}
