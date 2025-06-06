# CantayaOS

CantayaOS is an open-source operating system designed to be fully self-coded from the ground up. The goal of CantayaOS is to provide a unique and modern OS experience while supporting Windows applications and other advanced features.

## Features
- **Open Source**: Built with transparency and collaboration in mind.
- **Self-Coded**: Every component of the OS is written from scratch to ensure full control and understanding of the system.
- **Windows Application Support**: Planned compatibility with Windows apps to provide a seamless user experience.
- **UEFI Boot**: Fully supports modern UEFI systems for fast and secure booting.
- **x86_64 Architecture**: Designed for 64-bit systems, ensuring compatibility with modern hardware.

## Goals
CantayaOS aims to:
1. Be a fully functional and self-coded operating system.
2. Support Windows applications natively or through compatibility layers.
3. Provide a robust and efficient kernel and user-space environment.
4. Encourage contributions from the open-source community.

## Getting Started
### Prerequisites
- A cross-compiler for `x86_64` (e.g., `x86_64-elf-gcc`).
- GNU-EFI libraries for UEFI development.
- QEMU or real hardware for testing.

### Building CantayaOS
1. Clone the repository:
   ```bash
   git clone https://github.com/sbmfree/CantayaOS.git
   cd CantayaOS
