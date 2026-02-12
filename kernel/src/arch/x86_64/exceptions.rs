//! Exception and interrupt handling for x86_64
//!
//! Implements IDT (Interrupt Descriptor Table) with full context save/restore,
//! vector table, and dispatch to sync/IRQ/exception handlers.

use core::arch::{asm, global_asm};

/// Exception context saved on stack
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExceptionContext {
    // General purpose registers (pushed by SAVE_CONTEXT)
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    
    // Error code (pushed by CPU or stub for exceptions without error code)
    pub error_code: u64,
    
    // Interrupt frame (pushed by CPU)
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl ExceptionContext {
    pub const fn new() -> Self {
        ExceptionContext {
            rax: 0, rbx: 0, rcx: 0, rdx: 0,
            rsi: 0, rdi: 0, rbp: 0,
            r8: 0, r9: 0, r10: 0, r11: 0,
            r12: 0, r13: 0, r14: 0, r15: 0,
            error_code: 0,
            rip: 0, cs: 0, rflags: 0, rsp: 0, ss: 0,
        }
    }
}

/// IDT Entry
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    flags: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const fn new() -> Self {
        IdtEntry {
            offset_low: 0,
            selector: 0,
            ist: 0,
            flags: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    fn set_handler(&mut self, handler: unsafe extern "C" fn(), selector: u16, flags: u8) {
        let addr = handler as u64;
        self.offset_low = addr as u16;
        self.offset_mid = (addr >> 16) as u16;
        self.offset_high = (addr >> 32) as u32;
        self.selector = selector;
        self.ist = 0;
        self.flags = flags;
        self.reserved = 0;
    }
}

/// IDT Descriptor for LIDT instruction
#[repr(C, packed)]
struct IdtDescriptor {
    limit: u16,
    base: u64,
}

/// Interrupt Descriptor Table (256 entries)
static mut IDT: [IdtEntry; 256] = [IdtEntry::new(); 256];

// External ISR handlers defined in global_asm
extern "C" {
    fn isr0();
    fn isr1();
    fn isr2();
    fn isr3();
    fn isr4();
    fn isr5();
    fn isr6();
    fn isr7();
    fn isr8();
    fn isr10();
    fn isr11();
    fn isr12();
    fn isr13();
    fn isr14();
    fn isr16();
    fn isr17();
    fn isr18();
    fn isr19();
    fn isr20();
    fn isr30();
    fn isr128();
    fn isr_irq_common();
    fn isr_spurious();
}

/// Initialize exception vectors (IDT)
pub fn init() {
    unsafe {
        // Set up exception handlers (0-31)
        IDT[0].set_handler(isr0, 0x08, 0x8E);   // Divide by zero
        IDT[1].set_handler(isr1, 0x08, 0x8E);   // Debug
        IDT[2].set_handler(isr2, 0x08, 0x8E);   // NMI
        IDT[3].set_handler(isr3, 0x08, 0x8E);   // Breakpoint
        IDT[4].set_handler(isr4, 0x08, 0x8E);   // Overflow
        IDT[5].set_handler(isr5, 0x08, 0x8E);   // Bound Range
        IDT[6].set_handler(isr6, 0x08, 0x8E);   // Invalid Opcode
        IDT[7].set_handler(isr7, 0x08, 0x8E);   // Device Not Available
        IDT[8].set_handler(isr8, 0x08, 0x8E);   // Double Fault
        IDT[10].set_handler(isr10, 0x08, 0x8E); // Invalid TSS
        IDT[11].set_handler(isr11, 0x08, 0x8E); // Segment Not Present
        IDT[12].set_handler(isr12, 0x08, 0x8E); // Stack Fault
        IDT[13].set_handler(isr13, 0x08, 0x8E); // General Protection
        IDT[14].set_handler(isr14, 0x08, 0x8E); // Page Fault
        IDT[16].set_handler(isr16, 0x08, 0x8E); // x87 FPU Error
        IDT[17].set_handler(isr17, 0x08, 0x8E); // Alignment Check
        IDT[18].set_handler(isr18, 0x08, 0x8E); // Machine Check
        IDT[19].set_handler(isr19, 0x08, 0x8E); // SIMD Exception
        IDT[20].set_handler(isr20, 0x08, 0x8E); // Virtualization Exception
        IDT[30].set_handler(isr30, 0x08, 0x8E); // Security Exception
        
        // System call handler (int 0x80)
        IDT[0x80].set_handler(isr128, 0x08, 0xEE); // DPL=3 for user mode
        
        // Hardware interrupts (32-47)
        for i in 32..48 {
            IDT[i].set_handler(isr_irq_common, 0x08, 0x8E);
        }
        
        // Spurious interrupts (48-255)
        for i in 48..256 {
            IDT[i].set_handler(isr_spurious, 0x08, 0x8E);
        }
        
        // Load IDT
        let idtr = IdtDescriptor {
            limit: (core::mem::size_of::<[IdtEntry; 256]>() - 1) as u16,
            base: IDT.as_ptr() as u64,
        };
        
        asm!("lidt [{}]", in(reg) &idtr, options(readonly, nostack, preserves_flags));
        crate::kprintln!("[EXC] IDT installed at {:#x}", IDT.as_ptr() as u64);
    }
}

// ---------------------------------------------------------------------------
// Exception handlers and context save/restore macros in global_asm!
// ---------------------------------------------------------------------------

global_asm!(
r#"
// =====================================================================
// Context save macro
// =====================================================================
.macro SAVE_CONTEXT
    // Push general purpose registers
    push r15
    push r14
    push r13
    push r12
    push r11
    push r10
    push r9
    push r8
    push rbp
    push rdi
    push rsi
    push rdx
    push rcx
    push rbx
    push rax
    
    // RDI = pointer to ExceptionContext on stack
    mov rdi, rsp
.endm

// =====================================================================
// Context restore macro
// =====================================================================
.macro RESTORE_CONTEXT
    // Pop general purpose registers
    pop rax
    pop rbx
    pop rcx
    pop rdx
    pop rsi
    pop rdi
    pop rbp
    pop r8
    pop r9
    pop r10
    pop r11
    pop r12
    pop r13
    pop r14
    pop r15
    
    // Skip error code
    add rsp, 8
    
    // Return (pops RIP, CS, RFLAGS, RSP, SS)
    iretq
.endm

// =====================================================================
// ISR stub generators
// =====================================================================

// Exception without error code
.macro ISR_NOERR num
.global isr\num
isr\num\():
    push 0              // Dummy error code
    SAVE_CONTEXT
    mov rsi, \num       // Exception number in RSI
    call exception_handler_rust
    RESTORE_CONTEXT
.endm

// Exception with error code (CPU pushes it)
.macro ISR_ERR num
.global isr\num
isr\num\():
    // Error code already pushed by CPU
    SAVE_CONTEXT
    mov rsi, \num       // Exception number in RSI
    call exception_handler_rust
    RESTORE_CONTEXT
.endm

// =====================================================================
// Exception handlers (0-31)
// =====================================================================
ISR_NOERR 0   // Divide by zero
ISR_NOERR 1   // Debug
ISR_NOERR 2   // NMI
ISR_NOERR 3   // Breakpoint
ISR_NOERR 4   // Overflow
ISR_NOERR 5   // Bound Range
ISR_NOERR 6   // Invalid Opcode
ISR_NOERR 7   // Device Not Available
ISR_ERR   8   // Double Fault (with error code)
ISR_ERR   10  // Invalid TSS
ISR_ERR   11  // Segment Not Present
ISR_ERR   12  // Stack Fault
ISR_ERR   13  // General Protection
ISR_ERR   14  // Page Fault
ISR_NOERR 16  // x87 FPU Error
ISR_ERR   17  // Alignment Check
ISR_NOERR 18  // Machine Check
ISR_NOERR 19  // SIMD Exception
ISR_NOERR 20  // Virtualization Exception
ISR_ERR   30  // Security Exception

// System call (int 0x80)
ISR_NOERR 128

// =====================================================================
// IRQ handler
// =====================================================================
.global isr_irq_common
isr_irq_common:
    push 0
    SAVE_CONTEXT
    call irq_handler_rust
    RESTORE_CONTEXT

// =====================================================================
// Spurious interrupt handler
// =====================================================================
.global isr_spurious
isr_spurious:
    push 0
    SAVE_CONTEXT
    call spurious_handler_rust
    RESTORE_CONTEXT
"#
);

// ---------------------------------------------------------------------------
// Rust exception handlers (called with RDI = *ExceptionContext, RSI = vector)
// ---------------------------------------------------------------------------

#[no_mangle]
extern "C" fn exception_handler_rust(ctx: &mut ExceptionContext, vector: u64) {
    match vector {
        0x80 => {
            // System call (int 0x80)
            crate::process::syscall::handle_syscall_ctx(ctx);
        }
        14 => {
            // Page Fault
            let cr2: u64;
            unsafe {
                asm!("mov {}, cr2", out(reg) cr2);
            }
            crate::kprintln!("[exc] Page Fault at {:#x} (CR2={:#x} err={:#x})", ctx.rip, cr2, ctx.error_code);
            if !crate::mm::handle_page_fault(cr2) {
                panic!("Kernel page fault at {:#x} CR2={:#x}", ctx.rip, cr2);
            }
        }
        6 => {
            // Invalid Opcode
            crate::kprintln!("[exc] Invalid Opcode at {:#x}", ctx.rip);
            panic!("Invalid opcode");
        }
        13 => {
            // General Protection Fault
            crate::kprintln!("[exc] General Protection Fault at {:#x} (err={:#x})", ctx.rip, ctx.error_code);
            
            // Check if from user mode
            if (ctx.cs & 0x3) == 3 {
                crate::kprintln!("[exc] Killing user process: GPF");
                terminate_faulting_and_schedule(ctx);
            } else {
                panic!("Kernel GPF at {:#x}", ctx.rip);
            }
        }
        _ => {
            crate::kprintln!("Unhandled exception:");
            crate::kprintln!("  Vector = {}", vector);
            crate::kprintln!("  Error  = {:#x}", ctx.error_code);
            crate::kprintln!("  RIP    = {:#x}", ctx.rip);
            crate::kprintln!("  CS     = {:#x}", ctx.cs);
            crate::kprintln!("  RFLAGS = {:#x}", ctx.rflags);
            panic!("Unhandled exception vector={}", vector);
        }
    }
}

#[no_mangle]
extern "C" fn irq_handler_rust(ctx: &mut ExceptionContext) {
    crate::hal::interrupts::handle_irq_preemptive(ctx);
}

#[no_mangle]
extern "C" fn spurious_handler_rust(_ctx: &mut ExceptionContext) {
    // Ignore spurious interrupts
}

/// Terminate the currently-running userspace process and load the next thread.
fn terminate_faulting_and_schedule(ctx: &mut ExceptionContext) {
    use crate::process::scheduler;
    // Find current thread's PID and terminate its process
    if let Some(tid) = scheduler::current_tid() {
        let pid = {
            let table = crate::process::THREAD_TABLE.lock();
            table.get(&tid).map(|t| t.pid)
        };
        if let Some(pid) = pid {
            crate::process::terminate_process(pid, -1); // exit code -1 = killed
        }
    }
    // Schedule next thread via preemptive path (loads new context into ctx)
    scheduler::schedule_preemptive(ctx);
}
