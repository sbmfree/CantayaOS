//! Memory Management Unit for x86_64

use core::arch::asm;
use bitflags::bitflags;

/// Page size: 4KB
pub const PAGE_SIZE: usize = 4096;
pub const PAGE_SHIFT: usize = 12;

bitflags! {
    /// Page table entry flags
    #[derive(Clone, Copy, Debug)]
    pub struct PageFlags: u64 {
        const VALID = 1 << 0;           // Present
        const WRITABLE = 1 << 1;        // Read/Write
        const USER = 1 << 2;            // User/Supervisor
        const WRITE_THROUGH = 1 << 3;   // Page-level Write-Through
        const CACHE_DISABLE = 1 << 4;   // Page-level Cache Disable
        const ACCESSED = 1 << 5;        // Accessed
        const DIRTY = 1 << 6;           // Dirty (for pages)
        const HUGE_PAGE = 1 << 7;       // Page Size (2MB/1GB pages)
        const GLOBAL = 1 << 8;          // Global
        const NO_EXECUTE = 1 << 63;     // Execute Disable
        
        // Alias for compatibility with aarch64 code
        const TABLE = 0;                // Not used on x86_64 (all entries are uniform)
        const PAGE = 0;                 // Not used on x86_64
        const READ_ONLY = 0;            // Use !WRITABLE instead
        const EXECUTE_NEVER = Self::NO_EXECUTE.bits();
        const PRIVILEGED_EXECUTE_NEVER = Self::NO_EXECUTE.bits();
        
        // Memory type aliases (simplified for x86_64)
        const ATTR_DEVICE = Self::CACHE_DISABLE.bits() | Self::WRITE_THROUGH.bits();
        const ATTR_NORMAL_NC = Self::CACHE_DISABLE.bits();
        const ATTR_NORMAL_WB = 0; // Default write-back caching
        
        // Shareability aliases (not directly applicable to x86_64)
        const INNER_SHAREABLE = 0;
        const OUTER_SHAREABLE = 0;
        const NOT_GLOBAL = 0;
    }
}

/// Initialize MMU
pub fn init() {
    setup_pat();
    // Note: CR3 must be loaded before enabling paging.
    // virtual_mem::init() handles loading CR3.
    enable_features();
}

/// Setup Page Attribute Table (PAT) for memory types
fn setup_pat() {
    // PAT[0] = WB (Write-Back)
    // PAT[1] = WT (Write-Through)
    // PAT[2] = UC- (Uncacheable)
    // PAT[3] = UC (Uncacheable)
    // PAT[4] = WB (Write-Back)
    // PAT[5] = WT (Write-Through)
    // PAT[6] = UC- (Uncacheable)
    // PAT[7] = UC (Uncacheable)
    let pat: u64 = 0x0007_0406_0007_0406;
    
    unsafe {
        // Write to IA32_PAT MSR (0x277)
        let pat_low = pat as u32;
        let pat_high = (pat >> 32) as u32;
        asm!(
            "wrmsr",
            in("ecx") 0x277u32,
            in("eax") pat_low,
            in("edx") pat_high,
        );
    }
}

/// Enable x86_64 paging features
fn enable_features() {
    unsafe {
        // Enable PAE (Physical Address Extension) - required for long mode
        let mut cr4: u64;
        asm!("mov {}, cr4", out(reg) cr4);
        cr4 |= 1 << 5;  // CR4.PAE
        asm!("mov cr4, {}", in(reg) cr4);
        
        // Enable NXE (No-Execute Enable) if supported
        // Check CPUID.80000001h:EDX.NX[bit 20]
        let mut eax: u32;
        let mut edx: u32;
        asm!(
            "push rbx",
            "mov eax, 0x80000001",
            "cpuid",
            "pop rbx",
            out("eax") eax,
            out("edx") edx,
            out("ecx") _,
        );
        
        if (edx & (1 << 20)) != 0 {
            // NX is supported, enable it in IA32_EFER
            asm!(
                "mov ecx, 0xC0000080",  // IA32_EFER MSR
                "rdmsr",
                "or eax, {nxe:e}",       // Use :e to specify 32-bit
                "wrmsr",
                nxe = in(reg) 1u32 << 11,
                out("eax") _,
                out("edx") _,
                out("ecx") _,
            );
        }
        
        // Paging is already enabled in long mode, but ensure it's on
        let mut cr0: u64;
        asm!("mov {}, cr0", out(reg) cr0);
        cr0 |= 1 << 31; // CR0.PG (Paging)
        cr0 |= 1 << 16; // CR0.WP (Write Protect)
        asm!("mov cr0, {}", in(reg) cr0);
    }
}

/// Invalidate TLB
pub fn invalidate_tlb() {
    unsafe {
        // Reload CR3 to flush entire TLB
        asm!(
            "mov {tmp}, cr3",
            "mov cr3, {tmp}",
            tmp = out(reg) _,
        );
    }
}

/// Invalidate single TLB entry
pub fn invalidate_page(addr: u64) {
    unsafe {
        asm!("invlpg [{}]", in(reg) addr, options(nostack, preserves_flags));
    }
}
