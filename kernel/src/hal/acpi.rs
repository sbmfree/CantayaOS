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
    /// FADT: PM1a control block I/O port
    pub pm1a_control_block: u32,
    /// FADT: PM1b control block I/O port (0 if not present)
    pub pm1b_control_block: u32,
    /// FADT: SLP_TYPa value for S5 (shutdown)
    pub slp_typa_s5: u16,
    /// FADT: SLP_TYPb value for S5 (shutdown)
    pub slp_typb_s5: u16,
    /// Whether S5 (soft-off) is supported
    pub s5_supported: bool,
    /// DSDT address from FADT
    pub dsdt_address: u64,
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
            pm1a_control_block: 0,
            pm1b_control_block: 0,
            slp_typa_s5: 0,
            slp_typb_s5: 0,
            s5_supported: false,
            dsdt_address: 0,
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
        b"FACP" => parse_fadt(info, sdt_phys),
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

/// Parse the FADT (Fixed ACPI Description Table) for power management
fn parse_fadt(info: &mut AcpiInfo, fadt_phys: u64) {
    let base = fadt_phys as usize;
    let header = fadt_phys as *const SdtHeader;
    let total_len = unsafe { (*header).length } as usize;

    // FADT offsets (bytes from start):
    //   36: DSDT address (4 bytes, 32-bit)
    //   64: PM1a_CNT_BLK (4 bytes)
    //   68: PM1b_CNT_BLK (4 bytes)
    //  140: X_DSDT (8 bytes, 64-bit, ACPI 2.0+)

    if total_len < 72 {
        log::warn!("ACPI: FADT too short ({} bytes)", total_len);
        return;
    }

    unsafe {
        // DSDT address (32-bit)
        let dsdt32 = core::ptr::read_unaligned((base + 36) as *const u32) as u64;

        // PM1a/PM1b control blocks
        info.pm1a_control_block = core::ptr::read_unaligned((base + 64) as *const u32);
        info.pm1b_control_block = core::ptr::read_unaligned((base + 68) as *const u32);

        // Try 64-bit DSDT (ACPI 2.0+, offset 140)
        if total_len >= 148 {
            let dsdt64 = core::ptr::read_unaligned((base + 140) as *const u64);
            info.dsdt_address = if dsdt64 != 0 { dsdt64 } else { dsdt32 };
        } else {
            info.dsdt_address = dsdt32;
        }

        log::info!("ACPI: FADT — PM1a={:#X} PM1b={:#X} DSDT={:#X}",
            info.pm1a_control_block, info.pm1b_control_block, info.dsdt_address);

        // Try to find S5 sleep type values from the DSDT
        if info.dsdt_address != 0 {
            parse_s5_from_dsdt(info);
        }
    }
}

/// Search the DSDT for the \_S5 object to get SLP_TYP values for shutdown.
/// The AML bytecode pattern is: '_S5_' followed by a package containing the values.
fn parse_s5_from_dsdt(info: &mut AcpiInfo) {
    let dsdt = info.dsdt_address as *const SdtHeader;
    let total_len = unsafe { (*dsdt).length } as usize;
    let base = info.dsdt_address as usize;
    let header_size = core::mem::size_of::<SdtHeader>();

    if total_len <= header_size || total_len > 0x100000 {
        // Use QEMU default S5 values
        info.slp_typa_s5 = 5;
        info.slp_typb_s5 = 0;
        info.s5_supported = true;
        return;
    }

    let aml_start = base + header_size;
    let aml_len = total_len - header_size;
    let aml = unsafe { core::slice::from_raw_parts(aml_start as *const u8, aml_len) };

    // Search for "_S5_" in the AML bytecode
    let pattern = b"_S5_";
    let mut found = false;

    for i in 0..aml_len.saturating_sub(20) {
        if aml[i..i + 4] == *pattern {
            // Found _S5_ — look for package opcode (0x12)
            let mut j = i + 4;
            // Skip name and DefScope bytes
            while j < aml_len && j < i + 20 {
                if aml[j] == 0x12 {
                    // Package opcode found
                    j += 1; // skip package opcode
                    // PkgLength (simplified: single byte for small packages)
                    if j < aml_len {
                        j += 1; // skip PkgLength
                    }
                    if j < aml_len {
                        j += 1; // skip NumElements
                    }
                    // Read SLP_TYPa
                    if j < aml_len {
                        if aml[j] == 0x0A {
                            // BytePrefix
                            j += 1;
                            if j < aml_len {
                                info.slp_typa_s5 = aml[j] as u16;
                                j += 1;
                            }
                        } else {
                            info.slp_typa_s5 = aml[j] as u16;
                            j += 1;
                        }
                    }
                    // Read SLP_TYPb
                    if j < aml_len {
                        if aml[j] == 0x0A {
                            j += 1;
                            if j < aml_len {
                                info.slp_typb_s5 = aml[j] as u16;
                            }
                        } else {
                            info.slp_typb_s5 = aml[j] as u16;
                        }
                    }
                    info.s5_supported = true;
                    found = true;
                    break;
                }
                j += 1;
            }
            if found { break; }
        }
    }

    if !found {
        // Fallback: use typical values for QEMU/Bochs
        info.slp_typa_s5 = 5;
        info.slp_typb_s5 = 0;
        info.s5_supported = true;
    }

    log::info!("ACPI: S5 shutdown — SLP_TYPa={} SLP_TYPb={} supported={}",
        info.slp_typa_s5, info.slp_typb_s5, info.s5_supported);
}

/// Perform an ACPI S5 soft-off shutdown.
/// This writes the SLP_TYP and SLP_EN bits to the PM1 control registers.
pub fn acpi_shutdown() {
    let info = ACPI_INFO.lock();

    if !info.s5_supported || info.pm1a_control_block == 0 {
        log::warn!("ACPI: S5 shutdown not available, using fallback");
        drop(info);
        // Fallback: QEMU-specific shutdown port
        unsafe {
            super::port::outw(0x604, 0x2000);
            loop { core::arch::asm!("cli; hlt"); }
        }
    }

    let pm1a = info.pm1a_control_block as u16;
    let pm1b = info.pm1b_control_block as u16;
    let slp_typa = info.slp_typa_s5;
    let slp_typb = info.slp_typb_s5;
    drop(info);

    // SLP_EN = bit 13, SLP_TYP in bits 10-12
    let val_a = (slp_typa << 10) | (1 << 13);
    let val_b = (slp_typb << 10) | (1 << 13);

    unsafe {
        super::port::outw(pm1a, val_a);
        if pm1b != 0 {
            super::port::outw(pm1b, val_b);
        }
        // If ACPI shutdown didn't work, try QEMU port
        super::port::outw(0x604, 0x2000);
        loop { core::arch::asm!("cli; hlt"); }
    }
}

