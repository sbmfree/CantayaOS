//! Interrupt handling (GICv3)

use crate::sync::IrqMutex;

/// GICv3 base addresses (QEMU virt)
const GICD_BASE: usize = 0x0800_0000;  // Distributor
const GICR_BASE: usize = 0x080A_0000;  // Redistributor RD base
// GICR layout per CPU: RD_base (64KB) + SGI_base (64KB)
#[allow(dead_code)]
const GICR_SGI_SIZE: usize = 0x10000;  // 64KB

/// Redistributor RD (base) frame registers
const GICR_WAKER: usize = 0x0014;

/// Redistributor SGI frame registers (offset from RD base)
const GICR_SGI_OFFSET: usize = 0x10000; // SGI frame is at +64KB
const GICR_ISENABLER0: usize = 0x0100;  // Enable SGIs/PPIs
const GICR_IPRIORITYR: usize = 0x0400;  // Priority for SGIs/PPIs

/// Distributor registers
const GICD_CTLR: usize = 0x0000;
#[allow(dead_code)]
const GICD_TYPER: usize = 0x0004;
const GICD_IGROUPR: usize = 0x0080;
const GICD_ISENABLER: usize = 0x0100;
const GICD_ICENABLER: usize = 0x0180;
#[allow(dead_code)]
const GICD_IPRIORITYR: usize = 0x0400;
#[allow(dead_code)]
const GICD_ITARGETSR: usize = 0x0800;
#[allow(dead_code)]
const GICD_ICFGR: usize = 0x0C00;

/// CPU interface (system registers)

static GIC: IrqMutex<Gic> = IrqMutex::new(Gic::new());

pub struct Gic {
    gicd_base: usize,
    #[allow(dead_code)]
    gicr_base: usize,
}

impl Gic {
    const fn new() -> Self {
        Gic {
            gicd_base: GICD_BASE,
            gicr_base: GICR_BASE,
        }
    }
    
    unsafe fn write_gicd(&self, offset: usize, val: u32) {
        core::ptr::write_volatile((self.gicd_base + offset) as *mut u32, val);
    }
    
    #[allow(dead_code)]
    unsafe fn read_gicd(&self, offset: usize) -> u32 {
        core::ptr::read_volatile((self.gicd_base + offset) as *const u32)
    }
}

/// Initialize GIC
pub fn init() {
    let gic = GIC.lock();
    
    unsafe {
        // Wake up the Redistributor (required for GICv3)
        // Clear ProcessorSleep bit and wait for ChildrenAsleep to clear
        let waker_addr = GICR_BASE + GICR_WAKER;
        let waker_val = core::ptr::read_volatile(waker_addr as *const u32);
        core::ptr::write_volatile(waker_addr as *mut u32, waker_val & !2); // Clear ProcessorSleep
        
        // Wait for ChildrenAsleep to clear (with timeout)
        let mut timeout = 10000;
        loop {
            let waker = core::ptr::read_volatile(waker_addr as *const u32);
            if (waker & 4) == 0 { 
                break; 
            }
            timeout -= 1;
            if timeout == 0 {
                break;
            }
        }
        
        // Enable distributor
        // For GICv3, set ARE_NS (bit 4) and EnableGrp1NS (bit 1)
        gic.write_gicd(GICD_CTLR, 0x12); // ARE_NS + EnableGrp1NS
        
        // Set priority mask (allow all priorities) - set to minimum filtering (0xFF = allow all)
        core::arch::asm!("msr ICC_PMR_EL1, {}", in(reg) 0xFFu64);
        
        // Configure ICC_CTLR_EL1 for proper operation
        // Enable EOI mode 0 (write to ICC_EOIR1_EL1 drops priority and deactivates)
        core::arch::asm!("msr ICC_CTLR_EL1, {}", in(reg) 0u64);
        
        // Enable System Register Interface (should already be enabled by firmware)
        // ICC_SRE_EL1.SRE bit
        let sre: u64;
        core::arch::asm!("mrs {}, ICC_SRE_EL1", out(reg) sre);
        if (sre & 1) == 0 {
            core::arch::asm!("msr ICC_SRE_EL1, {}", in(reg) sre | 1);
        }
        
        // Enable group 1 NS interrupts (this is the key enable!)
        core::arch::asm!("msr ICC_IGRPEN1_EL1, {}", in(reg) 1u64);
        
        // Configure UART0 IRQ (SPI #1 = IRQ 33)
        // Set priority
        let pri_reg = GICD_IPRIORITYR + (33 / 4) * 4;
        let pri_shift = (33 % 4) * 8;
        let mut pri_val = gic.read_gicd(pri_reg);
        pri_val &= !(0xFF << pri_shift);
        pri_val |= 0xA0 << pri_shift; // Priority 0xA0
        gic.write_gicd(pri_reg, pri_val);
        
        // Set target to CPU 0 (for GICv2 compat)
        let tgt_reg = GICD_ITARGETSR + (33 / 4) * 4;
        let tgt_shift = (33 % 4) * 8;
        let mut tgt_val = gic.read_gicd(tgt_reg);
        tgt_val |= 0x01 << tgt_shift;
        gic.write_gicd(tgt_reg, tgt_val);

        // Set UART SPI to Group 1 NS (GICD_IGROUPR)
        let grp_reg = GICD_IGROUPR + (33 / 32) * 4;
        let grp_bit = 33 % 32;
        let grp_val = gic.read_gicd(grp_reg);
        gic.write_gicd(grp_reg, grp_val | (1 << grp_bit));
    }
    
    // Enable UART0 IRQ
    drop(gic);
    enable_irq(33);
}

