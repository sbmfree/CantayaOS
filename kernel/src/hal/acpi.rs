// ACPI Table Parser
//
// Parses the ACPI (Advanced Configuration and Power Interface) tables
// to discover system hardware configuration.
//
// ACPI table hierarchy:
//   RSDP (Root System Description Pointer) — found via UEFI or BIOS search
//     └─ RSDT/XSDT (Root/Extended System Description Table)
//         ├─ MADT (Multiple APIC Description Table) — interrupt controllers, CPUs
//         ├─ FADT (Fixed ACPI Description Table) — power management
//         ├─ HPET (High Precision Event Timer table)
//         ├─ MCFG (PCI Express configuration space)
//         └─ ... other tables
//
// We parse:
//   - RSDP → find RSDT/XSDT address
//   - RSDT/XSDT → enumerate all SDT entries
//   - MADT → count CPUs (Local APIC entries), find I/O APIC address
//
// In Windows NT, ACPI parsing is done by acpi.sys and the HAL.

use spin::Mutex;

/// Parsed ACPI information available to the rest of the kernel
pub struct AcpiInfo {
    /// Whether ACPI was successfully parsed
    pub valid: bool,
    /// ACPI revision (0 = ACPI 1.0, 2+ = ACPI 2.0+)
    pub revision: u8,
    /// Number of logical processors found in the MADT
    pub cpu_count: u8,
    /// I/O APIC base address (from MADT)
    pub ioapic_address: u64,
    /// Local APIC base address (from MADT)
    pub lapic_address: u64,
    /// HPET base address (from HPET table, 0 if not found)
    pub hpet_address: u64,
    /// PCI Express MCFG base address (0 if not found)
    pub mcfg_address: u64,
    /// OEM ID from the RSDT/XSDT
    pub oem_id: [u8; 6],
    /// Signatures of discovered SDT tables
    pub table_signatures: [[u8; 4]; 32],
    /// Number of discovered tables
    pub table_count: usize,
}

impl AcpiInfo {
    const fn new() -> Self {
        Self {
            valid: false,
            revision: 0,
            cpu_count: 0,
            ioapic_address: 0,
            lapic_address: 0,
            hpet_address: 0,
            mcfg_address: 0,
            oem_id: [0; 6],
            table_signatures: [[0; 4]; 32],
            table_count: 0,
        }
    }

    /// Get OEM ID as string
    pub fn oem_id_str(&self) -> &str {
        core::str::from_utf8(&self.oem_id).unwrap_or("??????")
    }
}

pub static ACPI_INFO: Mutex<AcpiInfo> = Mutex::new(AcpiInfo::new());

// ---------------------------------------------------------------------------
// ACPI Table Structures (repr(C, packed) to match hardware layout)
// ---------------------------------------------------------------------------

/// RSDP — Root System Description Pointer (ACPI 1.0)
#[repr(C, packed)]
struct Rsdp {
    signature: [u8; 8],     // "RSD PTR "
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,           // 0 = ACPI 1.0, 2 = ACPI 2.0+
    rsdt_address: u32,      // Physical address of RSDT
}

/// RSDP Extended (ACPI 2.0+) — extends the 1.0 RSDP
#[repr(C, packed)]
struct RsdpExtended {
    base: Rsdp,
    length: u32,
    xsdt_address: u64,      // Physical address of XSDT (64-bit)
    extended_checksum: u8,
    _reserved: [u8; 3],
}

