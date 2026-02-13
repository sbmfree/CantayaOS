# CantayaOS

A modern 64-bit Windows-inspired operating system written in Rust, featuring a
graphical desktop environment, PS/2 mouse and keyboard drivers, preemptive
multitasking, and a full interactive shell — all running bare-metal on x86_64.

## Screenshots

Type `desktop` in the shell to enter the graphical desktop environment with a
mouse cursor, clickable icons, window manager, taskbar, and start menu.

## Architecture

CantayaOS is a hybrid kernel inspired by Windows NT, built entirely in Rust
for the x86_64 architecture with UEFI boot.

```
┌─────────────────────────────────────────────────────┐
│                 User Mode (Ring 3)                   │
│       [Processes]  [GUI]  [Shell]  (future)          │
├──────────────────────────────────────────────────────┤
│              System Call Interface                    │
├──────────────────────────────────────────────────────┤
│  Executive Layer                                     │
│    ├── Process Manager    (process/thread lifecycle)  │
│    ├── Object Manager     (handle-based resources)   │
│    └── Syscall Dispatch   (SYSCALL/SYSRET MSRs)      │
├──────────────────────────────────────────────────────┤
│  Kernel Core                                         │
│    ├── Scheduler          (preemptive round-robin)   │
│    └── Sync Primitives    (spinlocks, etc.)          │
├──────────────────────────────────────────────────────┤
│  Memory Management                                   │
│    ├── Frame Allocator    (bitmap-based, 4 KiB)      │
│    └── Heap Allocator     (linked-list free-list)    │
├──────────────────────────────────────────────────────┤
│  Hardware Abstraction Layer (HAL)                     │
│    ├── GDT / TSS          (CPU segmentation)         │
│    ├── IDT                (interrupt dispatch)       │
│    ├── PIC                (8259 interrupt controller) │
│    ├── PIT                (timer at 1000 Hz)         │
│    ├── PS/2 Keyboard      (scancode set 1, IRQ1)     │
│    ├── PS/2 Mouse         (3-byte packets, IRQ12)    │
│    ├── RTC                (real-time clock)          │
│    ├── ACPI               (system table parsing)     │
│    ├── PCI                (bus enumeration)          │
│    ├── Serial             (COM1 debug output)        │
│    └── Port I/O           (hardware access)          │
├──────────────────────────────────────────────────────┤
│  Graphics                                            │
│    ├── Framebuffer        (double-buffered, 1920×1080)│
│    ├── Font               (8×16 bitmap font)         │
│    └── Console            (text output with scroll)  │
├──────────────────────────────────────────────────────┤
│  Desktop Environment                                 │
│    ├── Window Manager     (overlapping windows)      │
│    ├── Taskbar            (Start button + clock)     │
│    ├── Start Menu         (app launcher)             │
│    ├── Mouse Cursor       (12×19 arrow pointer)      │
│    └── Built-in Apps      (6 applications)           │
├──────────────────────────────────────────────────────┤
│  UEFI Bootloader                                     │
│    ├── GOP init           (framebuffer setup)        │
│    ├── ELF loader         (kernel loading)           │
│    └── Paging setup       (higher-half mapping)      │
└─────────────────────────────────────────────────────┘
```

## Features

### Interactive Shell
- 21+ built-in commands: `help`, `clear`, `mem`, `cpu`, `pci`, `tasks`, `uptime`,
  `date`, `hexdump`, `color`, `echo`, `ver`, `reboot`, `shutdown`, `desktop`, and more
- **Tab completion** for all commands
- **Command history** (Up/Down arrow keys)
- Status bar with uptime and IRQ counters

### Graphical Desktop Environment
- **Window manager** with overlapping windows, focus cycling (F2), and close buttons
- **Taskbar** with Start button, running app buttons, and real-time clock
- **Start menu** with application launcher
- **Desktop icons** — click to select, click again to launch
- **Mouse support** — full click handling on windows, taskbar, icons, and start menu

### Built-in Applications
| App | Description |
|-----|-------------|
| System Info | RAM, uptime, CPU registers, PCI devices (scrollable) |
| Task Manager | Live task list with kill support |
| Notepad | Full text editor with cursor, insert/delete, line handling |
| Calculator | Arithmetic operations with chained expressions |
| About | System information panel |
| Terminal | Mini shell inside a desktop window |

### Hardware Drivers
- **PS/2 Keyboard** — scancode set 1, full key mapping, modifier tracking
- **PS/2 Mouse** — 3-byte packets, IntelliMouse scroll detection, IRQ12
- **PIT Timer** — 1000 Hz for preemptive scheduling
- **RTC** — real-time clock for date/time display
- **PCI** — bus/device/function enumeration
- **ACPI** — RSDP/RSDT/XSDT table parsing
- **Serial** — COM1 at 115200 baud for debug logging

