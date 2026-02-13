// CantayaOS Shared Crate
//
// This crate provides the boot protocol types shared between the bootloader and kernel.
// It is #![no_std] so it can be compiled for both UEFI and bare-metal targets.
//
// Architecture Note:
//   In a Windows-inspired kernel, this is analogous to the LOADER_PARAMETER_BLOCK
//   that the Windows Boot Manager passes to ntoskrnl.exe. It contains all the
//   information the kernel needs to initialize itself without having to re-probe
//   hardware.

#![no_std]

pub mod boot_info;
pub mod memory;
