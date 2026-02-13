// PCI Bus Subsystem
//
// Provides PCI Configuration Space access and device enumeration
// using the legacy Configuration Mechanism #1 (I/O ports 0xCF8/0xCFC).
//
// PCI Configuration Space Layout (per function):
//   0x00: Vendor ID (16) | Device ID (16)
//   0x04: Command (16) | Status (16)
//   0x08: Revision (8) | Prog IF (8) | Subclass (8) | Class (8)
//   0x0C: Cache Line (8) | Latency (8) | Header Type (8) | BIST (8)
//   0x10-0x24: BAR0-BAR5 (Base Address Registers)
//   0x2C: Subsystem Vendor ID (16) | Subsystem ID (16)
//   0x3C: Interrupt Line (8) | Interrupt Pin (8) | Min Grant (8) | Max Latency (8)
//
// In Windows NT, PCI enumeration is done by pci.sys (PCI Bus Driver).

extern crate alloc;

use alloc::vec::Vec;
use spin::Mutex;

/// Maximum number of PCI devices we track
const MAX_PCI_DEVICES: usize = 64;

/// A discovered PCI device
#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub revision: u8,
    pub header_type: u8,
    pub interrupt_line: u8,
    pub interrupt_pin: u8,
    /// Base Address Registers (up to 6 for type 0 headers)
    pub bars: [u32; 6],
    /// Subsystem IDs
    pub subsystem_vendor_id: u16,
    pub subsystem_id: u16,
}

impl PciDevice {
    const fn empty() -> Self {
        Self {
            bus: 0, device: 0, function: 0,
            vendor_id: 0, device_id: 0,
            class_code: 0, subclass: 0, prog_if: 0, revision: 0,
            header_type: 0, interrupt_line: 0, interrupt_pin: 0,
            bars: [0; 6],
            subsystem_vendor_id: 0, subsystem_id: 0,
        }
    }

    /// Get human-readable class name
    pub fn class_name(&self) -> &'static str {
        pci_class_name(self.class_code, self.subclass)
    }

    /// Check if a BAR is MMIO (memory-mapped) vs I/O port
    pub fn bar_is_mmio(&self, bar_idx: usize) -> bool {
        bar_idx < 6 && self.bars[bar_idx] & 1 == 0
    }

    /// Get the base address from a BAR (masking type bits)
    pub fn bar_address(&self, bar_idx: usize) -> u64 {
        if bar_idx >= 6 { return 0; }
        let bar = self.bars[bar_idx];
        if bar & 1 != 0 {
            // I/O BAR — mask bottom 2 bits
            (bar & !0x3) as u64
        } else {
            // Memory BAR
            let bar_type = (bar >> 1) & 0x3;
            if bar_type == 2 && bar_idx < 5 {
                // 64-bit BAR — combine with next BAR
                let low = (bar & !0xF) as u64;
                let high = self.bars[bar_idx + 1] as u64;
                low | (high << 32)
            } else {
                (bar & !0xF) as u64
            }
        }
    }
}

/// Global PCI device list
struct PciSubsystem {
    devices: [PciDevice; MAX_PCI_DEVICES],
    count: usize,
    enumerated: bool,
}

impl PciSubsystem {
    const fn new() -> Self {
        Self {
            devices: [PciDevice::empty(); MAX_PCI_DEVICES],
            count: 0,
            enumerated: false,
        }
    }
}

static PCI_SUBSYSTEM: Mutex<PciSubsystem> = Mutex::new(PciSubsystem::new());

// ---------------------------------------------------------------------------
// PCI Configuration Space I/O
// ---------------------------------------------------------------------------

/// Read a 32-bit value from PCI configuration space (Mechanism #1)
pub fn config_read32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let address: u32 = 0x8000_0000
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC);

    unsafe {
        crate::hal::port::outd(0xCF8, address);
        crate::hal::port::ind(0xCFC)
    }
}

/// Write a 32-bit value to PCI configuration space
pub fn config_write32(bus: u8, device: u8, function: u8, offset: u8, value: u32) {
    let address: u32 = 0x8000_0000
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC);

    unsafe {
        crate::hal::port::outd(0xCF8, address);
        crate::hal::port::outd(0xCFC, value);
    }
}

