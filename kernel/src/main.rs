//! CantayaOS Kernel binary entry point
//!
//! This is a thin wrapper that pulls in the kernel library.
//! The actual entry point (_start) is in arch/aarch64/boot.rs.

#![no_std]
#![no_main]

extern crate cantaya_kernel;
