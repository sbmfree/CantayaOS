//! QEMU fw-cfg MMIO driver
//!
//! The fw-cfg interface provides access to firmware configuration data.
//! On the QEMU `virt` machine it is memory-mapped at 0x0902_0000.
//!
//! Register layout (ARM virt machine, MMIO):
//!   +0x00  Data     (1-byte read/write to currently selected file)
//!   +0x08  Selector (16-bit write, selects a fw-cfg file, DEVICE_BIG_ENDIAN)
//!   +0x10  DMA      (64-bit write, triggers DMA transfer, DEVICE_BIG_ENDIAN)
//!
//! Reads use the DMA interface. Writes use PIO (selector + data register)
//! to avoid memory-ordering issues with DMA descriptors on AArch64.

use core::ptr;

const FWCFG_BASE: usize = 0x0902_0000;

// Register addresses (separate MMIO regions on ARM virt)
#[allow(dead_code)]
const FWCFG_DATA: usize     = FWCFG_BASE;         // data register (1-byte r/w)
#[allow(dead_code)]
const FWCFG_SELECTOR: usize = FWCFG_BASE + 0x08;   // selector register (u16 BE write)

// Register offsets
const FWCFG_DMA: usize = 0x10;

// Selector keys
const FW_CFG_FILE_DIR: u16 = 0x0019;

// DMA control bits (big-endian in the struct)
#[allow(dead_code)]
const FW_CFG_DMA_CTL_ERROR: u32  = 1 << 0;
const FW_CFG_DMA_CTL_READ: u32   = 1 << 1;
#[allow(dead_code)]
const FW_CFG_DMA_CTL_SKIP: u32   = 1 << 2;
const FW_CFG_DMA_CTL_SELECT: u32 = 1 << 3;
#[allow(dead_code)]
const FW_CFG_DMA_CTL_WRITE: u32  = 1 << 4;

/// DMA access descriptor — must be 4-byte aligned, fields big-endian.
#[repr(C, align(4))]
struct FwCfgDmaAccess {
    control: u32,
    length: u32,
    address: u64,
}

/// Perform a DMA **read**: select `sel`, read `len` bytes into `buf_phys`.
unsafe fn dma_read(sel: u16, buf_phys: usize, len: usize) {
    let mut desc = FwCfgDmaAccess {
        control: 0,
        length:  0,
        address: 0,
    };

    // Write fields via volatile to guarantee they reach memory
    let base = &mut desc as *mut FwCfgDmaAccess as *mut u8;
    let control = ((sel as u32) << 16) | FW_CFG_DMA_CTL_SELECT | FW_CFG_DMA_CTL_READ;
    ptr::write_volatile(base.add(0) as *mut u32, control.to_be());
    ptr::write_volatile(base.add(4) as *mut u32, (len as u32).to_be());
    ptr::write_volatile(base.add(8) as *mut u64, (buf_phys as u64).to_be());

    let desc_phys = base as usize;

    // DMB ST: ensure descriptor stores are visible before the MMIO write
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!("dmb st", options(nostack, preserves_flags));
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("mfence", options(nostack, preserves_flags));

    let hi = ((desc_phys as u64) >> 32) as u32;
    let lo = desc_phys as u32;
    ptr::write_volatile((FWCFG_BASE + FWCFG_DMA) as *mut u32, hi.to_be());
    ptr::write_volatile((FWCFG_BASE + FWCFG_DMA + 4) as *mut u32, lo.to_be());

    // Poll until the device clears the control field
    let ctrl_ptr = base as *mut u32;
    let mut timeout = 10_000_000u32;
    while ptr::read_volatile(ctrl_ptr) != 0 {
        timeout -= 1;
        if timeout == 0 { break; }
        core::hint::spin_loop();
    }
}

/// Perform a DMA **continuation read** (no SELECT, reads from current offset).
unsafe fn dma_read_continue(buf_phys: usize, len: usize) {
    let mut desc = FwCfgDmaAccess {
        control: 0,
        length:  0,
        address: 0,
    };

    let base = &mut desc as *mut FwCfgDmaAccess as *mut u8;
    ptr::write_volatile(base.add(0) as *mut u32, FW_CFG_DMA_CTL_READ.to_be());
    ptr::write_volatile(base.add(4) as *mut u32, (len as u32).to_be());
    ptr::write_volatile(base.add(8) as *mut u64, (buf_phys as u64).to_be());

    let desc_phys = base as usize;

    #[cfg(target_arch = "aarch64")]
    core::arch::asm!("dmb st", options(nostack, preserves_flags));
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("mfence", options(nostack, preserves_flags));

    let hi = ((desc_phys as u64) >> 32) as u32;
    let lo = desc_phys as u32;
    ptr::write_volatile((FWCFG_BASE + FWCFG_DMA) as *mut u32, hi.to_be());
    ptr::write_volatile((FWCFG_BASE + FWCFG_DMA + 4) as *mut u32, lo.to_be());

    let ctrl_ptr = base as *mut u32;
    let mut timeout = 10_000_000u32;
    while ptr::read_volatile(ctrl_ptr) != 0 {
        timeout -= 1;
        if timeout == 0 { break; }
        core::hint::spin_loop();
    }
}

