// Kernel ELF Loader
//
// This module handles loading the kernel from the UEFI filesystem and parsing
// it as a 64-bit ELF executable.
//
// Boot Flow:
//   1. Open the boot filesystem (the ESP — EFI System Partition)
//   2. Read the kernel file from a known path (\cantaya\kernel.elf)
//   3. Parse the ELF headers to find loadable segments (PT_LOAD)
//   4. Allocate physical memory and copy segments there
//   5. Return the entry point address and load information
//
// Design Decision:
//   We implement a minimal ELF parser instead of depending on a crate because:
//   - We only need PT_LOAD segments from a 64-bit ELF
//   - It avoids pulling in a large dependency for a simple task
//   - Full control over memory allocation during loading

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use log::info;
use uefi::boot;
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode, RegularFile};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::cstr16;

/// Information about a single ELF PT_LOAD segment.
///
/// Tracks the segment's position within the kernel image and its
/// permission flags (PF_R, PF_W, PF_X) for page-table mapping.
#[derive(Debug, Clone, Copy)]
pub struct SegmentInfo {
    /// Offset from the kernel's virtual base to this segment
    pub offset_from_base: u64,
    /// Size of this segment in memory (p_memsz)
    pub size: u64,
    /// ELF p_flags: PF_X=1, PF_W=2, PF_R=4
    pub flags: u32,
}

/// Information about the loaded kernel, returned after ELF parsing
#[derive(Debug)]
pub struct KernelLoadInfo {
    /// Virtual address of the kernel entry point (_start)
    pub entry_point: u64,
    /// Physical base address where kernel was loaded
    pub physical_base: u64,
    /// Virtual base address (from ELF, should match linker script)
    pub virtual_base: u64,
    /// Total size of loaded kernel in memory
    pub size: u64,
    /// Per-segment information for page-permission mapping
    pub segments: [SegmentInfo; 8],
    /// Number of valid entries in `segments`
    pub segment_count: usize,
}

/// Load the kernel binary from the UEFI boot filesystem.
///
/// The kernel is stored on the EFI System Partition at \cantaya\kernel.elf.
/// We read the entire file into a Vec<u8> for ELF parsing.
pub fn load_kernel_from_disk() -> Vec<u8> {
    // Open the Simple File System protocol on the boot device
    let fs_handle = boot::get_handle_for_protocol::<SimpleFileSystem>()
        .expect("No filesystem available on boot device");

    let mut fs = boot::open_protocol_exclusive::<SimpleFileSystem>(fs_handle)
        .expect("Failed to open filesystem protocol");

    // Open the root directory of the ESP
    let mut root = fs.open_volume().expect("Failed to open ESP volume");

    // Open the kernel file
    let kernel_path = cstr16!("\\cantaya\\kernel.elf");
    let file_handle = root
        .open(kernel_path, FileMode::Read, FileAttribute::empty())
        .expect("Failed to open \\cantaya\\kernel.elf — is the kernel on the ESP?");

    let mut kernel_file: RegularFile = file_handle
        .into_regular_file()
        .expect("kernel.elf is not a regular file");

    // Get file size
    let mut info_buf = vec![0u8; 256];
    let file_info = kernel_file
        .get_info::<FileInfo>(&mut info_buf)
        .expect("Failed to get kernel file info");
    let file_size = file_info.file_size() as usize;

    info!("Kernel file size: {} bytes", file_size);

    // Read the entire file
    let mut kernel_data = vec![0u8; file_size];
    kernel_file
        .read(&mut kernel_data)
        .expect("Failed to read kernel.elf");

    kernel_data
}

// ============================================================================
// Minimal ELF64 Parser
//
// We only parse what we need: the ELF header and PT_LOAD program headers.
// This is a subset of the full ELF specification, sufficient for loading
// a statically-linked kernel executable.
// ============================================================================

// ELF64 constants
const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const ELF_CLASS_64: u8 = 2;
const ELF_DATA_LSB: u8 = 1; // Little-endian
const ELF_TYPE_EXEC: u16 = 2; // Executable
const ELF_TYPE_DYN: u16 = 3;  // Shared object (PIE executable)
const PT_LOAD: u32 = 1; // Loadable segment
const EM_X86_64: u16 = 0x3E;  // AMD64 / x86-64 architecture

// ELF segment permission flags (from p_flags)
pub const PF_X: u32 = 0x1; // Executable
pub const PF_W: u32 = 0x2; // Writable
pub const PF_R: u32 = 0x4; // Readable

/// ELF64 File Header (first 64 bytes of the file)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct Elf64Header {
    e_ident: [u8; 16],     // Magic number and other info
    e_type: u16,           // Object file type
    e_machine: u16,        // Architecture
    e_version: u32,        // Object file version
    e_entry: u64,          // Entry point virtual address
    e_phoff: u64,          // Program header table file offset
    e_shoff: u64,          // Section header table file offset
    e_flags: u32,          // Processor-specific flags
    e_ehsize: u16,         // ELF header size
    e_phentsize: u16,      // Program header table entry size
    e_phnum: u16,          // Program header table entry count
    e_shentsize: u16,      // Section header table entry size
    e_shnum: u16,          // Section header table entry count
    e_shstrndx: u16,       // Section name string table index
}

/// ELF64 Program Header (describes a segment to load)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct Elf64ProgramHeader {
    p_type: u32,    // Segment type
    p_flags: u32,   // Segment flags
    p_offset: u64,  // Segment file offset
    p_vaddr: u64,   // Segment virtual address
    p_paddr: u64,   // Segment physical address
    p_filesz: u64,  // Segment size in file
    p_memsz: u64,   // Segment size in memory
    p_align: u64,   // Segment alignment
}

