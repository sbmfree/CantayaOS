//! Exception handling for AArch64
//!
//! Full context save/restore around exception entry/exit,
//! vector table, and dispatch to sync/IRQ/FIQ/SError handlers.

use core::arch::{asm, global_asm};

/// Exception context saved on stack
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExceptionContext {
    pub regs: [u64; 31],  // x0-x30
    pub sp: u64,
    pub pc: u64,           // ELR_EL1
    pub pstate: u64,       // SPSR_EL1
    pub esr: u64,          // ESR_EL1
    pub far: u64,          // FAR_EL1
}

impl ExceptionContext {
    pub const fn new() -> Self {
        ExceptionContext {
            regs: [0; 31],
            sp: 0,
            pc: 0,
            pstate: 0,
            esr: 0,
            far: 0,
        }
    }
}

/// Initialize exception vectors
pub fn init() {
    unsafe {
        let vbar: u64;
        asm!(
            "adrp {vbar}, exception_vector_table",
            "add  {vbar}, {vbar}, :lo12:exception_vector_table",
            "msr vbar_el1, {vbar}",
            "isb",
            vbar = out(reg) vbar,
        );
        crate::kprintln!("[EXC] Exception vectors installed at {:#x}", vbar);
    }
}

// ---------------------------------------------------------------------------
// Exception vector table and trampoline stubs in global_asm!
// This avoids issues with naked functions and macro expansion.
// ---------------------------------------------------------------------------

