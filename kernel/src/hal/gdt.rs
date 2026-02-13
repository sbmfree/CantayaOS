// Global Descriptor Table (GDT)
//
// The GDT defines memory segments for the CPU. In long mode (64-bit), segmentation
// is mostly disabled, but the GDT is still required for:
//   1. Defining the kernel code segment (Ring 0)
//   2. Defining the user code/data segments (Ring 3)
//   3. Pointing to the TSS (Task State Segment) for stack switching on interrupts
//
// In Windows NT, the GDT is set up by the HAL during Phase 0 initialization.
// We follow the same pattern.
//
// Segment Layout:
//   0x00: Null segment (required by x86)
//   0x08: Kernel code segment (64-bit, Ring 0)
//   0x10: Kernel data segment (Ring 0)
//   0x18: User code segment (64-bit, Ring 3) — for future user mode
//   0x20: User data segment (Ring 3) — for future user mode
//   0x28: TSS segment (16 bytes, spans two GDT entries)

use core::mem::size_of;

/// GDT entry (8 bytes each, except TSS which is 16 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct GdtEntry {
    limit_low: u16,
    base_low: u16,
    base_mid: u8,
    access: u8,
    granularity: u8,
    base_high: u8,
}

impl GdtEntry {
    /// Create a null GDT entry
    const fn null() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_mid: 0,
            access: 0,
            granularity: 0,
            base_high: 0,
        }
    }

    /// Create a 64-bit code segment
    ///
    /// Access byte:
    ///   Bit 7: Present (1)
    ///   Bits 5-6: DPL (privilege level, 0=kernel, 3=user)
    ///   Bit 4: Descriptor type (1=code/data)
    ///   Bit 3: Executable (1=code)
    ///   Bit 1: Readable (1)
    const fn code_segment(dpl: u8) -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_mid: 0,
            access: 0b1001_1010 | ((dpl & 3) << 5), // Present, Code, Readable
            granularity: 0b0010_0000, // Long mode flag (bit 5)
            base_high: 0,
        }
    }

    /// Create a data segment
    const fn data_segment(dpl: u8) -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_mid: 0,
            access: 0b1001_0010 | ((dpl & 3) << 5), // Present, Data, Writable
            granularity: 0,
            base_high: 0,
        }
    }
}

/// Task State Segment (TSS)
///
/// The TSS is required in long mode for:
///   1. Stack switching when transitioning from Ring 3 to Ring 0 (system calls, interrupts)
///   2. IST (Interrupt Stack Table) — separate stacks for critical exceptions
///
/// When a user-mode process triggers an interrupt, the CPU automatically
/// loads RSP from TSS.rsp0 to switch to the kernel stack.
#[repr(C, packed)]
pub struct TaskStateSegment {
    _reserved1: u32,
    /// Stack pointers for privilege level changes (RSP0 used for Ring 3 → Ring 0)
    pub rsp: [u64; 3],
    _reserved2: u64,
    /// Interrupt Stack Table — 7 separate stacks for critical interrupts
    /// IST1 is used for double faults, IST2 for NMIs, etc.
    pub ist: [u64; 7],
    _reserved3: u64,
    _reserved4: u16,
    /// I/O Map Base Address (offset to I/O permission bitmap)
    pub iomap_base: u16,
}

impl TaskStateSegment {
    const fn new() -> Self {
        Self {
            _reserved1: 0,
            rsp: [0; 3],
            _reserved2: 0,
            ist: [0; 7],
            _reserved3: 0,
            _reserved4: 0,
            iomap_base: size_of::<TaskStateSegment>() as u16,
        }
    }
}

/// The actual GDT table (7 entries to accommodate the 16-byte TSS descriptor)
#[repr(C, align(8))]
struct Gdt {
    entries: [GdtEntry; 7],
}

/// GDT Descriptor (GDTR register value) — points to the GDT in memory
#[repr(C, packed)]
struct GdtDescriptor {
    size: u16,
    offset: u64,
}

