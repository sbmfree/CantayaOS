# CantayaOS

A modern 64-bit Windows-inspired operating system written in Rust.

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
│    └── Syscall Dispatch   (user↔kernel transition)   │
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
│    ├── PIC                (interrupt controller)     │
│    ├── Serial             (COM1 debug output)        │
│    └── Port I/O           (hardware access)          │
├──────────────────────────────────────────────────────┤
│  Graphics                                            │
│    ├── Framebuffer        (UEFI GOP pixel access)    │
│    ├── Font               (8x16 bitmap font)        │
│    └── Console            (text output with scroll)  │
├──────────────────────────────────────────────────────┤
│  UEFI Bootloader                                     │
│    ├── GOP init           (framebuffer setup)        │
│    ├── ELF loader         (kernel loading)           │
│    └── Paging setup       (higher-half mapping)      │
└─────────────────────────────────────────────────────┘
```

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
├── bootloader/             # UEFI bootloader application
│   └── src/
│       ├── main.rs         # UEFI entry point & boot sequence
│       ├── framebuffer.rs  # GOP initialization
│       ├── loader.rs       # ELF parser & kernel loader
│       └── paging.rs       # 4-level page table setup
│
├── kernel/                 # The CantayaOS kernel
│   ├── linker.ld           # Linker script (higher-half at 0xFFFFFFFF80000000)
│   └── src/
│       ├── main.rs         # Kernel entry & initialization sequence
│       ├── logging.rs      # Serial-based kernel logging
│       │
│       ├── hal/            # Hardware Abstraction Layer
│       │   ├── cpu.rs      # Entry point (_start), CR register access
│       │   ├── gdt.rs      # Global Descriptor Table + TSS
│       │   ├── idt.rs      # Interrupt Descriptor Table + handlers
│       │   ├── interrupts.rs # PIC setup, interrupt enable/disable
│       │   ├── port.rs     # x86 port I/O (in/out instructions)
│       │   └── serial.rs   # UART 16550 serial port driver
│       │
│       ├── memory/         # Memory management
│       │   ├── frame_allocator.rs  # Bitmap-based physical frame allocator
│       │   └── heap.rs     # Linked-list kernel heap allocator
│       │
│       ├── core_kernel/    # Kernel core services
│       │   ├── scheduler.rs # Round-robin preemptive scheduler
│       │   └── sync.rs     # Synchronization primitives
│       │
│       ├── executive/      # Executive services (highest kernel layer)
│       │   ├── process.rs  # Process & thread management (stub)
│       │   ├── object.rs   # Object manager / handle system (stub)
│       │   └── syscall.rs  # System call interface (stub)
│       │
│       └── graphics/       # Visual output
│           ├── framebuffer.rs # Pixel-level framebuffer access
│           ├── font.rs     # Built-in 8x16 bitmap font
│           └── console.rs  # Text console with scrolling
│
└── scripts/
    ├── build.ps1           # Build all components + create ESP
    └── run-qemu.ps1        # Launch in QEMU with UEFI
```

## Prerequisites

- **Rust nightly** (managed by `rust-toolchain.toml` — auto-installed)
- **QEMU** with x86_64 support: `winget install QEMU.QEMU`
- **OVMF** UEFI firmware (usually bundled with QEMU)

## Building

```powershell
# Build everything (bootloader + kernel + ESP image)
.\scripts\build.ps1

# Build in release mode
.\scripts\build.ps1 -Release
```

## Running

```powershell
# Run in QEMU (builds first if needed)
.\scripts\run-qemu.ps1

# Run without rebuilding
.\scripts\run-qemu.ps1 -NoBuild

# Run with GDB debug server
.\scripts\run-qemu.ps1 -Debug
```

## Boot Sequence

1. **UEFI Firmware** discovers `BOOTX64.EFI` on the ESP
2. **Bootloader** initializes GOP (display), loads `kernel.elf` from ESP
3. **Bootloader** parses ELF, copies segments to physical memory
4. **Bootloader** creates 4-level page tables (identity + higher-half mapping)
5. **Bootloader** exits UEFI boot services, captures memory map
6. **Bootloader** switches to new page tables, jumps to kernel `_start`
7. **Kernel HAL** initializes GDT, IDT, serial port
8. **Kernel Memory** initializes frame allocator (from UEFI memory map), then heap
9. **Kernel** enables interrupts (PIC + timer + keyboard)
10. **Kernel Graphics** initializes framebuffer console, displays boot messages
11. **Kernel** enters idle loop (HLT until next interrupt)

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
| Serial debug output | Works even when framebuffer isn't initialized; QEMU captures it |

## Roadmap

- [ ] **Phase 1** — Boot & display (current)
  - [x] UEFI bootloader with GOP
  - [x] Kernel loading and higher-half mapping
  - [x] GDT, IDT, exception handlers
  - [x] Physical frame allocator
  - [x] Kernel heap
  - [x] Framebuffer console
  - [x] Preemptive scheduler (basic)
  - [ ] PS/2 keyboard input processing
  - [ ] Context switch implementation

- [ ] **Phase 2** — Processes & syscalls
  - [ ] Virtual memory manager (per-process page tables)
  - [ ] SYSCALL/SYSRET interface
  - [ ] User-mode process loading (ELF)
  - [ ] Basic security model (Ring 0/3 separation)

- [ ] **Phase 3** — Filesystem & I/O
  - [ ] VFS abstraction layer
  - [ ] FAT32 filesystem driver
  - [ ] Block device abstraction
  - [ ] AHCI/NVMe disk driver

- [ ] **Phase 4** — GUI
  - [ ] Window manager (compositing)
  - [ ] Desktop + taskbar + start menu concept
  - [ ] Mouse driver (PS/2 or USB HID)
  - [ ] Basic widget toolkit

- [ ] **Phase 5** — Advanced
  - [ ] SMP (multi-core) support
  - [ ] APIC (replacing legacy PIC)
  - [ ] Network stack (TCP/IP)
  - [ ] Windows compatibility layer (long-term vision)

## License

MIT