global_asm!(
r#"
// =====================================================================
// Context save macro (saves 36 x 8 = 288 bytes)
// =====================================================================
.macro SAVE_CONTEXT
    sub sp, sp, #(36 * 8)
    stp x0,  x1,  [sp, #(0  * 8)]
    stp x2,  x3,  [sp, #(2  * 8)]
    stp x4,  x5,  [sp, #(4  * 8)]
    stp x6,  x7,  [sp, #(6  * 8)]
    stp x8,  x9,  [sp, #(8  * 8)]
    stp x10, x11, [sp, #(10 * 8)]
    stp x12, x13, [sp, #(12 * 8)]
    stp x14, x15, [sp, #(14 * 8)]
    stp x16, x17, [sp, #(16 * 8)]
    stp x18, x19, [sp, #(18 * 8)]
    stp x20, x21, [sp, #(20 * 8)]
    stp x22, x23, [sp, #(22 * 8)]
    stp x24, x25, [sp, #(24 * 8)]
    stp x26, x27, [sp, #(26 * 8)]
    stp x28, x29, [sp, #(28 * 8)]
    str x30,       [sp, #(30 * 8)]
    mrs x0, sp_el0
    str x0,        [sp, #(31 * 8)]
    mrs x0, elr_el1
    str x0,        [sp, #(32 * 8)]
    mrs x0, spsr_el1
    str x0,        [sp, #(33 * 8)]
    mrs x0, esr_el1
    str x0,        [sp, #(34 * 8)]
    mrs x0, far_el1
    str x0,        [sp, #(35 * 8)]
    mov x0, sp
.endm

// =====================================================================
// Context restore macro
// =====================================================================
.macro RESTORE_CONTEXT
    ldr x0,        [sp, #(33 * 8)]
    msr spsr_el1, x0
    ldr x0,        [sp, #(32 * 8)]
    msr elr_el1, x0
    ldr x0,        [sp, #(31 * 8)]
    msr sp_el0, x0
    ldp x28, x29, [sp, #(28 * 8)]
    ldp x26, x27, [sp, #(26 * 8)]
    ldp x24, x25, [sp, #(24 * 8)]
    ldp x22, x23, [sp, #(22 * 8)]
    ldp x20, x21, [sp, #(20 * 8)]
    ldp x18, x19, [sp, #(18 * 8)]
    ldp x16, x17, [sp, #(16 * 8)]
    ldp x14, x15, [sp, #(14 * 8)]
    ldp x12, x13, [sp, #(12 * 8)]
    ldp x10, x11, [sp, #(10 * 8)]
    ldp x8,  x9,  [sp, #(8  * 8)]
    ldp x6,  x7,  [sp, #(6  * 8)]
    ldp x4,  x5,  [sp, #(4  * 8)]
    ldp x2,  x3,  [sp, #(2  * 8)]
    ldr x30,       [sp, #(30 * 8)]
    ldp x0,  x1,  [sp, #(0  * 8)]
    add sp, sp, #(36 * 8)
    eret
.endm

// =====================================================================
// Trampoline stubs: each saves context, calls Rust handler, restores
// =====================================================================
.global __trampoline_sync
__trampoline_sync:
    SAVE_CONTEXT
    bl sync_handler_rust
    RESTORE_CONTEXT

.global __trampoline_irq
__trampoline_irq:
    SAVE_CONTEXT
    bl irq_handler_rust
    RESTORE_CONTEXT

.global __trampoline_fiq
__trampoline_fiq:
    SAVE_CONTEXT
    bl fiq_handler_rust
    RESTORE_CONTEXT

.global __trampoline_serror
__trampoline_serror:
    SAVE_CONTEXT
    bl serror_handler_rust
    RESTORE_CONTEXT

.global __trampoline_sync_lower
__trampoline_sync_lower:
    SAVE_CONTEXT
    bl sync_lower_handler_rust
    RESTORE_CONTEXT

// =====================================================================
// Exception vector table (2KB aligned, 16 entries of 0x80 bytes each)
// =====================================================================
.section .vectors, "ax"
.balign 2048
.global exception_vector_table
exception_vector_table:
    // ------- Current EL with SP0 -------
    .balign 0x80
    b __trampoline_sync
    .balign 0x80
    b __trampoline_irq
    .balign 0x80
    b __trampoline_fiq
    .balign 0x80
    b __trampoline_serror

    // ------- Current EL with SPx -------
    .balign 0x80
    b __trampoline_sync
    .balign 0x80
    b __trampoline_irq
    .balign 0x80
    b __trampoline_fiq
    .balign 0x80
    b __trampoline_serror

    // ------- Lower EL using AArch64 -------
    .balign 0x80
    b __trampoline_sync_lower
    .balign 0x80
    b __trampoline_irq
    .balign 0x80
    b __trampoline_fiq
    .balign 0x80
    b __trampoline_serror

    // ------- Lower EL using AArch32 -------
    .balign 0x80
    b __trampoline_sync_lower
    .balign 0x80
    b __trampoline_irq
    .balign 0x80
    b __trampoline_fiq
    .balign 0x80
    b __trampoline_serror
"#
);

// ---------------------------------------------------------------------------
// Rust exception handlers (called with x0 = *ExceptionContext)
// ---------------------------------------------------------------------------

#[no_mangle]
extern "C" fn sync_handler_rust(ctx: &mut ExceptionContext) {
    let esr = ctx.esr;
    let ec = (esr >> 26) & 0x3F;
    let iss = esr & 0x1FFFFFF;

    match ec {
        0x15 => {
            // SVC from AArch64 — system call
            crate::process::syscall::handle_syscall_ctx(ctx);
        }
        0x20 | 0x21 => {
            // Instruction Abort
            crate::kprintln!("[exc] Instruction Abort at {:#x} (FAR={:#x} ISS={:#x})", ctx.pc, ctx.far, iss);
            crate::mm::handle_page_fault(ctx.far);
        }
        0x24 | 0x25 => {
            // Data Abort
            crate::kprintln!("[exc] Data Abort at {:#x} (FAR={:#x} ISS={:#x})", ctx.pc, ctx.far, iss);
            crate::mm::handle_page_fault(ctx.far);
        }
        _ => {
            crate::kprintln!("Unhandled synchronous exception:");
            crate::kprintln!("  EC  = {:#x}", ec);
            crate::kprintln!("  ISS = {:#x}", iss);
            crate::kprintln!("  ESR = {:#x}", esr);
            crate::kprintln!("  FAR = {:#x}", ctx.far);
            crate::kprintln!("  ELR = {:#x}", ctx.pc);
            panic!("Unhandled exception EC={:#x}", ec);
        }
    }
}

#[no_mangle]
extern "C" fn sync_lower_handler_rust(ctx: &mut ExceptionContext) {
    let esr = ctx.esr;
    let ec = (esr >> 26) & 0x3F;

    match ec {
        0x15 => {
            // SVC from lower EL — user-mode system call
            crate::process::syscall::handle_syscall_ctx(ctx);
        }
        0x20 | 0x24 => {
            // Instruction/Data Abort from lower EL
            crate::kprintln!("[exc] Lower-EL abort: EC={:#x} FAR={:#x} ELR={:#x}",
                ec, ctx.far, ctx.pc);
            crate::mm::handle_page_fault(ctx.far);
        }
        _ => {
            crate::kprintln!("Unhandled lower-EL exception: EC={:#x} FAR={:#x} ELR={:#x}", ec, ctx.far, ctx.pc);
            panic!("Unhandled lower-EL exception EC={:#x}", ec);
        }
    }
}

#[no_mangle]
extern "C" fn irq_handler_rust(ctx: &mut ExceptionContext) {
    crate::hal::interrupts::handle_irq_preemptive(ctx);
}

#[no_mangle]
extern "C" fn fiq_handler_rust(_ctx: &mut ExceptionContext) {
    panic!("FIQ not implemented");
}

#[no_mangle]
extern "C" fn serror_handler_rust(ctx: &mut ExceptionContext) {
    crate::kprintln!("System Error (SError):");
    crate::kprintln!("  ESR = {:#x}", ctx.esr);
    crate::kprintln!("  ELR = {:#x}", ctx.pc);
    panic!("SError exception");
}