/// Parse the kernel ELF and load PT_LOAD segments into physical memory.
///
/// This function:
///   1. Validates the ELF header (magic, class, endianness, type)
///   2. Iterates over program headers to find PT_LOAD segments
///   3. Allocates physical memory via UEFI's allocate_pages
///   4. Copies segment data from the ELF file to physical memory
///   5. Returns the entry point and load information
pub fn parse_and_load_elf(elf_data: &[u8]) -> KernelLoadInfo {
    // Safety: We verify the magic number and size before trusting the header
    assert!(
        elf_data.len() >= core::mem::size_of::<Elf64Header>(),
        "ELF file too small for header"
    );

    let header: &Elf64Header = unsafe { &*(elf_data.as_ptr() as *const Elf64Header) };

    // Validate ELF magic
    assert_eq!(
        header.e_ident[0..4],
        ELF_MAGIC,
        "Not a valid ELF file (bad magic)"
    );
    assert_eq!(header.e_ident[4], ELF_CLASS_64, "Not a 64-bit ELF");
    assert_eq!(header.e_ident[5], ELF_DATA_LSB, "Not little-endian ELF");
    let e_type = header.e_type;
    assert!(
        e_type == ELF_TYPE_EXEC || e_type == ELF_TYPE_DYN,
        "Not an executable ELF (type={})", e_type
    );

    // Validate architecture (must be x86-64)
    let e_machine = header.e_machine;
    assert_eq!(
        e_machine, EM_X86_64,
        "ELF is not x86-64 (e_machine={:#X}, expected {:#X})",
        e_machine, EM_X86_64
    );

    let entry_point = header.e_entry;
    let ph_offset = header.e_phoff as usize;
    let ph_count = header.e_phnum as usize;
    let ph_size = header.e_phentsize as usize;

    info!("ELF: {} program headers, entry at {:#X}", ph_count, entry_point);

    let mut virtual_base: u64 = u64::MAX;
    let mut total_end: u64 = 0;

    // First pass: determine total memory range needed
    for i in 0..ph_count {
        let ph_addr = ph_offset + i * ph_size;
        assert!(ph_addr + ph_size <= elf_data.len(), "Program header out of bounds");

        let ph: &Elf64ProgramHeader =
            unsafe { &*(elf_data.as_ptr().add(ph_addr) as *const Elf64ProgramHeader) };

        if ph.p_type != PT_LOAD {
            continue;
        }

        let vaddr = ph.p_vaddr;
        let memsz = ph.p_memsz;

        if vaddr < virtual_base {
            virtual_base = vaddr;
        }
        if vaddr + memsz > total_end {
            total_end = vaddr + memsz;
        }
    }

    let total_size = total_end - virtual_base;
    let pages_needed = (total_size + 4095) / 4096;

    info!(
        "Kernel needs {} pages ({} KiB) at virtual {:#X}",
        pages_needed,
        pages_needed * 4,
        virtual_base
    );

    // Allocate physical memory for the kernel using UEFI
    let phys_addr = boot::allocate_pages(
        uefi::boot::AllocateType::AnyPages,
        uefi::mem::memory_map::MemoryType::LOADER_DATA,
        pages_needed as usize,
    )
    .expect("Failed to allocate memory for kernel")
    .as_ptr() as u64;

    let physical_base = phys_addr;

    // Zero the allocated memory (important for BSS segments)
    unsafe {
        core::ptr::write_bytes(phys_addr as *mut u8, 0, (pages_needed * 4096) as usize);
    }

    // Second pass: load PT_LOAD segments and collect per-segment info
    let mut segments = [SegmentInfo { offset_from_base: 0, size: 0, flags: 0 }; 8];
    let mut segment_count: usize = 0;

    for i in 0..ph_count {
        let ph_addr = ph_offset + i * ph_size;
        let ph: &Elf64ProgramHeader =
            unsafe { &*(elf_data.as_ptr().add(ph_addr) as *const Elf64ProgramHeader) };

        if ph.p_type != PT_LOAD {
            continue;
        }

        let offset_in_kernel = ph.p_vaddr - virtual_base;
        let dest = (physical_base + offset_in_kernel) as *mut u8;

        let seg_vaddr = ph.p_vaddr;
        let seg_offset = ph.p_offset;
        let seg_filesz = ph.p_filesz;
        let seg_memsz = ph.p_memsz;
        let seg_flags = ph.p_flags;

        // Bounds check: verify file data is within the ELF buffer
        let file_end = (seg_offset + seg_filesz) as usize;
        assert!(
            file_end <= elf_data.len(),
            "ELF segment file data out of bounds: offset={:#X} filesz={:#X} elf_size={:#X}",
            seg_offset, seg_filesz, elf_data.len()
        );

        let src = &elf_data[seg_offset as usize..file_end];

        let flag_str = [
            if seg_flags & PF_R != 0 { 'R' } else { '-' },
            if seg_flags & PF_W != 0 { 'W' } else { '-' },
            if seg_flags & PF_X != 0 { 'X' } else { '-' },
        ];
        info!(
            "  Loading segment: vaddr={:#X} offset={:#X} filesz={:#X} memsz={:#X} flags={}{}{}",
            seg_vaddr, seg_offset, seg_filesz, seg_memsz,
            flag_str[0], flag_str[1], flag_str[2]
        );

        unsafe {
            core::ptr::copy_nonoverlapping(src.as_ptr(), dest, src.len());
        }

        // Record segment info for page-permission mapping
        if segment_count < 8 {
            segments[segment_count] = SegmentInfo {
                offset_from_base: offset_in_kernel,
                size: seg_memsz,
                flags: seg_flags,
            };
            segment_count += 1;
        }
    }

    KernelLoadInfo {
        entry_point,
        physical_base,
        virtual_base,
        size: total_size,
        segments,
        segment_count,
    }
}
