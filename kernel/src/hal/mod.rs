// Hardware Abstraction Layer (HAL)
//
// The HAL is the lowest layer of the kernel, providing an abstraction over
// the physical hardware. All hardware-specific code lives here, so the rest
// of the kernel can be (theoretically) portable to other architectures.
//
// In Windows NT, the HAL (hal.dll) abstracts:
//   - Interrupt controllers (PIC/APIC)
//   - Timers (PIT/HPET/APIC timer)
//   - DMA controllers
//   - CPU-specific features
//
// Our HAL provides:
//   - GDT (Global Descriptor Table) — CPU segment configuration
//   - IDT (Interrupt Descriptor Table) — interrupt/exception handlers
//   - Serial port I/O — debug output
//   - Port I/O — raw hardware port access
//   - CPU entry point — the _start function

pub mod cpu;
pub mod gdt;
pub mod idt;
pub mod interrupts;
pub mod keyboard;
pub mod pit;
pub mod serial;
pub mod port;
pub mod mouse;
pub mod rtc;
pub mod acpi;
pub mod pci;
pub mod speaker;
pub mod virtio;
pub mod virtio_blk;

/// Initialize all HAL subsystems.
///
/// This must be called before any other kernel subsystem.
/// Order matters: GDT before IDT, IDT before enabling interrupts.
pub fn init() {
    // 1. Initialize the GDT (defines kernel/user code and data segments, TSS)
    gdt::init();

    // 2. Initialize the IDT (registers exception and interrupt handlers)
    idt::init();

    // 3. Initialize serial port for debug output
    serial::init();
}
