//! ELF64 Loader for AArch64
//!
//! Parses and loads ELF64 executables into memory.

use alloc::vec::Vec;
use alloc::string::String;

/// ELF64 header magic number
pub const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// ELF class (64-bit)
pub const ELFCLASS64: u8 = 2;

/// ELF data encoding (little-endian)
pub const ELFDATA2LSB: u8 = 1;

/// ELF machine type (AArch64)
pub const EM_AARCH64: u16 = 183;

/// ELF type (executable)
pub const ET_EXEC: u16 = 2;
pub const ET_DYN: u16 = 3;

/// Program header types
pub const PT_NULL: u32 = 0;
pub const PT_LOAD: u32 = 1;
pub const PT_DYNAMIC: u32 = 2;
pub const PT_INTERP: u32 = 3;
pub const PT_NOTE: u32 = 4;
pub const PT_PHDR: u32 = 6;

/// Program header flags
pub const PF_X: u32 = 1; // Execute
pub const PF_W: u32 = 2; // Write
pub const PF_R: u32 = 4; // Read

/// ELF64 file header
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Elf64Header {
    pub e_ident: [u8; 16],
    pub e_type: u16,
    pub e_machine: u16,
    pub e_version: u32,
    pub e_entry: u64,
    pub e_phoff: u64,
    pub e_shoff: u64,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
}

impl Elf64Header {
    /// Size of the header in bytes
    pub const SIZE: usize = 64;

    /// Parse ELF header from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }

        // Check magic number
        if &data[0..4] != &ELF_MAGIC {
            return None;
        }

        // Check class (64-bit)
        if data[4] != ELFCLASS64 {
            return None;
        }

        // Check endianness (little-endian)
        if data[5] != ELFDATA2LSB {
            return None;
        }

        // Parse fields
        Some(Elf64Header {
            e_ident: {
                let mut ident = [0u8; 16];
                ident.copy_from_slice(&data[0..16]);
                ident
            },
            e_type: u16::from_le_bytes([data[16], data[17]]),
            e_machine: u16::from_le_bytes([data[18], data[19]]),
            e_version: u32::from_le_bytes([data[20], data[21], data[22], data[23]]),
            e_entry: u64::from_le_bytes([
                data[24], data[25], data[26], data[27],
                data[28], data[29], data[30], data[31],
            ]),
            e_phoff: u64::from_le_bytes([
                data[32], data[33], data[34], data[35],
                data[36], data[37], data[38], data[39],
            ]),
            e_shoff: u64::from_le_bytes([
                data[40], data[41], data[42], data[43],
                data[44], data[45], data[46], data[47],
            ]),
            e_flags: u32::from_le_bytes([data[48], data[49], data[50], data[51]]),
            e_ehsize: u16::from_le_bytes([data[52], data[53]]),
            e_phentsize: u16::from_le_bytes([data[54], data[55]]),
            e_phnum: u16::from_le_bytes([data[56], data[57]]),
            e_shentsize: u16::from_le_bytes([data[58], data[59]]),
            e_shnum: u16::from_le_bytes([data[60], data[61]]),
            e_shstrndx: u16::from_le_bytes([data[62], data[63]]),
        })
    }

    /// Check if this is a valid AArch64 executable
    pub fn is_valid_aarch64_exec(&self) -> bool {
        self.e_machine == EM_AARCH64 && 
        (self.e_type == ET_EXEC || self.e_type == ET_DYN)
    }
}

