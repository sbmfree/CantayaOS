# CantayaOS

A hybrid kernel operating system written in Rust for ARM64 (AArch64) architecture.

## Overview

CantayaOS is a modern operating system featuring a Windows NT-like hybrid kernel design, combining the modularity of microkernels with the performance of monolithic kernels.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      User Mode                               │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐            │
│  │   Apps      │ │   Services  │ │   Drivers   │            │
│  └─────────────┘ └─────────────┘ └─────────────┘            │
├─────────────────────────────────────────────────────────────┤
│                    System Call Interface                     │
├─────────────────────────────────────────────────────────────┤
│                      Kernel Mode                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │                    Executive                         │    │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐   │    │
│  │  │ Process │ │ Memory  │ │   I/O   │ │   IPC   │   │    │
│  │  │ Manager │ │ Manager │ │ Manager │ │ Manager │   │    │
│  │  └─────────┘ └─────────┘ └─────────┘ └─────────┘   │    │
│  └─────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │              Hardware Abstraction Layer (HAL)        │    │
│  └─────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │                    Microkernel                       │    │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐   │    │
│  │  │ Sched-  │ │ Except- │ │   MMU   │ │  Timer  │   │    │
│  │  │  uler   │ │  ions   │ │         │ │         │   │    │
│  │  └─────────┘ └─────────┘ └─────────┘ └─────────┘   │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                            │
                    ┌───────┴───────┐
                    │   Hardware    │
                    │   (ARM64)     │
                    └───────────────┘
```

## Features

- **Hybrid Kernel**: Combines microkernel security with monolithic performance
- **ARM64 Native**: Built specifically for AArch64 architecture
- **Windows NT-like Design**: Familiar concepts (EPROCESS, ETHREAD, handles, etc.)
- **Modern Rust**: Memory-safe kernel with zero-cost abstractions
- **Preemptive Multitasking**: Priority-based scheduler with 32 priority levels
- **Virtual Memory**: 4-level page tables with demand paging support
- **IPC Mechanisms**: Events, mutexes, semaphores, pipes

## Project Structure

```
CantayaOS/
├── kernel/
│   ├── src/
│   │   ├── arch/aarch64/   # ARM64 architecture code
│   │   ├── hal/            # Hardware Abstraction Layer
│   │   ├── mm/             # Memory management
│   │   ├── process/        # Process & thread management
│   │   ├── ipc/            # Inter-process communication
│   │   └── drivers/        # Driver framework
│   ├── linker.ld           # Kernel linker script
│   └── aarch64-cantaya.json # Target specification
├── bootloader/             # UEFI bootloader
├── userspace/              # User applications (future)
└── tools/                  # Build tools
```

## Building

### Prerequisites

- Rust nightly toolchain
- `aarch64-unknown-none` target
- LLVM tools (for objcopy)
- QEMU (for testing)

### Build Commands

```bash
# Install Rust nightly
rustup default nightly
rustup target add aarch64-unknown-none
rustup component add rust-src llvm-tools-preview

# Build kernel
./build.sh

# Build debug version
./build.sh debug
```

## Running

```bash
# Run in QEMU
./run.sh

# Or manually:
qemu-system-aarch64 -M virt -cpu cortex-a72 -m 128M -nographic -kernel cantaya.bin
```

## System Calls

CantayaOS uses Windows NT-style system calls:

| Syscall | Number | Description |
|---------|--------|-------------|
| NtCreateProcess | 0x0001 | Create a new process |
| NtCreateThread | 0x0002 | Create a new thread |
| NtTerminateProcess | 0x0003 | Terminate a process |
| NtAllocateVirtualMemory | 0x0010 | Allocate memory |
| NtCreateFile | 0x0020 | Create/open a file |
| NtReadFile | 0x0021 | Read from file |
| NtWriteFile | 0x0022 | Write to file |
| NtCreateEvent | 0x0030 | Create event object |
| NtWaitForSingleObject | 0x0040 | Wait for object |

## License

MIT License

## Contributing

Contributions welcome! Please read the contributing guidelines first.
