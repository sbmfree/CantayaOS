// CantayaOS UEFI Bootloader — Entry Point
//
// This is the main entry point for the UEFI bootloader application.
// UEFI calls this function when loading our .efi file from the ESP (EFI System Partition).
//
// Boot Sequence:
//   1. UEFI firmware loads our .efi from ESP:/EFI/BOOT/BOOTX64.EFI
//   2. We initialize logging and GOP (framebuffer)
//   3. We load the kernel ELF from ESP:/cantaya/kernel.elf
//   4. We set up 4-level page tables with higher-half kernel mapping
//   5. We collect the UEFI memory map and RSDP address
//   6. We exit UEFI boot services (point of no return — UEFI runtime services still available)
//   7. We jump to the kernel entry point, passing BootInfo
//
// Architecture Note:
//   This is analogous to the Windows Boot Manager (bootmgfw.efi) which loads
//   winload.efi, which then loads ntoskrnl.exe. We combine these roles into one
//   bootloader for simplicity.

#![no_std]
#![no_main]

extern crate alloc;

mod cmdline;
mod framebuffer;
mod loader;
mod paging;
mod splash;

use cantaya_shared::boot_info::*;
use cantaya_shared::memory::*;
use log::info;
use uefi::prelude::*;
use uefi::boot;
use uefi::mem::memory_map::{MemoryMap as _, MemoryType};

/// The kernel's higher-half base address (must match linker.ld)
const KERNEL_VADDR_BASE: u64 = 0xFFFFFFFF80000000;

/// UEFI entry point.
///
/// The `#[entry]` macro from the uefi crate sets up the UEFI environment
/// and calls this function. It provides SystemTable and Handle.
/// Raw serial write for early debugging (before UEFI init).
/// Writes a byte directly to COM1 (port 0x3F8).
unsafe fn serial_byte(b: u8) {
    // Wait for transmit buffer empty
    loop {
        let status: u8;
        core::arch::asm!("in al, dx", out("al") status, in("dx") 0x3FDu16);
        if status & 0x20 != 0 { break; }
    }
    core::arch::asm!("out dx, al", in("al") b, in("dx") 0x3F8u16);
}

/// Write a string directly to COM1
unsafe fn serial_str(s: &str) {
    for b in s.bytes() {
        serial_byte(b);
    }
    serial_byte(b'\r');
    serial_byte(b'\n');
}

/// Write raw bytes to COM1 (no newline)
unsafe fn serial_raw(s: &str) {
    for b in s.bytes() {
        serial_byte(b);
    }
}

/// Write a u64 as hexadecimal to COM1 (no newline)
unsafe fn serial_hex(val: u64) {
    let hex = b"0123456789ABCDEF";
    serial_raw("0x");
    if val == 0 { serial_byte(b'0'); return; }
    let mut started = false;
    for i in (0..16).rev() {
        let n = ((val >> (i * 4)) & 0xF) as usize;
        if n != 0 || started {
            serial_byte(hex[n]);
            started = true;
        }
    }
}

/// Write a u64 as decimal to COM1 (no newline)
unsafe fn serial_dec(val: u64) {
    if val == 0 { serial_byte(b'0'); return; }
    let mut buf = [0u8; 20];
    let mut i = 0;
    let mut v = val;
    while v > 0 {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        serial_byte(buf[i]);
    }
}

/// Read the CPU's Time Stamp Counter (64-bit cycle count)
#[inline]
unsafe fn rdtsc() -> u64 {
    let lo: u32;
    let hi: u32;
    core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nostack, nomem));
    ((hi as u64) << 32) | (lo as u64)
}

