//! ARM Generic Timer

use core::arch::asm;
use crate::sync::IrqMutex;
use crate::arch::aarch64::exceptions::ExceptionContext;

/// Timer frequency (usually 62.5 MHz on QEMU)
static TIMER_FREQ: IrqMutex<u64> = IrqMutex::new(0);

/// System ticks since boot
pub static SYSTEM_TICKS: IrqMutex<u64> = IrqMutex::new(0);

/// Timer interval in ticks (10ms)
const TIMER_INTERVAL_MS: u64 = 10;

/// Initialize timer
pub fn init() {
    let freq: u64;
    unsafe {
        asm!("mrs {}, cntfrq_el0", out(reg) freq);
    }
    
    *TIMER_FREQ.lock() = freq;
    
    // Set timer interval (10ms)
    let interval = freq * TIMER_INTERVAL_MS / 1000;
    
    unsafe {
        asm!("msr cntp_tval_el0, {}", in(reg) interval);
        // Enable timer, unmask interrupt
        asm!("msr cntp_ctl_el0, {}", in(reg) 1u64);
    }
    
    // Enable timer IRQ (PPI 30)
    crate::hal::interrupts::enable_irq(30);
}

/// Handle timer interrupt with preemption support
pub fn handle_timer_irq_preemptive(ctx: &mut ExceptionContext) {
    // Increment system ticks
    *SYSTEM_TICKS.lock() += 1;
    
    // Reload timer (must clear ISTATUS by writing new TVAL)
    let freq = *TIMER_FREQ.lock();
    let interval = freq * TIMER_INTERVAL_MS / 1000;
    unsafe {
        asm!("msr cntp_tval_el0, {}", in(reg) interval);
    }
    
    // Trigger preemptive scheduler
    crate::process::scheduler::tick_preemptive(ctx);
}

/// Handle timer interrupt (legacy non-preemptive version)
pub fn handle_timer_irq() {
    *SYSTEM_TICKS.lock() += 1;
    
    // Reload timer
    let freq = *TIMER_FREQ.lock();
    let interval = freq * TIMER_INTERVAL_MS / 1000;
    unsafe {
        asm!("msr cntp_tval_el0, {}", in(reg) interval);
    }
    
    // Trigger scheduler (non-preemptive)
    crate::process::scheduler::tick();
}

/// Get system uptime in milliseconds
pub fn uptime_ms() -> u64 {
    *SYSTEM_TICKS.lock() * TIMER_INTERVAL_MS
}

/// Get current timestamp
pub fn timestamp() -> u64 {
    let cnt: u64;
    unsafe {
        asm!("mrs {}, cntvct_el0", out(reg) cnt);
    }
    cnt
}