/// ELF64 program header
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Elf64ProgramHeader {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

impl Elf64ProgramHeader {
    /// Size of a program header in bytes
    pub const SIZE: usize = 56;

    /// Parse program header from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }

        Some(Elf64ProgramHeader {
            p_type: u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            p_flags: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
            p_offset: u64::from_le_bytes([
                data[8], data[9], data[10], data[11],
                data[12], data[13], data[14], data[15],
            ]),
            p_vaddr: u64::from_le_bytes([
                data[16], data[17], data[18], data[19],
                data[20], data[21], data[22], data[23],
            ]),
            p_paddr: u64::from_le_bytes([
                data[24], data[25], data[26], data[27],
                data[28], data[29], data[30], data[31],
            ]),
            p_filesz: u64::from_le_bytes([
                data[32], data[33], data[34], data[35],
                data[36], data[37], data[38], data[39],
            ]),
            p_memsz: u64::from_le_bytes([
                data[40], data[41], data[42], data[43],
                data[44], data[45], data[46], data[47],
            ]),
            p_align: u64::from_le_bytes([
                data[48], data[49], data[50], data[51],
                data[52], data[53], data[54], data[55],
            ]),
        })
    }

    /// Check if this segment should be loaded
    pub fn is_loadable(&self) -> bool {
        self.p_type == PT_LOAD
    }

    /// Check if executable
    pub fn is_executable(&self) -> bool {
        (self.p_flags & PF_X) != 0
    }

    /// Check if writable
    pub fn is_writable(&self) -> bool {
        (self.p_flags & PF_W) != 0
    }

    /// Check if readable
    pub fn is_readable(&self) -> bool {
        (self.p_flags & PF_R) != 0
    }
}

/// ELF64 section header
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Elf64SectionHeader {
    pub sh_name: u32,
    pub sh_type: u32,
    pub sh_flags: u64,
    pub sh_addr: u64,
    pub sh_offset: u64,
    pub sh_size: u64,
    pub sh_link: u32,
    pub sh_info: u32,
    pub sh_addralign: u64,
    pub sh_entsize: u64,
}

impl Elf64SectionHeader {
    /// Size of a section header in bytes
    pub const SIZE: usize = 64;

    /// Parse section header from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }

        Some(Elf64SectionHeader {
            sh_name: u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            sh_type: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
            sh_flags: u64::from_le_bytes([
                data[8], data[9], data[10], data[11],
                data[12], data[13], data[14], data[15],
            ]),
            sh_addr: u64::from_le_bytes([
                data[16], data[17], data[18], data[19],
                data[20], data[21], data[22], data[23],
            ]),
            sh_offset: u64::from_le_bytes([
                data[24], data[25], data[26], data[27],
                data[28], data[29], data[30], data[31],
            ]),
            sh_size: u64::from_le_bytes([
                data[32], data[33], data[34], data[35],
                data[36], data[37], data[38], data[39],
            ]),
            sh_link: u32::from_le_bytes([data[40], data[41], data[42], data[43]]),
            sh_info: u32::from_le_bytes([data[44], data[45], data[46], data[47]]),
            sh_addralign: u64::from_le_bytes([
                data[48], data[49], data[50], data[51],
                data[52], data[53], data[54], data[55],
            ]),
            sh_entsize: u64::from_le_bytes([
                data[56], data[57], data[58], data[59],
                data[60], data[61], data[62], data[63],
            ]),
        })
    }
}

/// Parsed ELF file information
#[derive(Debug)]
pub struct ElfInfo {
    pub header: Elf64Header,
    pub program_headers: Vec<Elf64ProgramHeader>,
    pub entry_point: u64,
    pub is_executable: bool,
}

/// Error type for ELF parsing
#[derive(Debug, Clone)]
pub enum ElfError {
    TooSmall,
    BadMagic,
    Not64Bit,
    NotLittleEndian,
    NotAarch64,
    NotExecutable,
    BadProgramHeader,
    OutOfBounds,
}

impl ElfError {
    pub fn as_str(&self) -> &'static str {
        match self {
            ElfError::TooSmall => "file too small",
            ElfError::BadMagic => "invalid ELF magic",
            ElfError::Not64Bit => "not 64-bit ELF",
            ElfError::NotLittleEndian => "not little-endian",
            ElfError::NotAarch64 => "not AArch64 binary",
            ElfError::NotExecutable => "not an executable",
            ElfError::BadProgramHeader => "invalid program header",
            ElfError::OutOfBounds => "segment out of bounds",
        }
    }
}

