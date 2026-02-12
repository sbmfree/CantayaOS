//! Driver Framework
//! 
//! Windows-like driver model with device objects

pub mod device;
pub mod pci;
pub mod net;
pub mod fwcfg;
pub mod framebuffer;
pub mod virtio_mmio;
pub mod virtio_input;

use alloc::vec::Vec;
use spin::Mutex;

extern crate alloc;

/// Driver entry point type
pub type DriverEntry = fn() -> DriverStatus;

#[derive(Clone, Copy, Debug)]
pub enum DriverStatus {
    Success,
    Failed,
    NotSupported,
}

/// Driver object (like DRIVER_OBJECT)
pub struct Driver {
    pub name: &'static str,
    pub major_function: [Option<fn()>; 28],
}

impl Driver {
    pub const fn new(name: &'static str) -> Self {
        Driver {
            name,
            major_function: [None; 28],
        }
    }
}

static DRIVERS: Mutex<Vec<Driver>> = Mutex::new(Vec::new());

/// Initialize driver framework
pub fn init() {
    // Initialize built-in drivers
    device::init();
    net::init();
}

/// Register a driver
pub fn register_driver(driver: Driver) {
    DRIVERS.lock().push(driver);
}
