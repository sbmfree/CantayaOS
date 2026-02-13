// Memory Region Types
//
// This module defines the memory region types that are shared between
// the bootloader and kernel. The bootloader translates UEFI memory types
// into these more general categories that the kernel understands.
//
// This abstraction is important because:
//   1. UEFI has ~15 different memory types, but the kernel only cares about ~5 categories
//   2. It decouples the kernel from UEFI specifics (future: could support other boot protocols)
//   3. It matches how Windows categorizes memory via MEMORY_BASIC_INFORMATION

/// A contiguous region of physical memory with a specific type.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    /// Physical start address of this region (always page-aligned, 4 KiB)
    pub base: u64,
    /// Size of this region in bytes
    pub size: u64,
    /// What kind of memory this region contains
    pub kind: MemoryRegionKind,
}

impl MemoryRegion {
    /// Create an empty/invalid memory region (used for array initialization)
    pub const fn empty() -> Self {
        Self {
            base: 0,
            size: 0,
            kind: MemoryRegionKind::Reserved,
        }
    }

    /// The end address (exclusive) of this region
    pub const fn end(&self) -> u64 {
        self.base + self.size
    }

    /// Number of 4 KiB pages in this region
    pub const fn page_count(&self) -> u64 {
        self.size / 4096
    }
}

/// Classification of physical memory regions.
///
/// The bootloader translates UEFI memory types into these categories:
///   - EfiConventionalMemory → Usable
///   - EfiBootServicesCode/Data → Usable (after ExitBootServices)
///   - EfiACPIReclaimMemory → AcpiReclaimable
///   - EfiACPIMemoryNVS → AcpiNvs  
///   - EfiReservedMemoryType → Reserved
///   - Regions used by kernel/bootloader → KernelAndModules / Bootloader
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryRegionKind {
    /// Free memory that the kernel can use for anything
    Usable = 0,
    /// Reserved by firmware — must not be touched
    Reserved = 1,
    /// ACPI tables that can be reclaimed after parsing
    AcpiReclaimable = 2,
    /// ACPI Non-Volatile Storage — must not be touched
    AcpiNvs = 3,
    /// Memory containing the kernel image and its modules
    KernelAndModules = 4,
    /// Memory used by the bootloader (can be reclaimed after kernel init)
    Bootloader = 5,
    /// Framebuffer memory — must not be used for general allocation
    Framebuffer = 6,
    /// Page table memory created by the bootloader — must not be overwritten
    PageTables = 7,
}