#[entry]
fn main() -> Status {
    // Early serial debug before UEFI init
    unsafe { serial_str("[BOOT] CantayaOS Bootloader entry"); }

    // Record boot start time (TSC — will calibrate below)
    let tsc_start = unsafe { rdtsc() };

    // Initialize UEFI services (console output, memory allocation, etc.)
    uefi::helpers::init().expect("Failed to initialize UEFI helpers");
    unsafe { serial_str("[BOOT] UEFI helpers initialized"); }
    info!("CantayaOS Bootloader v{}", env!("CARGO_PKG_VERSION"));

    // Disable the UEFI watchdog timer (default 5-minute reboot)
    let _ = boot::set_watchdog_timer(0, 0x10000, None);
    info!("Watchdog timer disabled");

    // Calibrate TSC using UEFI's Stall service (10 ms reference)
    boot::stall(10_000); // 10,000 µs = 10 ms
    let tsc_after_cal = unsafe { rdtsc() };
    let tsc_per_ms = if tsc_after_cal > tsc_start {
        (tsc_after_cal - tsc_start) / 10
    } else {
        1 // Fallback to avoid division by zero
    };
    info!("TSC calibrated: ~{} ticks/ms", tsc_per_ms);

    // Step 1: Set up the framebuffer via GOP
    let fb_info = framebuffer::initialize_gop();
    info!(
        "Framebuffer: {}x{} at {:#X} ({}x stride)",
        fb_info.width, fb_info.height, fb_info.address, fb_info.stride
    );

    // Step 2: Draw boot splash screen
    splash::draw_splash(&fb_info);
    splash::update_progress(&fb_info, splash::STAGE_INIT, "Initializing UEFI environment...");
    splash::update_progress(&fb_info, splash::STAGE_GOP, "Display initialized");

    // Step 3: Load the kernel ELF from the boot filesystem
    splash::update_progress(&fb_info, splash::STAGE_LOAD_KERNEL, "Loading kernel from disk...");
    unsafe { serial_str("[BOOT] Loading kernel from disk..."); }
    let kernel_data = loader::load_kernel_from_disk();
    info!("Kernel loaded: {} bytes", kernel_data.len());

    // Step 4: Parse the ELF and load segments into physical memory
    splash::update_progress(&fb_info, splash::STAGE_PARSE_ELF, "Parsing ELF executable...");
    let kernel_info = loader::parse_and_load_elf(&kernel_data);
    info!(
        "Kernel entry: {:#X}, phys={:#X}, virt={:#X}, size={:#X}, {} segments",
        kernel_info.entry_point, kernel_info.physical_base,
        kernel_info.virtual_base, kernel_info.size,
        kernel_info.segment_count
    );

    // Release the ELF data buffer (segments are already copied to physical memory)
    drop(kernel_data);

    // Step 5: Find ACPI RSDP and SMBIOS
    splash::update_progress(&fb_info, splash::STAGE_ACPI_SMBIOS, "Scanning ACPI/SMBIOS tables...");
    let rsdp_addr = find_rsdp();
    info!("RSDP at: {:#X}", rsdp_addr);

    let smbios_addr = find_smbios();
    let smbios_found = smbios_addr != 0;
    if smbios_found {
        info!("SMBIOS at: {:#X}", smbios_addr);
    } else {
        info!("No SMBIOS table found");
    }

    // Step 6: Load kernel command line from ESP
    let (cmdline_buf, cmdline_len) = cmdline::load_command_line();
    if cmdline_len > 0 {
        if let Ok(s) = core::str::from_utf8(&cmdline_buf[..cmdline_len]) {
            info!("Kernel command line: \"{}\"", s);
        }
    }

    // Step 7: Enable NX/XD bit and set up page tables
    splash::update_progress(&fb_info, splash::STAGE_PAGE_TABLES, "Setting up page tables...");
    let nx_enabled = paging::enable_nx_bit();
    let page_table_addr = paging::setup_kernel_page_tables(&kernel_info, &fb_info, nx_enabled);
    info!("Page tables created at physical: {:#X}", page_table_addr);

    // Step 8: Allocate BootInfo in a dedicated UEFI page
    // The BootInfo must live in allocated memory (not the stack) because its
    // pointer is passed to the kernel which runs long after our stack is gone.
    let boot_info_pages = (core::mem::size_of::<BootInfo>() + 4095) / 4096;
    let boot_info_phys = boot::allocate_pages(
        uefi::boot::AllocateType::AnyPages,
        MemoryType::LOADER_DATA,
        boot_info_pages,
    )
    .expect("Failed to allocate BootInfo page")
    .as_ptr() as u64;

    // Zero the allocation
    unsafe {
        core::ptr::write_bytes(boot_info_phys as *mut u8, 0, boot_info_pages * 4096);
    }

    // Fill BootInfo structure
    let boot_info = unsafe { &mut *(boot_info_phys as *mut BootInfo) };
    boot_info.magic = BOOT_INFO_MAGIC;
    boot_info.framebuffer = fb_info;
    boot_info.memory_map = cantaya_shared::boot_info::MemoryMap::empty();
    boot_info.rsdp_address = rsdp_addr;
    boot_info.kernel_physical_base = kernel_info.physical_base;
    boot_info.kernel_virtual_base = kernel_info.virtual_base;
    boot_info.kernel_size = kernel_info.size;
    boot_info.smbios_address = smbios_addr;
    boot_info.boot_time_ms = 0; // Updated after exit_boot_services
    boot_info.cpu_count = 1;    // Kernel detects from ACPI MADT later
    boot_info.command_line = cmdline_buf;
    boot_info.command_line_len = cmdline_len;

    // Set boot flags
    let mut flags = BOOT_FLAG_WATCHDOG_DISABLED;
    if nx_enabled { flags |= BOOT_FLAG_NX_ENABLED; }
    if smbios_found { flags |= BOOT_FLAG_SMBIOS_FOUND; }
    if cmdline_len > 0 { flags |= BOOT_FLAG_CMDLINE_LOADED; }
    boot_info.boot_flags = flags;

    info!(
        "BootInfo at phys {:#X} ({} pages, {} bytes, flags={:#X})",
        boot_info_phys, boot_info_pages, core::mem::size_of::<BootInfo>(), flags
    );

    // Step 9: Exit boot services — point of no return
    // After this: no UEFI boot services (allocation, console, GOP, filesystem).
    // Only serial output and direct memory access remain available.
    splash::update_progress(&fb_info, splash::STAGE_EXIT_BOOT, "Exiting boot services...");
    unsafe { serial_str("[BOOT] Exiting boot services..."); }

    let memory_map = unsafe { boot::exit_boot_services(MemoryType::LOADER_DATA) };

    unsafe { serial_str("[BOOT] Boot services exited — translating memory map"); }

    // Step 10: Translate UEFI memory map to our format
    for descriptor in memory_map.entries() {
        let kind = match descriptor.ty {
            MemoryType::CONVENTIONAL => MemoryRegionKind::Usable,
            MemoryType::BOOT_SERVICES_CODE | MemoryType::BOOT_SERVICES_DATA => {
                MemoryRegionKind::Usable // Reclaimable after ExitBootServices
            }
            MemoryType::LOADER_CODE | MemoryType::LOADER_DATA => {
                MemoryRegionKind::Bootloader // Bootloader allocations, reclaimable
            }
            MemoryType::ACPI_RECLAIM => MemoryRegionKind::AcpiReclaimable,
            MemoryType::ACPI_NON_VOLATILE => MemoryRegionKind::AcpiNvs,
            MemoryType::RUNTIME_SERVICES_CODE | MemoryType::RUNTIME_SERVICES_DATA => {
                // Track runtime services regions for future SetVirtualAddressMap
                if boot_info.uefi_runtime_map_count < 32 {
                    boot_info.uefi_runtime_regions[boot_info.uefi_runtime_map_count] =
                        MemoryRegion {
                            base: descriptor.phys_start,
                            size: descriptor.page_count * 4096,
                            kind: MemoryRegionKind::Reserved,
                        };
                    boot_info.uefi_runtime_map_count += 1;
                }
                MemoryRegionKind::Reserved
            }
            _ => MemoryRegionKind::Reserved,
        };

        let region = MemoryRegion {
            base: descriptor.phys_start,
            size: descriptor.page_count * 4096,
            kind,
        };

        if !boot_info.memory_map.add_region(region) {
            unsafe { serial_str("[BOOT] WARNING: memory map full, dropping region"); }
        }
    }

    // Step 11: Retag specialized memory regions
    // Kernel image: Bootloader → KernelAndModules
    boot_info.memory_map.retag_range(
        kernel_info.physical_base,
        kernel_info.size,
        MemoryRegionKind::KernelAndModules,
    );

    // Page table pages: Bootloader → PageTables
    let pt_pages = paging::page_table_pages();
    for &pt_addr in pt_pages {
        boot_info
            .memory_map
            .retag_range(pt_addr, 4096, MemoryRegionKind::PageTables);
    }

    // Framebuffer: identity-mapped MMIO, tag as Framebuffer
    let fb_byte_size = ((fb_info.size + 4095) / 4096) * 4096;
    boot_info.memory_map.retag_range(
        fb_info.address,
        fb_byte_size,
        MemoryRegionKind::Framebuffer,
    );

    // Step 12: Sort and merge memory map
    boot_info.memory_map.sort_regions();
    boot_info.memory_map.merge_adjacent();

    // Step 13: Calculate boot time from TSC
    let tsc_end = unsafe { rdtsc() };
    boot_info.boot_time_ms = if tsc_per_ms > 0 {
        (tsc_end - tsc_start) / tsc_per_ms
    } else {
        0
    };

    // Step 14: Log memory map summary to serial
    unsafe {
        serial_str("[BOOT] =========== Memory Map ===========");
        let regions = boot_info.memory_map.iter();
        for r in regions {
            serial_raw("  ");
            serial_hex(r.base);
            serial_raw(" - ");
            serial_hex(r.base + r.size);
            serial_raw("  (");
            serial_dec(r.size / 1024);
            serial_raw(" KiB) ");
            let kind_name = match r.kind {
                MemoryRegionKind::Usable => "Usable",
                MemoryRegionKind::Reserved => "Reserved",
                MemoryRegionKind::AcpiReclaimable => "ACPI Reclaimable",
                MemoryRegionKind::AcpiNvs => "ACPI NVS",
                MemoryRegionKind::KernelAndModules => "Kernel",
                MemoryRegionKind::Bootloader => "Bootloader",
                MemoryRegionKind::Framebuffer => "Framebuffer",
                MemoryRegionKind::PageTables => "PageTables",
            };
            serial_str(kind_name);
        }
        serial_str("[BOOT] =====================================");

        // Memory summary
        serial_raw("[BOOT] Total usable: ");
        serial_dec(boot_info.memory_map.total_usable_memory() / 1024);
        serial_str(" KiB");

        serial_raw("[BOOT] Boot time: ~");
        serial_dec(boot_info.boot_time_ms);
        serial_str(" ms");

        serial_raw("[BOOT] Regions: ");
        serial_dec(boot_info.memory_map.iter().len() as u64);
        serial_str("");

        serial_raw("[BOOT] Page tables: ");
        serial_dec(pt_pages.len() as u64);
        serial_raw(" pages (");
        serial_dec(pt_pages.len() as u64 * 4);
        serial_str(" KiB)");

        if boot_info.uefi_runtime_map_count > 0 {
            serial_raw("[BOOT] UEFI runtime regions: ");
            serial_dec(boot_info.uefi_runtime_map_count as u64);
            serial_str("");
        }
    }

    // Step 15: Jump to kernel
    // Stack: use end of kernel image (near __kernel_stack_top from linker.ld)
    // The kernel's _start immediately sets the correct RSP, so this just needs
    // to be a valid mapped address that survives the single `jmp` instruction.
    let stack_top = (KERNEL_VADDR_BASE + kernel_info.size) & !0xFu64; // 16-byte aligned

    // Final splash update (writing directly to fb memory — no UEFI needed)
    splash::update_progress(&fb_info, splash::STAGE_JUMP_KERNEL, "Starting kernel...");

    unsafe {
        serial_str("[BOOT] Jumping to kernel...");
        paging::activate_and_jump_to_kernel(
            page_table_addr,
            kernel_info.entry_point,
            boot_info_phys,
            stack_top,
        );
    }
}