/// Standard ACPI SDT Header — present at the start of every ACPI table
#[repr(C, packed)]
struct SdtHeader {
    signature: [u8; 4],     // e.g., "APIC", "FACP", "HPET"
    length: u32,            // Total table length including header
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

/// MADT (Multiple APIC Description Table) header
#[repr(C, packed)]
struct MadtHeader {
    header: SdtHeader,
    local_apic_address: u32,
    flags: u32,
    // Followed by variable-length interrupt controller entries
}

/// MADT entry header — each entry starts with type and length
#[repr(C, packed)]
struct MadtEntry {
    entry_type: u8,
    length: u8,
}

/// MADT entry type 0: Processor Local APIC
#[repr(C, packed)]
struct MadtLocalApic {
    header: MadtEntry,
    acpi_processor_id: u8,
    apic_id: u8,
    flags: u32,     // Bit 0: enabled, Bit 1: online capable
}

/// MADT entry type 1: I/O APIC
#[repr(C, packed)]
struct MadtIoApic {
    header: MadtEntry,
    ioapic_id: u8,
    _reserved: u8,
    ioapic_address: u32,
    global_system_interrupt_base: u32,
}

/// HPET table structure
#[repr(C, packed)]
struct HpetTable {
    header: SdtHeader,
    event_timer_block_id: u32,
    base_address_space_id: u8,
    base_register_bit_width: u8,
    base_register_bit_offset: u8,
    _reserved: u8,
    base_address: u64,
    hpet_number: u8,
    minimum_tick: u16,
    page_protection: u8,
}

/// MCFG table entry
#[repr(C, packed)]
struct McfgEntry {
    base_address: u64,
    segment_group: u16,
    start_bus: u8,
    end_bus: u8,
    _reserved: u32,
}

// ---------------------------------------------------------------------------
// ACPI Parsing
// ---------------------------------------------------------------------------

/// Validate a checksum over a byte range (sum of all bytes must be 0 mod 256)
fn validate_checksum(ptr: *const u8, len: usize) -> bool {
    let mut sum: u8 = 0;
    for i in 0..len {
        sum = sum.wrapping_add(unsafe { *ptr.add(i) });
    }
    sum == 0
}

/// Initialize ACPI by parsing tables starting from the RSDP address.
///
/// `rsdp_phys` is the physical address of the RSDP, typically from boot_info.
pub fn init(rsdp_phys: u64) {
    if rsdp_phys == 0 {
        log::warn!("ACPI: No RSDP address provided, skipping");
        return;
    }

    let rsdp = rsdp_phys as *const Rsdp;

    // Validate RSDP signature
    let sig = unsafe { (*rsdp).signature };
    if &sig != b"RSD PTR " {
        log::warn!("ACPI: Invalid RSDP signature at {:#X}", rsdp_phys);
        return;
    }

    // Validate RSDP checksum (first 20 bytes)
    if !validate_checksum(rsdp as *const u8, 20) {
        log::warn!("ACPI: RSDP checksum failed");
        return;
    }

    let revision = unsafe { (*rsdp).revision };
    let mut info = ACPI_INFO.lock();
    info.revision = revision;
    info.oem_id = unsafe { (*rsdp).oem_id };

    log::info!("ACPI: RSDP found at {:#X}, revision {}", rsdp_phys,
        if revision >= 2 { "2.0+" } else { "1.0" });

    // Get the SDT root address
    if revision >= 2 {
        let xrsdp = rsdp_phys as *const RsdpExtended;
        let xsdt_addr = unsafe { (*xrsdp).xsdt_address };
        if xsdt_addr != 0 {
            parse_xsdt(&mut info, xsdt_addr);
        } else {
            let rsdt_addr = unsafe { (*rsdp).rsdt_address } as u64;
            parse_rsdt(&mut info, rsdt_addr);
        }
    } else {
        let rsdt_addr = unsafe { (*rsdp).rsdt_address } as u64;
        parse_rsdt(&mut info, rsdt_addr);
    }

    info.valid = true;
    log::info!("ACPI: {} CPUs, I/O APIC at {:#X}, {} tables found",
        info.cpu_count, info.ioapic_address, info.table_count);
}

/// Parse the RSDT (32-bit pointers to SDTs)
fn parse_rsdt(info: &mut AcpiInfo, rsdt_phys: u64) {
    let header = rsdt_phys as *const SdtHeader;

    // Validate
    let sig = unsafe { (*header).signature };
    if &sig != b"RSDT" {
        log::warn!("ACPI: Invalid RSDT signature");
        return;
    }

    let total_len = unsafe { (*header).length } as usize;
    let header_size = core::mem::size_of::<SdtHeader>();
    let entries_len = total_len - header_size;
    let entry_count = entries_len / 4; // 32-bit pointers

    let entries_base = rsdt_phys + header_size as u64;
    for i in 0..entry_count {
        let entry_ptr = (entries_base + (i * 4) as u64) as *const u32;
        let sdt_addr = unsafe { *entry_ptr } as u64;
        if sdt_addr != 0 {
            parse_sdt(info, sdt_addr);
        }
    }
}

/// Parse the XSDT (64-bit pointers to SDTs)
fn parse_xsdt(info: &mut AcpiInfo, xsdt_phys: u64) {
    let header = xsdt_phys as *const SdtHeader;

    let sig = unsafe { (*header).signature };
    if &sig != b"XSDT" {
        log::warn!("ACPI: Invalid XSDT signature");
        return;
    }

    let total_len = unsafe { (*header).length } as usize;
    let header_size = core::mem::size_of::<SdtHeader>();
    let entries_len = total_len - header_size;
    let entry_count = entries_len / 8; // 64-bit pointers

    let entries_base = xsdt_phys + header_size as u64;
    for i in 0..entry_count {
        let entry_ptr = (entries_base + (i * 8) as u64) as *const u64;
        let sdt_addr = unsafe { core::ptr::read_unaligned(entry_ptr) };
        if sdt_addr != 0 {
            parse_sdt(info, sdt_addr);
        }
    }
}

/// Parse an individual SDT table based on its signature
fn parse_sdt(info: &mut AcpiInfo, sdt_phys: u64) {
    let header = sdt_phys as *const SdtHeader;
    let sig = unsafe { (*header).signature };

    // Record the signature
    if info.table_count < 32 {
        info.table_signatures[info.table_count] = sig;
        info.table_count += 1;
    }

    let sig_str = core::str::from_utf8(&sig).unwrap_or("????");
    log::debug!("ACPI: Found table '{}' at {:#X}", sig_str, sdt_phys);

    match &sig {
        b"APIC" => parse_madt(info, sdt_phys),
        b"HPET" => parse_hpet(info, sdt_phys),
        b"MCFG" => parse_mcfg(info, sdt_phys),
        _ => {} // Ignore tables we don't need yet
    }
}

/// Parse the MADT (Multiple APIC Description Table) to find CPUs and I/O APICs
fn parse_madt(info: &mut AcpiInfo, madt_phys: u64) {
    let madt = madt_phys as *const MadtHeader;
    let lapic_addr = unsafe { (*madt).local_apic_address };
    info.lapic_address = lapic_addr as u64;

    let total_len = unsafe { (*madt).header.length } as usize;
    let madt_header_size = core::mem::size_of::<MadtHeader>();

    let mut offset = madt_header_size;
    let base = madt_phys as usize;

    while offset + 2 <= total_len {
        let entry = (base + offset) as *const MadtEntry;
        let entry_type = unsafe { (*entry).entry_type };
        let entry_len = unsafe { (*entry).length } as usize;

        if entry_len < 2 {
            break; // Safety: avoid infinite loop on malformed data
        }

        match entry_type {
            0 => {
                // Type 0: Processor Local APIC
                let lapic = (base + offset) as *const MadtLocalApic;
                let flags = unsafe { (*lapic).flags };
                // Bit 0: Processor enabled, Bit 1: Online capable
                if flags & 0x01 != 0 || flags & 0x02 != 0 {
                    info.cpu_count += 1;
                    let apic_id = unsafe { (*lapic).apic_id };
                    log::debug!("ACPI: CPU APIC ID {} (flags={:#X})", apic_id, flags);
                }
            }
            1 => {
                // Type 1: I/O APIC
                let ioapic = (base + offset) as *const MadtIoApic;
                let addr = unsafe { (*ioapic).ioapic_address };
                info.ioapic_address = addr as u64;
                log::debug!("ACPI: I/O APIC at {:#X}", addr);
            }
            // Type 2: Interrupt Source Override (ISO) — skip for now
            // Type 3: NMI Source — skip
            // Type 4: Local APIC NMI — skip
            // Type 5: Local APIC Address Override
            5 => {
                // 64-bit override of Local APIC address
                if entry_len >= 12 {
                    let addr_ptr = (base + offset + 4) as *const u64;
                    info.lapic_address = unsafe { core::ptr::read_unaligned(addr_ptr) };
                }
            }
            // Type 9: Processor Local x2APIC
            9 => {
                if entry_len >= 16 {
                    let flags_ptr = (base + offset + 8) as *const u32;
                    let flags = unsafe { core::ptr::read_unaligned(flags_ptr) };
                    if flags & 0x01 != 0 || flags & 0x02 != 0 {
                        info.cpu_count += 1;
                    }
                }
            }
            _ => {} // Skip unknown entry types
        }

        offset += entry_len;
    }

    log::info!("ACPI: MADT parsed — {} CPUs, Local APIC at {:#X}",
        info.cpu_count, info.lapic_address);
}

/// Parse the HPET table
fn parse_hpet(info: &mut AcpiInfo, hpet_phys: u64) {
    let hpet = hpet_phys as *const HpetTable;
    let addr = unsafe { (*hpet).base_address };
    info.hpet_address = addr;
    log::info!("ACPI: HPET at {:#X}", addr);
}

/// Parse the MCFG table (PCI Express enhanced configuration)
fn parse_mcfg(info: &mut AcpiInfo, mcfg_phys: u64) {
    let header = mcfg_phys as *const SdtHeader;
    let total_len = unsafe { (*header).length } as usize;
    let header_size = core::mem::size_of::<SdtHeader>() + 8; // 8 bytes reserved

    if total_len > header_size {
        let entry = (mcfg_phys as usize + header_size) as *const McfgEntry;
        let addr = unsafe {
            let entry_ref = &*entry;
            let ptr = core::ptr::addr_of!(entry_ref.base_address);
            core::ptr::read_unaligned(ptr)
        };
        info.mcfg_address = addr;
        log::info!("ACPI: PCIe MCFG base at {:#X}", addr);
    }
}
