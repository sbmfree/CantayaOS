// Boot Information Structure
//
// This module defines the BootInfo structure that the UEFI bootloader passes to the kernel.
// It contains everything the kernel needs for initialization:
//   - Framebuffer info for display output
//   - Memory map from UEFI (which regions are usable, reserved, etc.)
//   - Physical address where the kernel was loaded
//   - RSDP pointer for ACPI table access
//
// Design Decision:
//   We use a flat, repr(C) struct instead of a pointer-heavy tree structure.
//   This ensures the data survives the transition from UEFI's memory model
//   to the kernel's own paging setup without dangling pointers.

use crate::memory::{MemoryRegion, MemoryRegionKind};

/// Maximum number of memory regions we support from the UEFI memory map.
/// Most systems have 30-60 regions; 512 gives us ample headroom.
pub const MAX_MEMORY_REGIONS: usize = 512;

/// Boot information passed from the UEFI bootloader to the kernel.
///
/// This is the single point of truth for all hardware/firmware information
/// that the kernel needs. The bootloader fills this in before jumping to
/// the kernel entry point.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BootInfo {
    /// Magic number to verify the structure is valid (guards against corruption)
    pub magic: u64,

    /// Framebuffer information for early display output
    pub framebuffer: FramebufferInfo,

    /// UEFI memory map translated into our own format
    pub memory_map: MemoryMap,

    /// Physical address of the ACPI RSDP (Root System Description Pointer).
    /// Used to find ACPI tables for hardware enumeration.
    /// 0 means RSDP was not found.
    pub rsdp_address: u64,

    /// Physical address where the kernel ELF was loaded
    pub kernel_physical_base: u64,

    /// Virtual address of the kernel (should match linker script)
    pub kernel_virtual_base: u64,

    /// Size of the kernel image in bytes
    pub kernel_size: u64,

    // --- New fields (v2) ---

    /// Physical address of the SMBIOS entry point table.
    /// 0 means SMBIOS was not found.
    pub smbios_address: u64,

    /// Boot time in milliseconds (approximate, from TSC calibration)
    pub boot_time_ms: u64,

    /// Number of logical processors detected (0 or 1 = single-core)
    pub cpu_count: u32,

    /// Boot flags (see BOOT_FLAG_* constants)
    pub boot_flags: u32,

    /// Kernel command line loaded from \cantaya\cmdline.txt on the ESP.
    /// NUL-terminated when command_line_len < 256.
    pub command_line: [u8; 256],

    /// Length of the command line (not including NUL terminator)
    pub command_line_len: usize,

    /// UEFI runtime services memory regions.
    /// The kernel needs these to call SetVirtualAddressMap if it wants to
    /// use UEFI runtime services (RTC, NVRAM variables, etc.)
    pub uefi_runtime_regions: [MemoryRegion; 32],

    /// Number of valid entries in uefi_runtime_regions
    pub uefi_runtime_map_count: usize,
}

/// Magic value to identify a valid BootInfo structure.
/// "CNTY" in ASCII, zero-extended to 64 bits.
pub const BOOT_INFO_MAGIC: u64 = 0x43_4E_54_59_4F_53_00_00; // "CNTYOS\0\0"

/// Boot flags bitmask constants
pub const BOOT_FLAG_NX_ENABLED: u32 = 1 << 0;      // NX/XD bit active in page tables
pub const BOOT_FLAG_SMBIOS_FOUND: u32 = 1 << 1;    // SMBIOS entry point was discovered
pub const BOOT_FLAG_CMDLINE_LOADED: u32 = 1 << 2;  // Command line was loaded from ESP
pub const BOOT_FLAG_WATCHDOG_DISABLED: u32 = 1 << 3; // UEFI watchdog timer was disabled

impl BootInfo {
    /// Verify that this BootInfo structure is valid
    pub fn is_valid(&self) -> bool {
        self.magic == BOOT_INFO_MAGIC
    }

    /// Get the kernel command line as a string slice.
    pub fn command_line_str(&self) -> &str {
        core::str::from_utf8(&self.command_line[..self.command_line_len]).unwrap_or("")
    }
}

/// Framebuffer information from UEFI GOP (Graphics Output Protocol).
///
/// The framebuffer is a region of memory that directly maps to pixels on screen.
/// This is the simplest way to display graphics — just write pixel values to memory.
/// No GPU driver needed.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FramebufferInfo {
    /// Physical address of the framebuffer memory region
    pub address: u64,
    /// Total size of the framebuffer in bytes
    pub size: u64,
    /// Horizontal resolution in pixels
    pub width: u32,
    /// Vertical resolution in pixels
    pub height: u32,
    /// Number of bytes per horizontal line (may include padding beyond width)
    pub stride: u32,
    /// Pixel format used by the framebuffer
    pub pixel_format: PixelFormat,
}