// Static GDT and TSS — these live for the entire lifetime of the kernel
static mut GDT: Gdt = Gdt {
    entries: [
        GdtEntry::null(),          // 0x00: Null
        GdtEntry::code_segment(0), // 0x08: Kernel Code (Ring 0)
        GdtEntry::data_segment(0), // 0x10: Kernel Data (Ring 0)
        GdtEntry::code_segment(3), // 0x18: User Code (Ring 3)
        GdtEntry::data_segment(3), // 0x20: User Data (Ring 3)
        GdtEntry::null(),          // 0x28: TSS Low (filled at runtime)
        GdtEntry::null(),          // 0x30: TSS High (filled at runtime)
    ],
};

static mut TSS: TaskStateSegment = TaskStateSegment::new();

// IST stacks for critical exceptions (4 KiB each)
// These are separate stacks used by the IST mechanism to handle double faults
// and other critical exceptions safely, even if the main kernel stack is corrupted.
#[repr(C, align(4096))]
struct IstStack([u8; 4096]);

static mut DOUBLE_FAULT_STACK: IstStack = IstStack([0; 4096]);

/// Segment selectors (byte offsets into the GDT)
pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
pub const USER_CODE_SELECTOR: u16 = 0x18;
pub const USER_DATA_SELECTOR: u16 = 0x20;
pub const TSS_SELECTOR: u16 = 0x28;

/// Initialize the GDT with the TSS.
///
/// This must be called early in kernel initialization, before setting up the IDT.
pub fn init() {
    unsafe {
        // Set up TSS stack pointers
        // RSP0: kernel stack used when transitioning from user mode
        // IST1: dedicated stack for double fault handler
        TSS.ist[0] = (&DOUBLE_FAULT_STACK as *const _ as u64) + 4096; // Stack grows down

        // Create the TSS descriptor (16 bytes, spans two GDT entries)
        let tss_addr = &TSS as *const _ as u64;
        let tss_len = (size_of::<TaskStateSegment>() - 1) as u64;

        // TSS descriptor low half (GDT entry at 0x28)
        GDT.entries[5] = GdtEntry {
            limit_low: (tss_len & 0xFFFF) as u16,
            base_low: (tss_addr & 0xFFFF) as u16,
            base_mid: ((tss_addr >> 16) & 0xFF) as u8,
            access: 0b1000_1001, // Present, 64-bit TSS (Available)
            granularity: ((tss_len >> 16) & 0x0F) as u8,
            base_high: ((tss_addr >> 24) & 0xFF) as u8,
        };

        // TSS descriptor high half (GDT entry at 0x30 — upper 32 bits of base address)
        GDT.entries[6] = GdtEntry {
            limit_low: ((tss_addr >> 32) & 0xFFFF) as u16,
            base_low: ((tss_addr >> 48) & 0xFFFF) as u16,
            base_mid: 0,
            access: 0,
            granularity: 0,
            base_high: 0,
        };

        // Load the GDT
        let gdt_descriptor = GdtDescriptor {
            size: (size_of::<Gdt>() - 1) as u16,
            offset: &GDT as *const _ as u64,
        };

        core::arch::asm!(
            "lgdt [{}]",
            in(reg) &gdt_descriptor,
        );

        // Reload segment registers to use the new GDT
        // CS (code segment) requires a far jump/return
        core::arch::asm!(
            // Push new CS value and return address, then far return
            "push {cs}",
            "lea {tmp}, [rip + 2f]",
            "push {tmp}",
            "retfq",
            "2:",
            // Load data segments
            "mov ds, {ds:x}",
            "mov es, {ds:x}",
            "mov fs, {ds:x}",
            "mov gs, {ds:x}",
            "mov ss, {ds:x}",
            cs = in(reg) KERNEL_CODE_SELECTOR as u64,
            ds = in(reg) KERNEL_DATA_SELECTOR as u64,
            tmp = out(reg) _,
        );

        // Load the TSS
        core::arch::asm!(
            "ltr {tss:x}",
            tss = in(reg) TSS_SELECTOR,
        );
    }
}
