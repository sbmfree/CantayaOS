// CantayaOS Kernel — Entry Point and Root Module
//
// This is the bare-metal kernel for CantayaOS, a Windows-inspired hybrid kernel
// targeting x86_64. It uses no standard library (#![no_std]) and provides its own
// panic handler, memory allocator, and runtime.
//
// Architecture Overview (modeled after Windows NT):
//
//   ┌─────────────────────────────────────────────────┐
//   │                User Mode (Ring 3)                │
//   │   ┌──────────┐  ┌──────────┐  ┌──────────────┐  │
//   │   │ Process A │  │ Process B │  │ (Future GUI) │  │
//   │   └────┬─────┘  └────┬─────┘  └──────┬───────┘  │
//   ├────────┼──────────────┼───────────────┼──────────┤
//   │        │    System Call Interface      │          │
//   │   ┌────┴──────────────┴───────────────┴───────┐  │
//   │   │              Executive Layer              │  │
//   │   │  ┌──────────┐ ┌────────┐ ┌────────────┐   │  │
//   │   │  │ Process   │ │ I/O    │ │ Object     │   │  │
//   │   │  │ Manager   │ │ Manager│ │ Manager    │   │  │
//   │   │  └──────────┘ └────────┘ └────────────┘   │  │
//   │   └───────────────────┬───────────────────────┘  │
//   │   ┌───────────────────┴───────────────────────┐  │
//   │   │              Kernel Core                  │  │
//   │   │  ┌──────────┐ ┌──────────┐ ┌───────────┐  │  │
//   │   │  │ Memory   │ │Scheduler │ │ Sync      │  │  │
//   │   │  │ Manager  │ │          │ │ Primitives│  │  │
//   │   │  └──────────┘ └──────────┘ └───────────┘  │  │
//   │   └───────────────────┬───────────────────────┘  │
//   │   ┌───────────────────┴───────────────────────┐  │
//   │   │       Hardware Abstraction Layer (HAL)     │  │
//   │   │  ┌─────┐ ┌──────┐ ┌──────┐ ┌───────────┐  │  │
//   │   │  │ GDT │ │ IDT  │ │ APIC │ │ Serial/IO │  │  │
//   │   │  └─────┘ └──────┘ └──────┘ └───────────┘  │  │
//   │   └───────────────────────────────────────────┘  │
//   │                Kernel Mode (Ring 0)              │
//   └─────────────────────────────────────────────────┘
//
// Boot Sequence:
//   1. Bootloader loads us and jumps to _start (in hal/cpu.rs)
//   2. _start calls kernel_main with the BootInfo pointer
//   3. We initialize HAL (GDT, IDT, serial)
//   4. We initialize memory management (frame allocator, heap)
//   5. We initialize the scheduler
//   6. We initialize the framebuffer console
//   7. We enter the idle loop

#![no_std]
#![no_main]
// Required unstable features for bare-metal kernel development
#![feature(abi_x86_interrupt)] // Required for interrupt handler function signatures
#![feature(alloc_error_handler)] // Custom handler for allocation failures

extern crate alloc;

// Kernel subsystems, ordered by initialization dependency
pub mod hal;       // Hardware Abstraction Layer — must initialize first
pub mod memory;    // Memory management — depends on HAL
pub mod core_kernel; // Kernel core services — depends on memory
pub mod executive; // Executive services — depends on everything below
pub mod graphics;  // Framebuffer graphics — depends on memory
pub mod desktop;   // Graphical desktop environment — depends on graphics
pub mod logging;   // Kernel logging infrastructure
pub mod shell;     // Interactive kernel shell

use cantaya_shared::boot_info::BootInfo;
use core::panic::PanicInfo;