/// Skip `len` bytes in the currently selected fw-cfg file.
#[allow(dead_code)]
unsafe fn dma_skip(len: usize) {
    let mut desc = FwCfgDmaAccess {
        control: 0,
        length:  0,
        address: 0,
    };

    let base = &mut desc as *mut FwCfgDmaAccess as *mut u8;
    ptr::write_volatile(base.add(0) as *mut u32, FW_CFG_DMA_CTL_SKIP.to_be());
    ptr::write_volatile(base.add(4) as *mut u32, (len as u32).to_be());
    ptr::write_volatile(base.add(8) as *mut u64, 0u64);

    let desc_phys = base as usize;

    #[cfg(target_arch = "aarch64")]
    core::arch::asm!("dmb st", options(nostack, preserves_flags));
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("mfence", options(nostack, preserves_flags));

    let hi = ((desc_phys as u64) >> 32) as u32;
    let lo = desc_phys as u32;
    ptr::write_volatile((FWCFG_BASE + FWCFG_DMA) as *mut u32, hi.to_be());
    ptr::write_volatile((FWCFG_BASE + FWCFG_DMA + 4) as *mut u32, lo.to_be());

    let ctrl_ptr = base as *mut u32;
    let mut timeout = 10_000_000u32;
    while ptr::read_volatile(ctrl_ptr) != 0 {
        timeout -= 1;
        if timeout == 0 { break; }
        core::hint::spin_loop();
    }
}

/// fw-cfg file directory entry (big-endian on-disk format)
#[repr(C)]
#[derive(Clone, Copy)]
struct FwCfgFileEntry {
    size:    u32,        // file size in bytes (BE)
    select:  u16,        // selector key (BE)
    _reserved: u16,
    name:    [u8; 56],   // NUL-terminated name
}

/// Find a fw-cfg file by name. Returns (selector, size) on success.
pub fn find_file(name: &str) -> Option<(u16, u32)> {
    unsafe {
        // Read the file count (first 4 bytes of the file directory)
        let mut count_be: u32 = 0;
        dma_read(
            FW_CFG_FILE_DIR,
            &mut count_be as *mut u32 as usize,
            4,
        );
        let count = u32::from_be(count_be);

        // Read entries one at a time
        let mut entry = core::mem::MaybeUninit::<FwCfgFileEntry>::uninit();
        let entry_size = core::mem::size_of::<FwCfgFileEntry>();

        for _ in 0..count {
            dma_read_continue(
                entry.as_mut_ptr() as usize,
                entry_size,
            );

            let e = entry.assume_init();
            let ename = {
                let len = e.name.iter().position(|&b| b == 0).unwrap_or(56);
                core::str::from_utf8_unchecked(&e.name[..len])
            };
            if ename == name {
                return Some((u16::from_be(e.select), u32::from_be(e.size)));
            }
        }
    }
    None
}

/// Perform a DMA **write**: select `sel`, write `len` bytes from `buf_phys` to the fw-cfg file.
///
/// QEMU's ramfb (and other writable fw-cfg files) only invoke their write
/// callbacks via the DMA path — PIO data-register writes copy bytes to the
/// buffer but never trigger the callback.
unsafe fn dma_write(sel: u16, buf_phys: usize, len: usize) {
    let mut desc = FwCfgDmaAccess {
        control: 0,
        length:  0,
        address: 0,
    };

    let base = &mut desc as *mut FwCfgDmaAccess as *mut u8;
    let control = ((sel as u32) << 16) | FW_CFG_DMA_CTL_SELECT | FW_CFG_DMA_CTL_WRITE;
    ptr::write_volatile(base.add(0) as *mut u32, control.to_be());
    ptr::write_volatile(base.add(4) as *mut u32, (len as u32).to_be());
    ptr::write_volatile(base.add(8) as *mut u64, (buf_phys as u64).to_be());

    let desc_phys = base as usize;

    // DMB ST: ensure descriptor + data stores are visible before MMIO write
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!("dmb st", options(nostack, preserves_flags));
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("mfence", options(nostack, preserves_flags));

    let hi = ((desc_phys as u64) >> 32) as u32;
    let lo = desc_phys as u32;
    ptr::write_volatile((FWCFG_BASE + FWCFG_DMA) as *mut u32, hi.to_be());
    ptr::write_volatile((FWCFG_BASE + FWCFG_DMA + 4) as *mut u32, lo.to_be());

    // Poll until the device clears the control field
    let ctrl_ptr = base as *mut u32;
    let mut timeout = 10_000_000u32;
    while ptr::read_volatile(ctrl_ptr) != 0 {
        timeout -= 1;
        if timeout == 0 { break; }
        core::hint::spin_loop();
    }
}

/// Write data to a fw-cfg file via DMA.
///
/// Must use DMA (not PIO) because QEMU only invokes the file's write
/// callback (e.g. ramfb display setup) from the DMA transfer path.
pub fn write_file(selector: u16, data: &[u8]) {
    unsafe {
        dma_write(selector, data.as_ptr() as usize, data.len());
    }
}

/// Read data from a fw-cfg file identified by its selector key (via DMA).
pub fn read_file(selector: u16, buf: &mut [u8]) {
    unsafe {
        dma_read(selector, buf.as_mut_ptr() as usize, buf.len());
    }
}