/// Configure and enable an SPI interrupt with the given priority.
/// Handles Group1 NS assignment, priority register, and enable.
pub fn configure_spi(intid: u32, priority: u8) {
    let gic = GIC.lock();
    unsafe {
        // Set this SPI to Group 1 NS so it is delivered as IRQ, not FIQ
        let grp_reg = GICD_IGROUPR + (intid as usize / 32) * 4;
        let grp_bit = intid % 32;
        let grp_val = gic.read_gicd(grp_reg);
        gic.write_gicd(grp_reg, grp_val | (1 << grp_bit));

        // Set priority
        let pri_reg = GICD_IPRIORITYR + (intid as usize / 4) * 4;
        let pri_shift = ((intid % 4) * 8) as u32;
        let mut pri_val = gic.read_gicd(pri_reg);
        pri_val &= !(0xFF << pri_shift);
        pri_val |= (priority as u32) << pri_shift;
        gic.write_gicd(pri_reg, pri_val);

        // IROUTER defaults to 0 (CPU 0) in GICv3 with ARE, which is correct.
        // ITARGETSR is RES0 in GICv3 ARE mode, so skip it.
    }
    drop(gic);
    enable_irq(intid);
}

/// Enable specific interrupt
pub fn enable_irq(irq: u32) {
    if irq < 32 {
        // SGI/PPI: use Redistributor SGI frame (offset +64KB from RD base)
        let sgi_base = GICR_BASE + GICR_SGI_OFFSET;
        unsafe {
            // Configure as Group 1 NS (GICR_IGROUPR0)
            let group_reg = sgi_base + 0x0080; // GICR_IGROUPR0
            let current_group = core::ptr::read_volatile(group_reg as *const u32);
            core::ptr::write_volatile(group_reg as *mut u32, current_group | (1 << irq));
            
            // Set priority for this PPI (byte-accessible) - use lower value for higher priority
            let pri_reg = sgi_base + GICR_IPRIORITYR + (irq as usize);
            core::ptr::write_volatile(pri_reg as *mut u8, 0x80); // Priority 0x80 (midrange)
            
            // Enable the interrupt (set bit in GICR_ISENABLER0)
            let enable_reg = sgi_base + GICR_ISENABLER0;
            let bit = irq;
            core::ptr::write_volatile(enable_reg as *mut u32, 1 << bit);
        }
    } else {
        // SPI: use Distributor
        let gic = GIC.lock();
        let reg = (irq / 32) as usize;
        let bit = irq % 32;
        
        unsafe {
            gic.write_gicd(GICD_ISENABLER + reg * 4, 1 << bit);
        }
    }
}

/// Disable specific interrupt
pub fn disable_irq(irq: u32) {
    let gic = GIC.lock();
    let reg = (irq / 32) as usize;
    let bit = irq % 32;
    
    unsafe {
        gic.write_gicd(GICD_ICENABLER + reg * 4, 1 << bit);
    }
}

/// Handle IRQ (called from exception handler)
pub fn handle_irq() {
    let iar: u64;
    unsafe {
        core::arch::asm!("mrs {}, ICC_IAR1_EL1", out(reg) iar);
    }
    
    let irq = (iar & 0x3FF) as u32;
    
    // Dispatch to registered handlers (non-preemptive path)
    match irq {
        30 => crate::hal::timer::handle_timer_irq(),
        33 => crate::hal::console::handle_uart_irq(), // UART0 SPI #1 = IRQ 33
        48..=79 => crate::drivers::virtio_mmio::handle_irq((irq - 48) as usize),
        1023 => { /* Spurious interrupt, ignore */ }
        _ => crate::kprintln!("Unhandled IRQ: {}", irq),
    }
    
    // End of interrupt
    unsafe {
        core::arch::asm!("msr ICC_EOIR1_EL1, {}", in(reg) iar);
    }
}

/// Handle IRQ with context for preemption
pub fn handle_irq_preemptive(ctx: &mut crate::arch::exceptions::ExceptionContext) {
    let iar: u64;
    unsafe {
        core::arch::asm!("mrs {}, ICC_IAR1_EL1", out(reg) iar);
    }
    
    let irq = (iar & 0x3FF) as u32;
    
    // Dispatch to registered handlers with preemption support
    match irq {
        30 => crate::hal::timer::handle_timer_irq_preemptive(ctx),
        33 => crate::hal::console::handle_uart_irq(), // UART0 SPI #1 = IRQ 33
        48..=79 => crate::drivers::virtio_mmio::handle_irq((irq - 48) as usize),
        1023 => { /* Spurious interrupt, ignore */ }
        _ => crate::kprintln!("Unhandled IRQ: {}", irq),
    }
    
    // End of interrupt
    unsafe {
        core::arch::asm!("msr ICC_EOIR1_EL1, {}", in(reg) iar);
    }
}
