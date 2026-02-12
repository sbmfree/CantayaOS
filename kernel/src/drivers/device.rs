//! Device Objects

use spin::Mutex;
use alloc::collections::BTreeMap;
use alloc::string::String;

extern crate alloc;

/// Device types
#[derive(Clone, Copy, Debug)]
pub enum DeviceType {
    Unknown,
    Disk,
    Keyboard,
    Mouse,
    Display,
    Network,
    Serial,
}

/// Device object (like DEVICE_OBJECT)
pub struct Device {
    pub name: String,
    pub device_type: DeviceType,
    pub flags: u32,
    pub driver_data: usize,
}

impl Device {
    pub fn new(name: &str, device_type: DeviceType) -> Self {
        Device {
            name: String::from(name),
            device_type,
            flags: 0,
            driver_data: 0,
        }
    }
}

static DEVICES: Mutex<BTreeMap<String, Device>> = Mutex::new(BTreeMap::new());

/// Initialize device manager
pub fn init() {
    // Create null device
    create_device("\\Device\\Null", DeviceType::Unknown);
}

/// Create a device
pub fn create_device(name: &str, device_type: DeviceType) {
    let device = Device::new(name, device_type);
    DEVICES.lock().insert(String::from(name), device);
}

/// Find device by name
pub fn find_device(name: &str) -> bool {
    DEVICES.lock().contains_key(name)
}