/// Parse an ELF file from bytes
pub fn parse_elf(data: &[u8]) -> Result<ElfInfo, ElfError> {
    let header = Elf64Header::from_bytes(data).ok_or(ElfError::TooSmall)?;

    if !header.is_valid_aarch64_exec() {
        if header.e_machine != EM_AARCH64 {
            return Err(ElfError::NotAarch64);
        }
        return Err(ElfError::NotExecutable);
    }

    // Parse program headers
    let mut program_headers = Vec::new();
    let ph_start = header.e_phoff as usize;
    let ph_size = header.e_phentsize as usize;
    let ph_num = header.e_phnum as usize;

    for i in 0..ph_num {
        let offset = ph_start + i * ph_size;
        if offset + Elf64ProgramHeader::SIZE > data.len() {
            return Err(ElfError::BadProgramHeader);
        }
        if let Some(ph) = Elf64ProgramHeader::from_bytes(&data[offset..]) {
            program_headers.push(ph);
        }
    }

    Ok(ElfInfo {
        header,
        program_headers,
        entry_point: header.e_entry,
        is_executable: true,
    })
}

/// Loaded segment information
#[derive(Debug)]
pub struct LoadedSegment {
    pub vaddr: usize,
    pub size: usize,
    pub flags: u32,
}

/// Load an ELF executable into memory
/// Returns (entry_point, loaded_segments)
pub fn load_elf(data: &[u8]) -> Result<(usize, Vec<LoadedSegment>), ElfError> {
    let info = parse_elf(data)?;
    let mut loaded = Vec::new();

    for ph in &info.program_headers {
        if !ph.is_loadable() {
            continue;
        }

        let file_start = ph.p_offset as usize;
        let file_size = ph.p_filesz as usize;
        let mem_size = ph.p_memsz as usize;
        let vaddr = ph.p_vaddr as usize;

        // Check bounds
        if file_start + file_size > data.len() {
            return Err(ElfError::OutOfBounds);
        }

        // For kernel-space execution, we copy to the virtual address directly
        // In a real system, we'd allocate pages and set up mappings
        unsafe {
            let dest = vaddr as *mut u8;
            
            // Copy file contents
            core::ptr::copy_nonoverlapping(
                data.as_ptr().add(file_start),
                dest,
                file_size,
            );

            // Zero BSS (mem_size > file_size)
            if mem_size > file_size {
                core::ptr::write_bytes(
                    dest.add(file_size),
                    0,
                    mem_size - file_size,
                );
            }
        }

        loaded.push(LoadedSegment {
            vaddr,
            size: mem_size,
            flags: ph.p_flags,
        });
    }

    Ok((info.entry_point as usize, loaded))
}

/// Get a human-readable description of an ELF file
pub fn describe_elf(data: &[u8]) -> String {
    match parse_elf(data) {
        Ok(info) => {
            let mut desc = String::from("ELF64 ");
            
            match info.header.e_type {
                ET_EXEC => desc.push_str("executable"),
                ET_DYN => desc.push_str("shared object"),
                _ => desc.push_str("unknown type"),
            }

            desc.push_str(", AArch64");
            
            // Count loadable segments
            let load_count = info.program_headers.iter()
                .filter(|p| p.is_loadable())
                .count();
            
            desc.push_str(&alloc::format!(", {} loadable segments", load_count));
            desc.push_str(&alloc::format!(", entry {:#x}", info.entry_point));
            
            desc
        }
        Err(e) => alloc::format!("Not a valid ELF: {}", e.as_str()),
    }
}

/// Check if data starts with ELF magic
pub fn is_elf(data: &[u8]) -> bool {
    data.len() >= 4 && &data[0..4] == &ELF_MAGIC
}

/// User-space memory region for loaded program
pub struct UserProgram {
    pub entry_point: usize,
    pub base: usize,
    pub end: usize,
    pub stack_top: usize,
    pub argc: u64,
    pub argv: u64,
}