/// Kernel main entry point — called by the assembly stub in hal/cpu.rs
///
/// At this point:
///   - We're running in the higher half (virtual address 0xFFFFFFFF80000000+)
///   - We have a valid stack (set up by the bootloader, will be replaced)
///   - Interrupts are disabled
///   - boot_info points to the BootInfo structure from the bootloader
///
/// We initialize subsystems in dependency order, then enter the main loop.
pub extern "C" fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // Verify boot info integrity
    assert!(boot_info.is_valid(), "Invalid BootInfo magic — boot protocol mismatch!");

    // Phase 1: Hardware Abstraction Layer
    // Initialize CPU structures (GDT, IDT), serial port for debug output
    hal::init();
    logging::init();
    log::info!("CantayaOS Kernel v{}", env!("CARGO_PKG_VERSION"));

    // Enable SSE/FPU for floating-point and SIMD support
    hal::cpu::enable_sse();
    log::info!("SSE/FPU enabled");

    log::info!("Boot info valid, framebuffer {}x{}", 
        boot_info.framebuffer.width, boot_info.framebuffer.height);

    // Phase 2: Memory Management
    // Set up the physical frame allocator using the UEFI memory map,
    // then initialize the kernel heap allocator on top of it.
    memory::init(boot_info);
    log::info!("Memory management initialized");
    log::info!("Total usable memory: {} MiB", 
        boot_info.memory_map.total_usable_memory() / (1024 * 1024));

    // Phase 2.5: ACPI — parse system tables for hardware discovery
    if boot_info.rsdp_address != 0 {
        hal::acpi::init(boot_info.rsdp_address);
        log::info!("ACPI initialized (RSDP at {:#X})", boot_info.rsdp_address);
    } else {
        log::warn!("RSDP not provided by bootloader — ACPI unavailable");
    }

    // Phase 2.6: PCI — enumerate devices
    hal::pci::enumerate();
    log::info!("PCI bus enumerated");

    // Phase 3: Initialize the scheduler (before enabling interrupts to avoid lock contention)
    core_kernel::scheduler::init();
    log::info!("Scheduler initialized");

    // Phase 3.5: Initialize SYSCALL/SYSRET MSRs
    executive::syscall::init();
    log::info!("SYSCALL MSRs programmed");

    // Phase 3.6: Initialize PS/2 mouse BEFORE enabling interrupts.
    // Mouse init uses polling (reads ACK bytes from port 0x60 directly).
    // If we init after IRQ12 is unmasked, the IRQ handler steals the ACK
    // bytes before the polling code can read them, causing init to fail.
    hal::mouse::init();
    log::info!("PS/2 mouse initialized");

    // Phase 4: Enable interrupts and PIT timer
    hal::interrupts::enable();
    hal::pit::init(1000); // 1000 Hz = 1 ms per tick
    log::info!("Interrupts enabled, PIT at 1000 Hz");

    // Phase 5: Initialize the framebuffer console for visual output
    graphics::init(&boot_info.framebuffer);

    // Phase 6: Launch the interactive kernel shell
    log::info!("All subsystems initialized — launching shell");
    shell::run()
}

/// The kernel idle loop.
///
/// This is the equivalent of Windows' "System Idle Process" (PID 0).
/// It runs when no other thread is ready to execute.
/// The HLT instruction puts the CPU into a low-power state until the next interrupt.
fn idle_loop() -> ! {
    loop {
        // HLT: sleep until the next interrupt (timer, keyboard, etc.)
        // This saves power and reduces heat generation.
        x86_64_hlt();
    }
}

/// Execute the HLT instruction to wait for the next interrupt.
#[inline(always)]
fn x86_64_hlt() {
    unsafe {
        core::arch::asm!("hlt");
    }
}

