//! Hardware Abstraction Layer (HAL)
//! 
//! Windows-like HAL providing hardware abstraction for the kernel

pub mod console;
pub mod interrupts;
pub mod timer;
pub mod klog;
pub mod syslog;
pub mod services;

/// Initialize HAL
/// Note: console::init() is called earlier in kernel_main before exceptions::init()
/// so that UART output works from the very first print.
pub fn init() {
    interrupts::init();
    timer::init();
    syslog::init();
    // services::init() called later from kernel_main after heap is ready
}