### Kernel Features
- **Preemptive scheduler** — round-robin with FPU/SSE context save
- **Dynamic heap** — linked-list allocator with `alloc` crate support
- **Exception handlers** — all 20 x86_64 exceptions with register dumps
- **BSOD panic screen** — graphical blue screen on kernel panic
- **Double-buffered framebuffer** — tear-free rendering
- **Stack guard pages** — stack overflow detection

## Project Structure

```
CantayaOS/
├── Cargo.toml              # Workspace root
├── rust-toolchain.toml     # Nightly Rust + required components
├── x86_64-cantaya.json     # Custom bare-metal target for the kernel
│
├── shared/                 # Boot protocol (shared between bootloader & kernel)
│   └── src/
│       ├── boot_info.rs    # BootInfo structure passed to kernel
│       └── memory.rs       # Memory region types
│
├── bootloader/             # UEFI bootloader
│   └── src/
│       ├── main.rs         # UEFI entry point & boot sequence
│       ├── framebuffer.rs  # GOP initialization
│       ├── loader.rs       # ELF parser & kernel loader
│       ├── paging.rs       # 4-level page table setup
│       ├── splash.rs       # Boot splash screen
│       └── cmdline.rs      # Command-line parsing
│
├── kernel/                 # The CantayaOS kernel
│   ├── linker.ld           # Linker script (higher-half at 0xFFFFFFFF80000000)
│   └── src/
│       ├── main.rs         # Kernel entry & initialization sequence
│       ├── shell.rs        # Interactive kernel shell (21+ commands)
│       ├── logging.rs      # Serial-based kernel logging
│       │
│       ├── hal/            # Hardware Abstraction Layer
│       │   ├── cpu.rs      # Entry point (_start), CR register access, SSE
│       │   ├── gdt.rs      # Global Descriptor Table + TSS
│       │   ├── idt.rs      # Interrupt Descriptor Table + exception handlers
│       │   ├── interrupts.rs # PIC setup, interrupt enable/disable
│       │   ├── keyboard.rs # PS/2 keyboard driver (scancode set 1)
│       │   ├── mouse.rs    # PS/2 mouse driver (IRQ12, 3-byte packets)
│       │   ├── pit.rs      # Programmable Interval Timer (1000 Hz)
│       │   ├── rtc.rs      # Real-Time Clock driver
│       │   ├── acpi.rs     # ACPI table parser
│       │   ├── pci.rs      # PCI bus enumeration
│       │   ├── port.rs     # x86 port I/O (in/out instructions)
│       │   └── serial.rs   # UART 16550 serial port driver
│       │
│       ├── memory/         # Memory management
│       │   ├── frame_allocator.rs  # Bitmap-based physical frame allocator
│       │   └── heap.rs     # Linked-list kernel heap allocator
│       │
│       ├── core_kernel/    # Kernel core services
│       │   ├── scheduler.rs # Preemptive round-robin scheduler
│       │   └── sync.rs     # Synchronization primitives
│       │
│       ├── executive/      # Executive services
│       │   ├── process.rs  # Process & thread management
│       │   ├── object.rs   # Object manager / handle system
│       │   └── syscall.rs  # SYSCALL/SYSRET MSR programming
│       │
│       ├── graphics/       # Visual output
│       │   ├── framebuffer.rs # Double-buffered pixel framebuffer
│       │   ├── font.rs     # Built-in 8×16 bitmap font
│       │   └── console.rs  # Text console with scrolling + color
│       │
│       └── desktop/        # Graphical desktop environment
│           ├── mod.rs      # Desktop shell, icons, start menu, mouse cursor
│           ├── wm.rs       # Window manager (create/close/focus/cycle)
│           ├── taskbar.rs  # Taskbar with Start button + clock
│           └── apps.rs     # 6 built-in desktop applications
│
└── scripts/
    ├── build.ps1           # Build all components + create ESP
    └── run-qemu.ps1        # Launch in QEMU with UEFI
```

## Prerequisites

- **Rust nightly** (managed by `rust-toolchain.toml` — auto-installed)
- **QEMU** with x86_64 support: `winget install QEMU.QEMU`
- **OVMF** UEFI firmware (`code.fd` and `vars.fd` in project root)

## Building

```powershell
# Build everything (bootloader + kernel + ESP image)
.\scripts\build.ps1

# Build in release mode
.\scripts\build.ps1 -Release

# Build kernel only
cargo build -p cantaya_kernel --target x86_64-unknown-none `
  -Zbuild-std=core,alloc,compiler_builtins `
  -Zbuild-std-features=compiler-builtins-mem
```

