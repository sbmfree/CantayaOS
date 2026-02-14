# CantayaOS

A modern 64-bit Windows-inspired operating system written in Rust, featuring a
graphical desktop environment, PS/2 mouse and keyboard drivers, preemptive
multitasking, virtual memory management, storage with FAT32, networking,
and a full interactive shell — all running bare-metal on x86_64.

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
│    └── Sync Primitives    (spinlocks, ticket locks)  │
├──────────────────────────────────────────────────────┤
│  Memory Management                                   │
│    ├── Frame Allocator    (bitmap-based, 4 KiB)      │
│    ├── Heap Allocator     (linked-list free-list)    │
│    ├── Page Table Manager (4-level x86_64 paging)    │
│    └── VMM                (virtual address space mgr) │
├──────────────────────────────────────────────────────┤
│  Storage                                             │
│    ├── Block Device       (virtio-blk driver)        │
│    ├── FAT32              (filesystem driver)        │
│    └── VFS                (virtual filesystem layer) │
├──────────────────────────────────────────────────────┤
│  Networking                                          │
│    ├── Ethernet           (frame tx/rx)              │
│    ├── ARP                (address resolution)       │
│    ├── IPv4               (packet routing)           │
│    ├── ICMP               (ping support)             │
│    └── UDP                (datagram transport)       │
├──────────────────────────────────────────────────────┤
│  Hardware Abstraction Layer (HAL)                     │
│    ├── GDT / TSS          (StaticCell, no static mut)│
│    ├── IDT                (StaticCell, no static mut)│
│    ├── PIC                (8259 interrupt controller) │
│    ├── PIT                (timer at 1000 Hz)         │
│    ├── PS/2 Keyboard      (scancode set 1, IRQ1)     │
│    ├── PS/2 Mouse         (3-byte packets, IRQ12)    │
│    ├── RTC                (real-time clock)          │
│    ├── ACPI               (RSDP/FADT/MADT/HPET/MCFG)│
│    ├── PCI                (bus enumeration)          │
│    ├── Serial             (COM1 debug output)        │
│    ├── PC Speaker         (beep tones)               │
│    ├── VirtIO Block       (disk I/O via virtqueues)  │
│    ├── VirtIO Net         (network I/O)              │
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
│    └── Built-in Apps      (12 applications)          │
├──────────────────────────────────────────────────────┤
│  UEFI Bootloader                                     │
│    ├── GOP init           (framebuffer setup)        │
│    ├── ELF loader         (kernel loading)           │
│    └── Paging setup       (higher-half mapping)      │
└─────────────────────────────────────────────────────┘
```

## Features

### Interactive Shell
- 30+ built-in commands: `help`, `clear`, `mem`, `cpu`, `pci`, `tasks`, `uptime`,
  `date`, `hexdump`, `color`, `echo`, `ver`, `reboot`, `shutdown`, `desktop`,
  `vmm`, `vtop`, `ping`, `dmesg`, `free`, `top`, `neofetch`, `lsblk`, and more
- **Tab completion** for all commands
- **Command history** (Up/Down arrow keys)
- **Command aliasing** (`alias`, `unalias`)
- **Script execution** (`run <script>`)
- **Built-in text editor** (vi-like, launched with `edit`)
- **Calculator** (inline expression evaluation)
- Status bar with uptime and IRQ counters

### Graphical Desktop Environment
- **Window manager** with overlapping windows, focus cycling (F2), and close buttons
- **Taskbar** with Start button, running app buttons, and real-time clock
- **Start menu** with application launcher
- **Desktop icons** — click to select, click again to launch
- **Mouse support** — full click handling on windows, taskbar, icons, and start menu
- **Custom wallpaper** and cursor rendering

### Built-in Applications
| App | Description |
|-----|-------------|
| System Info | RAM, uptime, CPU registers, PCI devices (scrollable) |
| Task Manager | Live task list with kill support |
| Notepad | Full text editor with cursor, insert/delete, line handling |
| Calculator | Arithmetic operations with chained expressions |
| About | System information panel |
| Terminal | Mini shell inside a desktop window |
| File Browser | Navigate FAT32 filesystem |
| Paint | Pixel drawing application |
| Clock | Real-time clock display |
| Settings | System configuration |
| Snake | Classic snake game |
| Minesweeper | Mine-sweeping puzzle game |

### Hardware Drivers
- **PS/2 Keyboard** — scancode set 1, full key mapping, modifier tracking
- **PS/2 Mouse** — 3-byte packets, IntelliMouse scroll detection, IRQ12
- **PIT Timer** — 1000 Hz for preemptive scheduling
- **RTC** — real-time clock for date/time display
- **PCI** — bus/device/function enumeration
- **ACPI** — RSDP/FADT/MADT/HPET/MCFG table parsing, S5 shutdown
- **Serial** — COM1 at 115200 baud for debug logging
- **PC Speaker** — programmable tone generation
- **VirtIO Block** — disk I/O via virtqueues (32 MiB disk)
- **VirtIO Net** — network device support

### Storage & Filesystem
- **Block device abstraction** — sector-based read/write
- **FAT32 driver** — read/write files and directories
- **VFS layer** — unified filesystem interface
- **Auto-provisioning** — creates default directory structure on first boot

### Networking
- **Ethernet** — frame transmission and reception
- **ARP** — address resolution protocol
- **IPv4** — packet routing and forwarding
- **ICMP** — echo request/reply (ping)
- **UDP** — datagram transport

### Kernel Features
- **Virtual memory manager** — 4-level page table manipulation, dynamic virtual
  allocation, MMIO mapping, per-process page tables
- **Preemptive scheduler** — round-robin with FPU/SSE context save
- **Dynamic heap** — linked-list allocator with `alloc` crate support
- **Unified error types** — `KernelError` enum across subsystems
- **Exception handlers** — all 20 x86_64 exceptions with register dumps
- **BSOD panic screen** — graphical blue screen on kernel panic
- **Double-buffered framebuffer** — tear-free rendering
- **Stack guard pages** — stack overflow detection
- **Test harness** — in-kernel test framework

## Project Structure

```
CantayaOS/
├── Cargo.toml              # Workspace root
├── rust-toolchain.toml     # Nightly Rust + required components
├── x86_64-cantaya.json     # Custom bare-metal target (SSE/SSE2 enabled)
│
├── shared/                 # Boot protocol (shared between bootloader & kernel)
│   └── src/
│       ├── boot_info.rs    # BootInfo structure passed to kernel
│       ├── memory.rs       # Memory region types
│       └── lib.rs          # Crate root
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
│       ├── error.rs        # Unified KernelError types
│       ├── logging.rs      # Serial-based kernel logging
│       ├── testing.rs      # In-kernel test harness
│       │
│       ├── shell/          # Interactive kernel shell
│       │   ├── mod.rs      # Shell loop, dispatch, tab completion, history
│       │   ├── calc.rs     # Expression calculator
│       │   ├── editor.rs   # Built-in text editor
│       │   └── commands/   # Command implementations
│       │       ├── fs.rs       # Filesystem commands (ls, cat, mkdir, etc.)
│       │       ├── net.rs      # Network commands (ping, ip, arp, netstat)
│       │       ├── system.rs   # System commands (mem, cpu, free, vmm, vtop, etc.)
│       │       └── misc.rs     # Misc commands (echo, color, alias, etc.)
│       │
│       ├── hal/            # Hardware Abstraction Layer
│       │   ├── cpu.rs      # Entry point (_start), CR access, SSE, CPUID
│       │   ├── gdt.rs      # GDT + TSS (StaticCell, no static mut)
│       │   ├── idt.rs      # IDT + exception handlers (StaticCell)
│       │   ├── interrupts.rs # PIC setup, interrupt enable/disable
│       │   ├── keyboard.rs # PS/2 keyboard driver (scancode set 1)
│       │   ├── mouse.rs    # PS/2 mouse driver (IRQ12, 3-byte packets)
│       │   ├── pit.rs      # Programmable Interval Timer (1000 Hz)
│       │   ├── rtc.rs      # Real-Time Clock driver
│       │   ├── acpi.rs     # ACPI table parser (FADT, MADT, HPET, MCFG)
│       │   ├── pci.rs      # PCI bus enumeration
│       │   ├── port.rs     # x86 port I/O (in/out instructions)
│       │   ├── serial.rs   # UART 16550 serial port driver
│       │   ├── speaker.rs  # PC speaker tone generation
│       │   ├── virtio.rs   # VirtIO common (virtqueues, feature negotiation)
│       │   ├── virtio_blk.rs # VirtIO block device driver
│       │   └── virtio_net.rs # VirtIO network device driver
│       │
│       ├── memory/         # Memory management
│       │   ├── mod.rs              # Subsystem init (frames → heap → VMM)
│       │   ├── frame_allocator.rs  # Bitmap-based physical frame allocator
│       │   ├── heap.rs             # Linked-list kernel heap allocator
│       │   ├── paging.rs           # x86_64 4-level page table management
│       │   └── vmm.rs              # Virtual address space manager
│       │
│       ├── core_kernel/    # Kernel core services
│       │   ├── scheduler.rs # Preemptive round-robin scheduler
│       │   └── sync.rs     # Synchronization primitives (ticket locks)
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
│       ├── desktop/        # Graphical desktop environment
│       │   ├── mod.rs      # Desktop shell & main loop
│       │   ├── wm.rs       # Window manager (create/close/focus/cycle)
│       │   ├── taskbar.rs  # Taskbar with Start button + clock
│       │   ├── icons.rs    # Desktop icon rendering & selection
│       │   ├── cursor.rs   # Mouse cursor rendering
│       │   ├── wallpaper.rs # Desktop wallpaper
│       │   ├── drawing.rs  # Shared drawing primitives
│       │   ├── input.rs    # Input event dispatch
│       │   └── apps/       # 12 built-in desktop applications
│       │       ├── about.rs      # About dialog
│       │       ├── calculator.rs # Calculator app
│       │       ├── clock.rs      # Clock display
│       │       ├── filebrowser.rs # File browser (FAT32)
│       │       ├── minesweeper.rs # Minesweeper game
│       │       ├── notepad.rs    # Text editor
│       │       ├── paint.rs      # Paint application
│       │       ├── settings.rs   # Settings panel
│       │       ├── snake.rs      # Snake game
│       │       ├── sysinfo.rs    # System info viewer
│       │       ├── taskmgr.rs    # Task manager
│       │       └── terminal.rs   # In-desktop terminal
│       │
│       ├── storage/        # Storage subsystem
│       │   ├── mod.rs      # Storage init & provisioning
│       │   ├── block.rs    # Block device abstraction
│       │   ├── fat32.rs    # FAT32 filesystem driver
│       │   └── vfs.rs      # Virtual filesystem layer
│       │
│       └── net/            # Networking subsystem
│           ├── mod.rs      # Network init
│           ├── ethernet.rs # Ethernet frame handling
│           ├── arp.rs      # ARP protocol
│           ├── ipv4.rs     # IPv4 protocol
│           ├── icmp.rs     # ICMP (ping)
│           └── udp.rs      # UDP transport
│
└── scripts/
    ├── build.ps1           # Build all components + create ESP
    ├── run-qemu.ps1        # Launch in QEMU with UEFI
    ├── create-disk.ps1     # Create FAT32 disk image
    └── run-tests.ps1       # Run in-kernel test suite
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
desktop    Launch graphical desktop   free       Detailed memory report
vmm        Virtual memory status      vtop       Virt-to-phys translation
ping       ICMP echo requests         dmesg      Kernel log
top        Interactive task monitor   neofetch   System info with ASCII art
lsblk      List block devices         drivers    Loaded drivers
ls/cat/..  Filesystem commands        ...and more
```

## Boot Sequence

1. **UEFI Firmware** discovers `BOOTX64.EFI` on the ESP
2. **Bootloader** initializes GOP (display), shows splash screen
3. **Bootloader** loads `kernel.elf` from ESP, parses ELF segments
4. **Bootloader** creates 4-level page tables (identity + higher-half mapping)
5. **Bootloader** exits UEFI boot services, captures memory map
6. **Bootloader** switches to new page tables, jumps to kernel `_start`
7. **Kernel HAL** initializes GDT, IDT, serial port, SSE/FPU
8. **Kernel Memory** initializes frame allocator → heap → virtual memory manager
9. **Kernel** initializes ACPI, PCI enumeration, scheduler, SYSCALL MSRs
10. **Kernel** initializes PS/2 mouse, enables interrupts, starts PIT at 1000 Hz
11. **Kernel Storage** initializes VirtIO block device, mounts FAT32 filesystem
12. **Kernel Graphics** initializes double-buffered framebuffer console
13. **Kernel** runs autoexec script, enters interactive shell (type `desktop` for GUI)

## Design Decisions

| Decision | Rationale |
|----------|-----------|
| Rust `no_std` | Memory safety without runtime overhead; no OS dependencies |
| UEFI only | Modern standard, no legacy BIOS complexity |
| Higher-half kernel | Shared kernel mapping across all processes, efficient syscalls |
| 2 MiB huge pages (boot) | Simple initial mapping; kernel VMM uses 4 KiB pages |
| Bitmap frame allocator | Fixed overhead, cache-friendly, simple to debug |
| Linked-list heap | Good enough for early development; slab allocator planned |
| 4-level page tables | Full x86_64 virtual memory with per-process isolation |
| StaticCell for GDT/IDT | Eliminates `static mut` unsafety, safe one-time init |
| Round-robin scheduler | Simplest correct preemptive scheduler; priority queues planned |
| Double-buffered FB | Tear-free rendering for desktop environment |
| Legacy PIC + PIT | Simplest path to working interrupts; APIC planned |
| VirtIO drivers | Efficient paravirtualized I/O for QEMU |
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
  - [x] Interactive shell with 30+ commands

- [x] **Phase 2** — Hardware & Diagnostics
  - [x] RTC driver (date/time)
  - [x] ACPI table parsing (RSDP/FADT/MADT/HPET/MCFG)
  - [x] PCI bus enumeration
  - [x] BSOD panic screen
  - [x] Serial debug logging
  - [x] SYSCALL/SYSRET MSR programming
  - [x] PC speaker driver

- [x] **Phase 3** — Desktop Environment
  - [x] Window manager with overlapping windows
  - [x] Desktop with icons, start menu, wallpaper
  - [x] Taskbar with Start button, app buttons, clock
  - [x] PS/2 mouse driver (IRQ12)
  - [x] Mouse cursor and full click handling
  - [x] 12 built-in applications

- [x] **Phase 4** — Storage & Filesystem
  - [x] VFS abstraction layer
  - [x] FAT32 filesystem driver (read/write)
  - [x] Block device abstraction
  - [x] VirtIO block disk driver
  - [x] Auto-provisioning on first boot

- [x] **Phase 5** — Code Quality & VMM
  - [x] Eliminate `static mut` (StaticCell pattern for GDT/IDT)
  - [x] Unified `KernelError` types
  - [x] Shell split into modular subcommands (8 files)
  - [x] Desktop apps split into individual modules (13 files)
  - [x] In-kernel test harness
  - [x] Virtual memory manager (4-level page table manipulation)
  - [x] Per-process page table creation/teardown
  - [x] Dynamic virtual allocation with MMIO support
  - [x] `vmm` and `vtop` shell commands

- [x] **Phase 6** — Networking
  - [x] VirtIO network driver
  - [x] Ethernet frame handling
  - [x] ARP protocol
  - [x] IPv4 routing
  - [x] ICMP (ping)
  - [x] UDP transport

- [ ] **Phase 7** — Userspace & Advanced
  - [ ] User-mode process loading (ELF)
  - [ ] Ring 0/3 separation with SYSCALL/SYSRET
  - [ ] IPC mechanisms
  - [ ] SMP (multi-core) support
  - [ ] APIC (replacing legacy PIC)
  - [ ] USB HID (mouse/keyboard)
  - [ ] TCP transport

## License

MIT