/// Find the ACPI RSDP (Root System Description Pointer) from UEFI configuration tables.
///
/// The RSDP is the starting point for discovering all ACPI tables, which describe
/// hardware topology (CPUs, interrupt controllers, timers, etc.)
fn find_rsdp() -> u64 {
    use uefi::table::cfg::{ACPI2_GUID, ACPI_GUID};

    let config_entries = uefi::system::with_config_table(|entries| {
        entries.to_vec()
    });

    // Prefer ACPI 2.0+ RSDP over ACPI 1.0
    for entry in &config_entries {
        if entry.guid == ACPI2_GUID {
            return entry.address as u64;
        }
    }
    for entry in &config_entries {
        if entry.guid == ACPI_GUID {
            return entry.address as u64;
        }
    }
    0 // Not found
}

/// Find the SMBIOS entry point from UEFI configuration tables.
///
/// Prefers SMBIOS 3.0 64-bit entry point over the legacy 32-bit version.
/// The SMBIOS tables contain hardware inventory (manufacturer, model, serial, etc.)
fn find_smbios() -> u64 {
    // SMBIOS 3.0 64-bit Entry Point GUID
    const SMBIOS3_GUID: uefi::Guid = uefi::Guid::new(
        [0xf2, 0xfd, 0x15, 0x44],
        [0x97, 0x94],
        [0x4a, 0x2c],
        0x99,
        0x2e,
        [0xe5, 0xbb, 0xcf, 0x20, 0xe3, 0x94],
    );
    // Legacy SMBIOS Entry Point GUID
    const SMBIOS_GUID: uefi::Guid = uefi::Guid::new(
        [0xeb, 0x9d, 0x2d, 0x31],
        [0x2d, 0x88],
        [0x11, 0xd3],
        0x9a,
        0x16,
        [0x00, 0x90, 0x27, 0x3f, 0xc1, 0x4d],
    );

    let config_entries = uefi::system::with_config_table(|entries| {
        entries.to_vec()
    });

    // Prefer SMBIOS 3.0 64-bit
    for entry in &config_entries {
        if entry.guid == SMBIOS3_GUID {
            return entry.address as u64;
        }
    }
    // Fall back to legacy SMBIOS
    for entry in &config_entries {
        if entry.guid == SMBIOS_GUID {
            return entry.address as u64;
        }
    }
    0
}
