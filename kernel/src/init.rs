//! Init Process — first user-space program
//!
//! This module contains a minimal user-space program embedded in the kernel
//! as raw AArch64 machine code. At boot, `launch()` creates a new process
//! with per-process page tables, copies the code into the user address space,
//! sets up a user stack, and schedules it for execution.
//!
//! The init program prints a startup banner, then loops — sleeping 2 seconds
//! and printing a heartbeat — demonstrating real user-space execution and
//! syscall functionality.

use crate::arch::aarch64::mmu::{PAGE_SIZE, PageFlags};
use crate::mm::{physical, virtual_mem};
use crate::process;

/// Base virtual address where the init code is mapped in user space.
const INIT_CODE_BASE: usize = 0x0040_0000; // 4 MB

/// User stack size for the init process (64 KB).
const INIT_STACK_SIZE: usize = 64 * 1024;

/// User stack top address (matches ELF loader convention).
const INIT_STACK_TOP: usize = 0x7FFF_FFF0_0000;

// ---------------------------------------------------------------------------
// Embedded init program (AArch64 machine code)
//
// The code below is assembled by the compiler via global_asm! and placed in
// .rodata.  ADR instructions are PC-relative, so the entire blob is
// position-independent: we can copy it to any user address and it will work.
// ---------------------------------------------------------------------------
core::arch::global_asm!(
    // Place in .rodata so the linker picks it up via *(.rodata .rodata.*)
    ".pushsection .rodata.initcode, \"a\", %progbits",
    ".balign 4",

    ".global __initcode_start",
    "__initcode_start:",

    // ---- Print startup banner ----
    "adr x0, 77f",             // x0 = &start_msg (PC-relative)
    "mov x1, #35",             // x1 = length
    "mov x8, #0xFF",           // syscall = SYS_NT_DEBUG_PRINT
    "svc #0",

    // ---- Loop: sleep 2 s + heartbeat ----
    "78:",
    "mov x0, #2000",           // 2000 ms
    "mov x8, #8",              // syscall = SYS_NT_SLEEP
    "svc #0",

    "adr x0, 79f",             // x0 = &tick_msg
    "mov x1, #26",             // x1 = length
    "mov x8, #0xFF",           // syscall = SYS_NT_DEBUG_PRINT
    "svc #0",

    "b 78b",                   // loop forever

    // ---- String data (included in the same copied blob) ----
    ".balign 4",
    "77: .ascii \"[init] User-space process started!\\n\"",  // 35 bytes
    "79: .ascii \"[init] Alive (user-space)\\n\"",           // 26 bytes

    ".balign 4",
    ".global __initcode_end",
    "__initcode_end:",

    ".popsection",
);

// Symbols emitted by global_asm — their *addresses* delimit the code blob.
extern "C" {
    static __initcode_start: u8;
    static __initcode_end: u8;
}

/// Get the raw init program bytes (instructions + string data).
fn init_code_bytes() -> &'static [u8] {
    unsafe {
        let start = &__initcode_start as *const u8;
        let end = &__initcode_end as *const u8;
        let len = end as usize - start as usize;
        core::slice::from_raw_parts(start, len)
    }
}

/// Launch the init user-space process.
///
/// Called once from `kernel_main` after all subsystems are initialised.
/// Creates a process with its own page tables, maps the init code + stack
/// into the process address space, and spawns a user-mode thread.
pub fn launch() {
    let code = init_code_bytes();
    if code.is_empty() {
        crate::kprintln!("[init] WARNING: init code blob is empty, skipping");
        return;
    }

    // 1. Create process (gives us a per-process PGD via clone_kernel_tables)
    let pid = match process::create_process("init") {
        Some(p) => p,
        None => {
            crate::kprintln!("[init] ERROR: failed to create init process");
            return;
        }
    };
    let pgd = process::get_process_pgd(pid);

    // 2. Allocate physical pages for code and map into process PGD
    let code_pages = (code.len() + PAGE_SIZE - 1) / PAGE_SIZE;
    let code_flags = PageFlags::VALID | PageFlags::PAGE | PageFlags::ACCESSED
        | PageFlags::INNER_SHAREABLE | PageFlags::USER | PageFlags::ATTR_NORMAL_WB;

    for i in 0..code_pages {
        let frame = physical::alloc_frame().expect("init: out of memory for code");
        // Zero then copy code bytes into the identity-mapped physical frame
        unsafe {
            core::ptr::write_bytes(frame as *mut u8, 0, PAGE_SIZE);
            let offset = i * PAGE_SIZE;
            let len = core::cmp::min(PAGE_SIZE, code.len() - offset);
            core::ptr::copy_nonoverlapping(
                code.as_ptr().add(offset),
                frame as *mut u8,
                len,
            );
        }
        let vaddr = INIT_CODE_BASE + i * PAGE_SIZE;
        virtual_mem::map_page_in(pgd, vaddr, frame, code_flags);
    }
    
    // Verify mapping
    if virtual_mem::virt_to_phys_in(pgd, INIT_CODE_BASE).is_none() {
        crate::kprintln!("[init] ERROR: {:#x} NOT MAPPED!", INIT_CODE_BASE);
    }

    // 3. Allocate user stack pages and map into process PGD
    let stack_pages = INIT_STACK_SIZE / PAGE_SIZE;
    let stack_base = INIT_STACK_TOP - INIT_STACK_SIZE;
    let stack_flags = PageFlags::VALID | PageFlags::PAGE | PageFlags::ACCESSED
        | PageFlags::INNER_SHAREABLE | PageFlags::USER | PageFlags::EXECUTE_NEVER
        | PageFlags::ATTR_NORMAL_WB;

    for i in 0..stack_pages {
        let frame = physical::alloc_frame().expect("init: out of memory for stack");
        unsafe {
            core::ptr::write_bytes(frame as *mut u8, 0, PAGE_SIZE);
        }
        let vaddr = stack_base + i * PAGE_SIZE;
        virtual_mem::map_page_in(pgd, vaddr, frame, stack_flags);
    }

    // 4. Create a thread with user-mode context (EL0, entry = INIT_CODE_BASE)
    let entry_point = INIT_CODE_BASE;
    let stack_top = INIT_STACK_TOP - 16; // 16-byte aligned

    use process::{Thread, ThreadContext, ProcessState, THREAD_TABLE};
    use process::thread;
    use process::scheduler;

    let tid = {
        let mut next = process::NEXT_TID.lock();
        let t = *next;
        *next += 1;
        t
    };

    let kernel_stack = thread::alloc_kernel_stack()
        .expect("init: failed to allocate kernel stack");

    let thread = Thread {
        tid,
        pid,
        state: ProcessState::Ready,
        priority: scheduler::PRIORITY_NORMAL,
        context: ThreadContext::for_user_entry(entry_point, stack_top, kernel_stack),
        kernel_stack,
        user_stack: stack_top,
    };

    THREAD_TABLE.lock().insert(tid, thread);

    // Link thread to process and schedule it
    process::attach_thread_to_process(pid, tid);
    scheduler::ready(tid);

    crate::kprintln!("[init] User-space init process launched (PID {}, TID {}, entry {:#x})",
        pid, tid, entry_point);
}