/// User stack size (64KB)
pub const USER_STACK_SIZE: usize = 64 * 1024;

/// User stack base address (top of user address space)
pub const USER_STACK_BASE: usize = 0x7FFF_FFF0_0000;

/// Load ELF and prepare for user-mode execution
/// Allocates pages, copies segments, sets up user stack with argc/argv.
/// `pgd_phys` is the physical address of the process's page table root
pub fn load_elf_for_user(data: &[u8], pgd_phys: usize, args: &[&str]) -> Result<UserProgram, ElfError> {
    use crate::mm::physical::{alloc_frame, alloc_contiguous_frames};
    use crate::mm::virtual_mem::map_page_in;
    use crate::arch::aarch64::mmu::{PAGE_SIZE, PageFlags};
    
    let info = parse_elf(data)?;
    
    let mut base_addr = usize::MAX;
    let mut end_addr = 0usize;
    
    // First pass: calculate memory requirements and allocation bounds
    for ph in &info.program_headers {
        if !ph.is_loadable() {
            continue;
        }
        let vaddr = ph.p_vaddr as usize;
        let mem_size = ph.p_memsz as usize;
        
        if vaddr < base_addr {
            base_addr = vaddr;
        }
        if vaddr + mem_size > end_addr {
            end_addr = vaddr + mem_size;
        }
    }
    
    if base_addr == usize::MAX {
        return Err(ElfError::NotExecutable);
    }
    
    // Second pass: allocate and map pages, copy data
    for ph in &info.program_headers {
        if !ph.is_loadable() {
            continue;
        }
        
        let vaddr = ph.p_vaddr as usize;
        let file_start = ph.p_offset as usize;
        let file_size = ph.p_filesz as usize;
        let mem_size = ph.p_memsz as usize;
        
        // Check bounds
        if file_start + file_size > data.len() {
            return Err(ElfError::OutOfBounds);
        }
        
        // Base flags for user-accessible pages
        let mut flags = PageFlags::VALID | PageFlags::PAGE | PageFlags::ACCESSED 
            | PageFlags::INNER_SHAREABLE | PageFlags::USER | PageFlags::ATTR_NORMAL_WB;
        
        if !ph.is_executable() {
            flags = flags | PageFlags::EXECUTE_NEVER;
        }
        if !ph.is_writable() {
            flags = flags | PageFlags::READ_ONLY;
        }
        
        // Allocate and map pages for this segment
        let start_page = vaddr & !(PAGE_SIZE - 1);
        let end_page = (vaddr + mem_size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let page_offset_in_seg = vaddr - start_page;
        
        // Track physical pages for data copying
        let mut pages: alloc::vec::Vec<(usize, usize)> = alloc::vec::Vec::new(); // (vaddr, paddr)
        
        for page_addr in (start_page..end_page).step_by(PAGE_SIZE) {
            if let Some(frame) = alloc_frame() {
                // Zero the physical frame directly (it's identity-mapped in kernel space)
                unsafe {
                    core::ptr::write_bytes(frame as *mut u8, 0, PAGE_SIZE);
                }
                // Map user virtual address to this frame in process page tables
                map_page_in(pgd_phys, page_addr, frame, flags);
                pages.push((page_addr, frame));
            } else {
                return Err(ElfError::OutOfBounds); // Out of memory
            }
        }
        
        // Copy segment data to physical memory directly
        // We write to the physical frames (identity-mapped) not the user virtual addresses
        let mut bytes_copied = 0usize;
        let src_ptr = unsafe { data.as_ptr().add(file_start) };
        
        for (page_vaddr, page_paddr) in &pages {
            // Calculate what portion of data goes into this page
            let page_start_in_seg = if *page_vaddr == start_page { page_offset_in_seg } else { 0 };
            let page_end_in_seg = PAGE_SIZE;
            
            // How many bytes we need to write to this page
            let _seg_offset = (*page_vaddr - start_page) + page_start_in_seg - page_offset_in_seg;
            let remaining_data = file_size.saturating_sub(bytes_copied);
            let to_copy = remaining_data.min(page_end_in_seg - page_start_in_seg);
            
            if to_copy > 0 && bytes_copied < file_size {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        src_ptr.add(bytes_copied),
                        (*page_paddr + page_start_in_seg) as *mut u8,
                        to_copy,
                    );
                }
                bytes_copied += to_copy;
            }
        }
    }
    
    // Allocate user stack
    let stack_pages = (USER_STACK_SIZE + PAGE_SIZE - 1) / PAGE_SIZE;
    let stack_base = USER_STACK_BASE - USER_STACK_SIZE;
    
    // Map stack pages with RW, no-execute permissions for user
    let stack_flags = PageFlags::VALID | PageFlags::PAGE | PageFlags::ACCESSED 
        | PageFlags::INNER_SHAREABLE | PageFlags::USER | PageFlags::EXECUTE_NEVER
        | PageFlags::ATTR_NORMAL_WB;
    
    let stack_phys = match alloc_contiguous_frames(stack_pages) {
        Some(p) => p,
        None => return Err(ElfError::OutOfBounds),
    };
    for i in 0..stack_pages {
        let vaddr = stack_base + i * PAGE_SIZE;
        let paddr = stack_phys + i * PAGE_SIZE;
        unsafe {
            core::ptr::write_bytes(paddr as *mut u8, 0, PAGE_SIZE);
        }
        map_page_in(pgd_phys, vaddr, paddr, stack_flags);
    }

    // --- Write argc/argv onto the user stack ---
    // Layout (growing downward from USER_STACK_BASE):
    //   <string data area>   (arg strings, null-terminated)
    //   <8-byte align pad>
    //   NULL                 (argv[argc])
    //   argv[argc-1] ptr     (user VA pointer to string)
    //   ...
    //   argv[0] ptr
    //   argc (u64)
    //   <- SP points here
    let argc = args.len();

    // We write to the *physical* frames (identity-mapped in kernel VA).
    // User VA = stack_base + offset => phys = stack_phys + offset
    let stack_top_phys = stack_phys + USER_STACK_SIZE;

    // 1. Write string data from the top downward
    let mut cursor = stack_top_phys; // physical write cursor, descends
    let mut arg_user_ptrs: alloc::vec::Vec<u64> = alloc::vec::Vec::new();
    for arg in args.iter() {
        let bytes = arg.as_bytes();
        let needed = bytes.len() + 1; // include NUL
        cursor -= needed;
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), cursor as *mut u8, bytes.len());
            *((cursor + bytes.len()) as *mut u8) = 0; // NUL terminator
        }
        // Corresponding user-VA pointer for this string
        let user_va = USER_STACK_BASE - (stack_top_phys - cursor);
        arg_user_ptrs.push(user_va as u64);
    }

    // 2. Align cursor down to 8 bytes
    cursor &= !7;

    // 3. Write NULL terminator for argv array
    cursor -= 8;
    unsafe { *(cursor as *mut u64) = 0; }

    // 4. Write argv pointers in reverse order (argv[argc-1] .. argv[0])
    for i in (0..argc).rev() {
        cursor -= 8;
        unsafe { *(cursor as *mut u64) = arg_user_ptrs[i]; }
    }
    let argv_user_va = USER_STACK_BASE - (stack_top_phys - cursor);

    // 5. Write argc
    cursor -= 8;
    unsafe { *(cursor as *mut u64) = argc as u64; }

    // Align to 16 bytes (AArch64 SP alignment requirement)
    cursor &= !15;

    let final_sp = USER_STACK_BASE - (stack_top_phys - cursor);

    Ok(UserProgram {
        entry_point: info.entry_point as usize,
        base: base_addr,
        end: end_addr,
        stack_top: final_sp,
        argc: argc as u64,
        argv: argv_user_va as u64,
    })
}