/// Read a 16-bit value from PCI configuration space
pub fn config_read16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let val32 = config_read32(bus, device, function, offset & 0xFC);
    ((val32 >> ((offset & 2) * 8)) & 0xFFFF) as u16
}

/// Read an 8-bit value from PCI configuration space
pub fn config_read8(bus: u8, device: u8, function: u8, offset: u8) -> u8 {
    let val32 = config_read32(bus, device, function, offset & 0xFC);
    ((val32 >> ((offset & 3) * 8)) & 0xFF) as u8
}

// ---------------------------------------------------------------------------
// PCI Device Enumeration
// ---------------------------------------------------------------------------

/// Scan all PCI buses for devices.
/// Populates the global device list. Safe to call multiple times (rescans).
pub fn enumerate() {
    let mut pci = PCI_SUBSYSTEM.lock();
    pci.count = 0;

    for bus in 0u16..=255 {
        let mut found_on_bus = false;

        for device in 0u8..32 {
            let vendor_device = config_read32(bus as u8, device, 0, 0x00);
            let vendor_id = (vendor_device & 0xFFFF) as u16;

            if vendor_id == 0xFFFF || vendor_id == 0x0000 {
                continue;
            }
            found_on_bus = true;

            let header_type = config_read8(bus as u8, device, 0, 0x0E);
            let max_func = if header_type & 0x80 != 0 { 8u8 } else { 1u8 };

            for function in 0..max_func {
                let vd = if function == 0 {
                    vendor_device
                } else {
                    config_read32(bus as u8, device, function, 0x00)
                };

                let vid = (vd & 0xFFFF) as u16;
                if vid == 0xFFFF || vid == 0x0000 { continue; }
                let did = ((vd >> 16) & 0xFFFF) as u16;

                let class_reg = config_read32(bus as u8, device, function, 0x08);
                let class_code = ((class_reg >> 24) & 0xFF) as u8;
                let subclass = ((class_reg >> 16) & 0xFF) as u8;
                let prog_if = ((class_reg >> 8) & 0xFF) as u8;
                let revision = (class_reg & 0xFF) as u8;

                let int_reg = config_read32(bus as u8, device, function, 0x3C);
                let irq_line = (int_reg & 0xFF) as u8;
                let irq_pin = ((int_reg >> 8) & 0xFF) as u8;

                let subsys = config_read32(bus as u8, device, function, 0x2C);
                let subsys_vid = (subsys & 0xFFFF) as u16;
                let subsys_id = ((subsys >> 16) & 0xFFFF) as u16;

                // Read BARs (only for type 0 headers)
                let mut bars = [0u32; 6];
                let ht = header_type & 0x7F;
                let bar_count = if ht == 0 { 6 } else if ht == 1 { 2 } else { 0 };
                for b in 0..bar_count {
                    bars[b] = config_read32(bus as u8, device, function, 0x10 + (b as u8) * 4);
                }

                let idx = pci.count;
                if idx < MAX_PCI_DEVICES {
                    pci.devices[idx] = PciDevice {
                        bus: bus as u8,
                        device,
                        function,
                        vendor_id: vid,
                        device_id: did,
                        class_code,
                        subclass,
                        prog_if,
                        revision,
                        header_type: ht,
                        interrupt_line: irq_line,
                        interrupt_pin: irq_pin,
                        bars,
                        subsystem_vendor_id: subsys_vid,
                        subsystem_id: subsys_id,
                    };
                    pci.count += 1;
                }
            }
        }

        // Stop scanning if we're past bus 0 and this bus is empty
        if bus > 1 && !found_on_bus && pci.count > 0 {
            break;
        }
        // Quick exit: if bus 0 has nothing, don't scan all 256
        if bus == 0 && !found_on_bus {
            break;
        }
    }

    pci.enumerated = true;
    log::info!("PCI: {} device(s) found", pci.count);
}

/// Get a snapshot of all discovered PCI devices.
pub fn device_list() -> Vec<PciDevice> {
    let pci = PCI_SUBSYSTEM.lock();
    let mut list = Vec::with_capacity(pci.count);
    for i in 0..pci.count {
        list.push(pci.devices[i]);
    }
    list
}

