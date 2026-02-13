// Port I/O — Direct Hardware Access
//
// x86 hardware communicates through two mechanisms:
//   1. Memory-Mapped I/O (MMIO) — device registers at specific physical addresses
//   2. Port I/O — A separate 16-bit address space accessed via IN/OUT instructions
//
// Legacy devices (PIC, PIT, PS/2, serial) use port I/O.
// Modern devices (PCIe, USB, NVMe) use MMIO.
//
// These are the building blocks for all hardware drivers.
// The HAL exposes these as unsafe functions since incorrect I/O can crash hardware.

/// Read a byte from an I/O port.
///
/// SAFETY: Reading from the wrong port can have side effects on hardware.
/// The caller must ensure the port number is valid for the intended device.
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!(
        "in al, dx",
        out("al") value,
        in("dx") port,
        options(nomem, nostack)
    );
    value
}

/// Write a byte to an I/O port.
///
/// SAFETY: Writing to the wrong port can crash hardware or corrupt data.
/// The caller must ensure the port number and value are correct.
#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!(
        "out dx, al",
        in("al") value,
        in("dx") port,
        options(nomem, nostack)
    );
}

/// Read a 16-bit word from an I/O port.
#[inline]
pub unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    core::arch::asm!(
        "in ax, dx",
        out("ax") value,
        in("dx") port,
        options(nomem, nostack)
    );
    value
}

/// Write a 16-bit word to an I/O port.
#[inline]
pub unsafe fn outw(port: u16, value: u16) {
    core::arch::asm!(
        "out dx, ax",
        in("ax") value,
        in("dx") port,
        options(nomem, nostack)
    );
}

/// Read a 32-bit dword from an I/O port.
#[inline]
pub unsafe fn ind(port: u16) -> u32 {
    let value: u32;
    core::arch::asm!(
        "in eax, dx",
        out("eax") value,
        in("dx") port,
        options(nomem, nostack)
    );
    value
}

/// Write a 32-bit dword to an I/O port.
#[inline]
pub unsafe fn outd(port: u16, value: u32) {
    core::arch::asm!(
        "out dx, eax",
        in("eax") value,
        in("dx") port,
        options(nomem, nostack)
    );
}
