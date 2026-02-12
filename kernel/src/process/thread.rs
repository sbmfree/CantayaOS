//! Thread implementation with AArch64 context switching

use super::{Thread, ThreadContext, ProcessState};
use core::arch::naked_asm;

impl ThreadContext {
    /// Create a new context for a thread starting at the given entry point
    pub fn for_entry(entry_point: usize, stack_top: usize) -> Self {
        let mut ctx = ThreadContext::new();
        ctx.pc = entry_point as u64;
        ctx.sp = stack_top as u64;
        // PSTATE: EL1h, interrupts enabled, AArch64
        ctx.pstate = 0x0000_0005; // EL1h (SPSel=1)
        ctx
    }

    /// Create a user-mode context.
    /// Uses a trampoline so the thread can be first-scheduled either
    /// preemptively (via timer IRQ eret) or cooperatively (via context_switch ret).
    /// `kernel_stack` is the top of the thread's kernel-mode stack.
    pub fn for_user_entry(entry_point: usize, user_stack: usize, kernel_stack: usize, argc: u64, argv: u64) -> Self {
        let mut ctx = ThreadContext::new();
        // x19-x21 carry arguments for the trampoline
        ctx.regs[19] = entry_point as u64;        // → ELR_EL1
        ctx.regs[20] = user_stack as u64;          // → SP_EL0
        ctx.regs[21] = 0;                          // → SPSR_EL1 (EL0t)
        ctx.regs[22] = argc;                       // → x0 (argc)
        ctx.regs[23] = argv;                       // → x1 (argv)
        // x30 = trampoline (used by cooperative context_switch `ret`)
        ctx.regs[30] = return_to_user_trampoline as *const () as usize as u64;
        // SP = kernel stack (used by both paths)
        ctx.sp = kernel_stack as u64;
        // PC+PSTATE for preemptive path: eret → trampoline at EL1h
        ctx.pc = return_to_user_trampoline as *const () as usize as u64;
        ctx.pstate = 0x0000_0005; // EL1h
        ctx
    }
}

impl Thread {
    /// Save current thread context
    pub fn save_context(&mut self, ctx: &ThreadContext) {
        self.context = ThreadContext {
            regs: ctx.regs,
            sp: ctx.sp,
            pc: ctx.pc,
            pstate: ctx.pstate,
        };
    }

    /// Set thread state
    pub fn set_state(&mut self, state: ProcessState) {
        self.state = state;
    }

    /// Get thread priority
    pub fn priority(&self) -> u8 {
        self.priority
    }

    /// Set thread priority (0-31)
    pub fn set_priority(&mut self, priority: u8) {
        self.priority = priority.min(31);
    }
}

/// Perform a context switch between two threads.
/// Saves the current CPU state into `old` and loads state from `new`.
///
/// # Safety
/// Both pointers must be valid ThreadContext pointers.
#[unsafe(naked)]
pub unsafe extern "C" fn context_switch(old: *mut ThreadContext, new: *const ThreadContext) {
    naked_asm!(
        // Save callee-saved registers into old context
        // x0 = old context pointer, x1 = new context pointer

        // Save x19-x30 (callee-saved)
        "stp x19, x20, [x0, #(19 * 8)]",
        "stp x21, x22, [x0, #(21 * 8)]",
        "stp x23, x24, [x0, #(23 * 8)]",
        "stp x25, x26, [x0, #(25 * 8)]",
        "stp x27, x28, [x0, #(27 * 8)]",
        "stp x29, x30, [x0, #(29 * 8)]",

        // Save SP
        "mov x2, sp",
        "str x2, [x0, #(31 * 8)]",

        // Save return address as PC
        "str x30, [x0, #(32 * 8)]",

        // --- Now restore from new context ---

        // Restore callee-saved registers
        "ldp x19, x20, [x1, #(19 * 8)]",
        "ldp x21, x22, [x1, #(21 * 8)]",
        "ldp x23, x24, [x1, #(23 * 8)]",
        "ldp x25, x26, [x1, #(25 * 8)]",
        "ldp x27, x28, [x1, #(27 * 8)]",
        "ldp x29, x30, [x1, #(29 * 8)]",

        // Restore SP
        "ldr x2, [x1, #(31 * 8)]",
        "mov sp, x2",

        // Jump to saved PC (in x30/LR)
        "ret",
    );
}

/// Trampoline that transitions from EL1 → EL0 for first-time user thread scheduling.
///
/// Expects:
///   x19 = user-space entry point  (→ ELR_EL1)
///   x20 = user-space stack top    (→ SP_EL0)
///   x21 = SPSR value              (→ SPSR_EL1, typically 0 = EL0t)
///
/// Both preemptive (eret from IRQ handler) and cooperative (ret from context_switch)
/// paths reach this function on the new thread's first run.
#[unsafe(naked)]
pub unsafe extern "C" fn return_to_user_trampoline() {
    naked_asm!(
        "msr elr_el1, x19",
        "msr spsr_el1, x21",
        "msr sp_el0, x20",
        // Pass argc and argv to user space
        "mov x0, x22",
        "mov x1, x23",
        "mov x2, #0",
        "mov x3, #0",
        "mov x4, #0",
        "mov x5, #0",
        "mov x6, #0",
        "mov x7, #0",
        "mov x8, #0",
        "eret",
    );
}

/// Stack size per thread (16KB)
pub const THREAD_STACK_SIZE: usize = 4096 * 4;

/// Allocate a kernel stack for a thread (contiguous physical frames)
pub fn alloc_kernel_stack() -> Option<usize> {
    let pages = THREAD_STACK_SIZE / crate::arch::mmu::PAGE_SIZE;
    
    // Allocate contiguous frames for the stack
    let base = crate::mm::physical::alloc_contiguous_frames(pages)?;
    
    // Return top of stack (stacks grow downward on ARM64)
    Some(base + THREAD_STACK_SIZE)
}