## Running

```powershell
# Run in QEMU (builds first if needed)
.\scripts\run-qemu.ps1

# Run without rebuilding
.\scripts\run-qemu.ps1 -NoBuild

# Run with GDB debug server on port 1234
.\scripts\run-qemu.ps1 -Debug
```

### Shell Commands

Once booted, you'll be in the interactive shell. Type `help` for a full list:

```
help       Show all commands          mem        Memory statistics
clear      Clear the screen           cpu        CPU register dump
ver        Version info               pci        PCI device list
uptime     System uptime              tasks      Running tasks
date       Current date/time          hexdump    Memory hex viewer
echo       Print text                 color      Change color scheme
reboot     Restart system             shutdown   Power off
desktop    Launch graphical desktop   ...and more
```

## Boot Sequence

1. **UEFI Firmware** discovers `BOOTX64.EFI` on the ESP
2. **Bootloader** initializes GOP (display), shows splash screen
3. **Bootloader** loads `kernel.elf` from ESP, parses ELF segments
4. **Bootloader** creates 4-level page tables (identity + higher-half mapping)
5. **Bootloader** exits UEFI boot services, captures memory map
6. **Bootloader** switches to new page tables, jumps to kernel `_start`
7. **Kernel HAL** initializes GDT, IDT, serial port, SSE/FPU
8. **Kernel Memory** initializes frame allocator (from UEFI memory map), then heap
9. **Kernel** initializes ACPI, PCI enumeration, scheduler, SYSCALL MSRs
10. **Kernel** initializes PS/2 mouse, enables interrupts, starts PIT at 1000 Hz
11. **Kernel Graphics** initializes double-buffered framebuffer console
12. **Kernel** enters interactive shell (type `desktop` for GUI)

## Design Decisions

| Decision | Rationale |
|----------|-----------|
| Rust `no_std` | Memory safety without runtime overhead; no OS dependencies |
| UEFI only | Modern standard, no legacy BIOS complexity |
| Higher-half kernel | Shared kernel mapping across all processes, efficient syscalls |
| 2 MiB huge pages (boot) | Simple initial mapping; kernel switches to 4 KiB later |
| Bitmap frame allocator | Fixed overhead, cache-friendly, simple to debug |
| Linked-list heap | Good enough for early development; slab allocator planned |
| Round-robin scheduler | Simplest correct preemptive scheduler; priority queues planned |
| Double-buffered FB | Tear-free rendering for desktop environment |
| Legacy PIC + PIT | Simplest path to working interrupts; APIC planned |
| PS/2 mouse/keyboard | Universal QEMU support; USB HID planned |

## Roadmap

- [x] **Phase 1** — Boot & Core
  - [x] UEFI bootloader with GOP and splash screen
  - [x] Kernel loading and higher-half mapping
  - [x] GDT, IDT, all 20 exception handlers
  - [x] Physical frame allocator + kernel heap
  - [x] Framebuffer console with color and scrolling
  - [x] Preemptive round-robin scheduler
  - [x] PS/2 keyboard driver with full key mapping
  - [x] PIT timer (1000 Hz)
  - [x] Interactive shell with 21+ commands

- [x] **Phase 2** — Hardware & Diagnostics
  - [x] RTC driver (date/time)
  - [x] ACPI table parsing (RSDP/RSDT/XSDT)
  - [x] PCI bus enumeration
  - [x] BSOD panic screen
  - [x] Serial debug logging
  - [x] SYSCALL/SYSRET MSR programming

- [x] **Phase 3** — Desktop Environment
  - [x] Window manager with overlapping windows
  - [x] Desktop with icons, start menu
  - [x] Taskbar with Start button, app buttons, clock
  - [x] PS/2 mouse driver (IRQ12)
  - [x] Mouse cursor and full click handling
  - [x] 6 built-in applications

- [ ] **Phase 4** — Processes & Syscalls
  - [ ] Virtual memory manager (per-process page tables)
  - [ ] User-mode process loading (ELF)
  - [ ] Ring 0/3 separation with SYSCALL/SYSRET
  - [ ] IPC mechanisms

- [ ] **Phase 5** — Filesystem & I/O
  - [ ] VFS abstraction layer
  - [ ] FAT32 filesystem driver
  - [ ] Block device abstraction
  - [ ] AHCI/NVMe disk driver

- [ ] **Phase 6** — Advanced
  - [ ] SMP (multi-core) support
  - [ ] APIC (replacing legacy PIC)
  - [ ] USB HID (mouse/keyboard)
  - [ ] Network stack (TCP/IP)

## License

MIT
