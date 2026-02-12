# CantayaOS

A hybrid kernel operating system written in Rust supporting both ARM64 (AArch64) and x86_64 architectures.

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
                    │ ARM64/x86_64  │
                    └───────────────┘
```

## Features

- **Hybrid Kernel**: Combines microkernel security with monolithic performance
- **Multi-Architecture**: Supports both ARM64 (AArch64) and x86_64 architectures
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
│   │   ├── arch/
│   │   │   ├── aarch64/     # ARM64 architecture code
│   │   │   └── x86_64/      # x86_64 architecture code
│   │   ├── hal/             # Hardware Abstraction Layer
│   │   ├── mm/              # Memory management
│   │   ├── process/         # Process & thread management
│   │   ├── ipc/             # Inter-process communication
│   │   └── drivers/         # Driver framework
│   ├── linker.ld            # ARM64 linker script
│   ├── linker-x86_64.ld     # x86_64 linker script
│   ├── aarch64-cantaya.json # ARM64 target specification
│   └── x86_64-cantaya.json  # x86_64 target specification
├── bootloader/              # UEFI bootloader
├── userspace/               # User applications (future)
└── tools/                   # Build tools
```

## Building

### Prerequisites

- Rust nightly toolchain
- `aarch64-unknown-none` and/or `x86_64-unknown-none` target
- LLVM tools (for objcopy)
- QEMU (for testing)

### Build Commands

```bash
# Install Rust nightly
rustup default nightly
rustup target add aarch64-unknown-none x86_64-unknown-none
rustup component add rust-src llvm-tools-preview

# Build kernel for ARM64 (default)
./build.sh

# Build kernel for x86_64
ARCH=x86_64 ./build.sh

# Build debug version
./build.sh debug
ARCH=x86_64 ./build.sh debug

# Using Makefile
make release           # Build ARM64 release
make release-x86_64    # Build x86_64 release
```

## Running

```bash
# Run ARM64 in QEMU
./run.sh

# Run x86_64 in QEMU
ARCH=x86_64 ./run.sh

# Using Makefile
make run               # Run ARM64
make run-x86_64        # Run x86_64

# Or manually:
# ARM64:
qemu-system-aarch64 -M virt -cpu cortex-a72 -m 128M -nographic -kernel cantaya.bin

# x86_64:
qemu-system-x86_64 -cpu qemu64 -m 128M -nographic -kernel cantaya.bin
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
