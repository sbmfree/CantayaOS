# Cantaya OS

Cantaya is a 64-bit operating system designed to run on UEFI firmware. This project serves as a foundation for building a modern OS with a focus on modularity and interoperability.

## Project Structure

```
CantayaOS
├── boot
│   ├── efi_entry.c       # Entry point for the UEFI application
│   └── efi.h             # UEFI types and function prototypes
├── kernel
│   ├── main.c            # Main kernel logic
│   └── kernel.h          # Kernel functions and data structures
├── drivers
│   └── driver.c          # Basic device driver implementation
├── Makefile              # Build instructions
└── README.md             # Project documentation
```

## Features

- UEFI Boot Support: Cantaya OS is designed to boot using UEFI, ensuring compatibility with modern hardware.
- Modular Design: The OS is structured into distinct components, including boot, kernel, and drivers, allowing for easier maintenance and expansion.
- Basic Device Driver: Includes a simple driver to manage hardware devices.

## Setup Instructions

1. Clone the repository:
   ```
   git clone https://github.com/yourusername/CantayaOS.git
   cd CantayaOS
   ```

2. Build the project:
   ```
   make
   ```

3. Run the OS in a UEFI-compatible environment (e.g., QEMU, VirtualBox).

## Usage Guidelines

- To contribute to the project, please fork the repository and submit a pull request with your changes.
- For any issues or feature requests, please open an issue in the GitHub repository.

## License

This project is licensed under the MIT License. See the LICENSE file for more details.