/// Get the number of discovered devices
pub fn device_count() -> usize {
    PCI_SUBSYSTEM.lock().count
}

/// Whether the PCI bus has been enumerated
pub fn is_enumerated() -> bool {
    PCI_SUBSYSTEM.lock().enumerated
}

// ---------------------------------------------------------------------------
// PCI Class Code Database
// ---------------------------------------------------------------------------

/// Get a human-readable name for a PCI class/subclass combination
pub fn pci_class_name(class: u8, subclass: u8) -> &'static str {
    match (class, subclass) {
        (0x00, 0x00) => "Non-VGA Unclassified",
        (0x00, 0x01) => "VGA-Compatible Unclassified",
        (0x00, _) => "Unclassified",
        (0x01, 0x00) => "SCSI Controller",
        (0x01, 0x01) => "IDE Controller",
        (0x01, 0x02) => "Floppy Controller",
        (0x01, 0x04) => "RAID Controller",
        (0x01, 0x05) => "ATA Controller",
        (0x01, 0x06) => "SATA Controller",
        (0x01, 0x07) => "SAS Controller",
        (0x01, 0x08) => "NVMe Controller",
        (0x01, _) => "Storage Controller",
        (0x02, 0x00) => "Ethernet Controller",
        (0x02, 0x01) => "Token Ring",
        (0x02, 0x02) => "FDDI",
        (0x02, 0x03) => "ATM Controller",
        (0x02, 0x80) => "Other Network Controller",
        (0x02, _) => "Network Controller",
        (0x03, 0x00) => "VGA Controller",
        (0x03, 0x01) => "XGA Controller",
        (0x03, _) => "Display Controller",
        (0x04, 0x00) => "Video Device",
        (0x04, 0x01) => "Audio Device",
        (0x04, 0x03) => "Audio Device (HD Audio)",
        (0x04, _) => "Multimedia Device",
        (0x05, 0x00) => "RAM Controller",
        (0x05, 0x01) => "Flash Controller",
        (0x05, _) => "Memory Controller",
        (0x06, 0x00) => "Host Bridge",
        (0x06, 0x01) => "ISA Bridge",
        (0x06, 0x02) => "EISA Bridge",
        (0x06, 0x03) => "MCA Bridge",
        (0x06, 0x04) => "PCI-to-PCI Bridge",
        (0x06, 0x05) => "PCMCIA Bridge",
        (0x06, 0x80) => "Other Bridge",
        (0x06, _) => "Bridge Device",
        (0x07, 0x00) => "Serial Controller",
        (0x07, 0x01) => "Parallel Controller",
        (0x07, _) => "Communication Controller",
        (0x08, 0x00) => "PIC",
        (0x08, 0x01) => "DMA Controller",
        (0x08, 0x02) => "System Timer",
        (0x08, 0x03) => "RTC Controller",
        (0x08, _) => "System Peripheral",
        (0x09, 0x00) => "Keyboard Controller",
        (0x09, 0x01) => "Digitizer",
        (0x09, 0x02) => "Mouse Controller",
        (0x09, _) => "Input Device",
        (0x0A, _) => "Docking Station",
        (0x0B, _) => "Processor",
        (0x0C, 0x00) => "FireWire Controller",
        (0x0C, 0x03) => "USB Controller",
        (0x0C, 0x05) => "SMBus Controller",
        (0x0C, _) => "Serial Bus Controller",
        (0x0D, 0x00) => "iRDA Controller",
        (0x0D, 0x11) => "Bluetooth Controller",
        (0x0D, 0x20) => "WiFi Controller (802.11a)",
        (0x0D, 0x21) => "WiFi Controller (802.11b)",
        (0x0D, _) => "Wireless Controller",
        (0x0E, _) => "Intelligent I/O Controller",
        (0x0F, _) => "Satellite Controller",
        (0x10, _) => "Encryption Controller",
        (0x11, _) => "Signal Processing Controller",
        (0x12, _) => "Processing Accelerator",
        (0xFF, _) => "Vendor Specific",
        _ => "Unknown",
    }
}