/// Panic handler — Kernel Blue Screen of Death (BSOD)
///
/// Draws a graphical blue screen with the panic message and system info,
/// similar to the classic Windows BSOD. Uses direct framebuffer writes
/// to bypass all locks and guarantees display even when the system is broken.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Always log to serial (most reliable output)
    log::error!("KERNEL PANIC: {}", info);

    // Try to draw a BSOD on the framebuffer
    // We use try_lock to avoid deadlock if the FB is locked
    if let Some(fb) = graphics::framebuffer::FRAMEBUFFER.try_lock() {
        use graphics::framebuffer::Color;
        use graphics::font::{self, CHAR_WIDTH, CHAR_HEIGHT};

        let bsod_bg = Color::BSOD_BLUE;
        let white = Color::WHITE;
        let yellow = Color::YELLOW;

        // Clear screen to BSOD blue (direct write)
        fb.clear_direct(bsod_bg);

        // Helper to draw a string at a character grid position (direct)
        let draw_str = |col: u32, row: u32, s: &str, color: Color| {
            for (i, c) in s.chars().enumerate() {
                let x = (col + i as u32) * CHAR_WIDTH;
                let y = row * CHAR_HEIGHT;
                let bitmap = font::get_char_bitmap(c);
                for (dy, &row_bits) in bitmap.iter().enumerate() {
                    for dx in 0..8u32 {
                        let pixel_set = (row_bits >> (7 - dx)) & 1 != 0;
                        if pixel_set {
                            fb.put_pixel_direct(x + dx, y + dy as u32, color);
                        }
                    }
                }
            }
        };

        // Draw the BSOD content
        draw_str(2, 2, "*** KERNEL PANIC ***", yellow);
        draw_str(2, 4, "CantayaOS has encountered a fatal error and must stop.", white);

        // Format panic location
        if let Some(location) = info.location() {
            let mut buf = [0u8; 128];
            let len = fmt_to_buf(&mut buf, format_args!(
                "File: {}  Line: {}  Column: {}",
                location.file(), location.line(), location.column()
            ));
            if let Ok(s) = core::str::from_utf8(&buf[..len]) {
                draw_str(2, 6, s, white);
            }
        }

        // Format panic message (truncated to fit screen)
        if let Some(msg) = info.message().as_str() {
            // Simple string message
            let display = if msg.len() > 100 { &msg[..100] } else { msg };
            draw_str(2, 8, display, white);
        } else {
            // Formatted message - use our buffer formatter
            let mut buf = [0u8; 200];
            let len = fmt_to_buf(&mut buf, format_args!("{}", info.message()));
            if let Ok(s) = core::str::from_utf8(&buf[..len]) {
                let display = if s.len() > 100 { &s[..100] } else { s };
                draw_str(2, 8, display, white);
            }
        }

        // System info
        let ticks = shell::ticks();
        let ms = hal::pit::ticks_to_ms(ticks);
        let seconds = ms / 1000;
        let mut buf = [0u8; 80];
        let len = fmt_to_buf(&mut buf, format_args!("Uptime: {}s  Ticks: {}", seconds, ticks));
        if let Ok(s) = core::str::from_utf8(&buf[..len]) {
            draw_str(2, 10, s, white);
        }

        draw_str(2, 12, "The system has been halted to prevent damage.", white);
        draw_str(2, 13, "Please restart your computer.", white);

        // Register dump
        let cr3 = hal::cpu::read_cr3();
        let rflags: u64;
        unsafe { core::arch::asm!("pushfq; pop {}", out(reg) rflags); }
        let len = fmt_to_buf(&mut buf, format_args!("CR3={:#X}  RFLAGS={:#X}", cr3, rflags));
        if let Ok(s) = core::str::from_utf8(&buf[..len]) {
            draw_str(2, 15, s, white);
        }

        // Sad face
        draw_str(2, 17, ":(", yellow);
    } else {
        // Couldn't acquire framebuffer — fallback to text console
        graphics::console::println("*** KERNEL PANIC ***");
        graphics::console::println("See serial output for details");
    }

    // Halt all CPUs
    loop {
        unsafe {
            core::arch::asm!("cli; hlt");
        }
    }
}

/// Format into a fixed-size buffer without heap allocation.
/// Returns the number of bytes written.
fn fmt_to_buf(buf: &mut [u8], args: core::fmt::Arguments) -> usize {
    use core::fmt::Write;
    struct BufWriter<'a> {
        buf: &'a mut [u8],
        pos: usize,
    }
    impl<'a> Write for BufWriter<'a> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let bytes = s.as_bytes();
            let remaining = self.buf.len() - self.pos;
            let len = bytes.len().min(remaining);
            self.buf[self.pos..self.pos + len].copy_from_slice(&bytes[..len]);
            self.pos += len;
            Ok(())
        }
    }
    let mut writer = BufWriter { buf, pos: 0 };
    let _ = writer.write_fmt(args);
    writer.pos
}

/// Allocation error handler — called when heap allocation fails.
///
/// In a real OS, we might try to free cached memory or kill a process.
/// For now, we panic.
#[alloc_error_handler]
fn alloc_error(layout: core::alloc::Layout) -> ! {
    panic!(
        "Kernel heap allocation failed: size={}, align={}",
        layout.size(),
        layout.align()
    );
}