/// Pixel format of the framebuffer.
///
/// UEFI GOP typically provides BGR or RGB format.
/// We need to know this to write the correct byte order for colors.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// Each pixel is 4 bytes: [Blue, Green, Red, Reserved]
    Bgr = 0,
    /// Each pixel is 4 bytes: [Red, Green, Blue, Reserved]
    Rgb = 1,
    /// Unknown format — fallback, may produce wrong colors
    Unknown = 2,
}

/// The UEFI memory map, translated into CantayaOS's own format.
///
/// We copy the UEFI memory map into our own structure before exiting boot services,
/// because the original UEFI memory map becomes invalid after ExitBootServices().
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryMap {
    /// The actual memory regions
    pub regions: [MemoryRegion; MAX_MEMORY_REGIONS],
    /// Number of valid entries in the regions array
    pub region_count: usize,
}

impl MemoryMap {
    /// Create an empty memory map
    pub const fn empty() -> Self {
        Self {
            regions: [MemoryRegion::empty(); MAX_MEMORY_REGIONS],
            region_count: 0,
        }
    }

    /// Add a memory region to the map. Returns false if the map is full.
    pub fn add_region(&mut self, region: MemoryRegion) -> bool {
        if self.region_count >= MAX_MEMORY_REGIONS {
            return false;
        }
        self.regions[self.region_count] = region;
        self.region_count += 1;
        true
    }

    /// Iterate over valid memory regions
    pub fn iter(&self) -> &[MemoryRegion] {
        &self.regions[..self.region_count]
    }

    /// Calculate total usable (conventional) memory in bytes
    pub fn total_usable_memory(&self) -> u64 {
        self.iter()
            .iter()
            .filter(|r| r.kind == crate::memory::MemoryRegionKind::Usable)
            .map(|r| r.size)
            .sum()
    }

    /// Sort memory regions by base address (insertion sort — no alloc needed).
    pub fn sort_regions(&mut self) {
        for i in 1..self.region_count {
            let key = self.regions[i];
            let mut j = i;
            while j > 0 && self.regions[j - 1].base > key.base {
                self.regions[j] = self.regions[j - 1];
                j -= 1;
            }
            self.regions[j] = key;
        }
    }

    /// Merge adjacent regions of the same kind to reduce fragmentation.
    /// Must be called after sort_regions().
    pub fn merge_adjacent(&mut self) {
        if self.region_count < 2 {
            return;
        }
        let mut write = 0;
        for read in 1..self.region_count {
            if self.regions[write].kind == self.regions[read].kind
                && self.regions[write].end() == self.regions[read].base
            {
                // Merge: extend the write region to cover the read region
                self.regions[write].size += self.regions[read].size;
            } else {
                write += 1;
                self.regions[write] = self.regions[read];
            }
        }
        self.region_count = write + 1;
    }

    /// Re-tag all pages in [range_base, range_base+range_size) with `new_kind`.
    ///
    /// Regions overlapping the range are split as needed. Non-overlapping
    /// regions are kept unchanged. This is used to mark kernel, framebuffer,
    /// and page-table memory with their correct types after the initial
    /// UEFI memory map translation.
    pub fn retag_range(&mut self, range_base: u64, range_size: u64, new_kind: MemoryRegionKind) {
        if range_size == 0 {
            return;
        }
        let range_end = range_base + range_size;

        // Build a new region list in-place by scanning the old one.
        // We use a second fixed-size buffer to avoid aliasing issues.
        let mut new_regions = [MemoryRegion::empty(); MAX_MEMORY_REGIONS];
        let mut new_count: usize = 0;

        for i in 0..self.region_count {
            let r = self.regions[i];
            let r_end = r.base + r.size;

            if r.base >= range_end || r_end <= range_base {
                // No overlap — keep as-is
                if new_count < MAX_MEMORY_REGIONS {
                    new_regions[new_count] = r;
                    new_count += 1;
                }
            } else {
                // Overlap — potentially split into up to 3 sub-regions

                // 1. Portion before the retag range
                if r.base < range_base && new_count < MAX_MEMORY_REGIONS {
                    new_regions[new_count] = MemoryRegion {
                        base: r.base,
                        size: range_base - r.base,
                        kind: r.kind,
                    };
                    new_count += 1;
                }

                // 2. Overlapping portion with new kind
                let ovl_start = if r.base > range_base { r.base } else { range_base };
                let ovl_end = if r_end < range_end { r_end } else { range_end };
                if new_count < MAX_MEMORY_REGIONS {
                    new_regions[new_count] = MemoryRegion {
                        base: ovl_start,
                        size: ovl_end - ovl_start,
                        kind: new_kind,
                    };
                    new_count += 1;
                }

                // 3. Portion after the retag range
                if r_end > range_end && new_count < MAX_MEMORY_REGIONS {
                    new_regions[new_count] = MemoryRegion {
                        base: range_end,
                        size: r_end - range_end,
                        kind: r.kind,
                    };
                    new_count += 1;
                }
            }
        }

        self.regions = new_regions;
        self.region_count = new_count;
    }
}
