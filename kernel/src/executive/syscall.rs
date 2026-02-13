// System Call Interface
//
// System calls (syscalls) are the mechanism by which user-mode programs
// request services from the kernel. On x86_64, this is done via the SYSCALL
// instruction (fast system call).
//
// In Windows NT:
//   - User mode calls NtXxx functions (ntdll.dll)
//   - ntdll.dll executes SYSCALL with the function number in RAX
//   - The CPU switches to Ring 0 and jumps to KiSystemCall64
//   - KiSystemCall64 dispatches to the correct Nt handler based on the number
//
// Our syscall ABI:
//   - RAX: syscall number
//   - RDI: argument 1
//   - RSI: argument 2
//   - RDX: argument 3
//   - R10: argument 4 (RCX is clobbered by SYSCALL)
//   - R8:  argument 5
//   - R9:  argument 6
//   - RAX: return value (0 = success, negative = error)
//
// Current status: Stub
// We define the syscall numbers and will implement the SYSCALL/SYSRET mechanism
// when user-mode is ready.

/// System call numbers.
///
/// These are the function numbers that user-mode programs place in RAX
/// before executing the SYSCALL instruction.
///
/// Convention: We group syscalls by subsystem, with gaps for future additions.
/// Process management: 0-99
/// Thread management: 100-199
/// Memory management: 200-299
/// File I/O: 300-399
/// Device I/O: 400-499
/// Graphics/Window: 500-599
#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallNumber {
    // Process Management
    Exit = 0,
    CreateProcess = 1,
    TerminateProcess = 2,
    GetProcessId = 3,

    // Thread Management
    CreateThread = 100,
    ExitThread = 101,
    GetThreadId = 102,
    Sleep = 103,
    Yield = 104,

    // Memory Management
    AllocateMemory = 200,
    FreeMemory = 201,
    MapMemory = 202,

    // File I/O
    Open = 300,
    Close = 301,
    Read = 302,
    Write = 303,

    // Console / Debug
    DebugPrint = 900,
}

/// System call return codes
#[repr(i64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallResult {
    Success = 0,
    InvalidSyscall = -1,
    InvalidArgument = -2,
    PermissionDenied = -3,
    NotFound = -4,
    OutOfMemory = -5,
    NotImplemented = -6,
}

/// Initialize the SYSCALL/SYSRET mechanism.
///
/// This programs the CPU's MSRs (Model Specific Registers) to set up:
///   - STAR: segment selectors for SYSCALL/SYSRET transitions
///   - LSTAR: kernel entry point for SYSCALL
///   - SFMASK: flags to clear on SYSCALL (disable interrupts during transition)
///
/// Even though we don't have user-mode yet, we set up the MSRs now so the
/// infrastructure is ready. The SYSCALL entry point is a minimal stub that
/// returns -ENOSYS for all calls.
pub fn init() {
    use crate::hal::cpu::{rdmsr, wrmsr};
    use crate::hal::gdt::{KERNEL_CODE_SELECTOR, KERNEL_DATA_SELECTOR};

    const IA32_STAR: u32 = 0xC000_0081;
    const IA32_LSTAR: u32 = 0xC000_0082;
    const IA32_SFMASK: u32 = 0xC000_0084;
    const IA32_EFER: u32 = 0xC000_0080;
    const EFER_SCE: u64 = 1 << 0; // SYSCALL enable

    // Enable SYSCALL/SYSRET in EFER MSR
    unsafe {
        let efer = rdmsr(IA32_EFER);
        wrmsr(IA32_EFER, efer | EFER_SCE);
    }

    // STAR MSR layout:
    //   Bits 63:48 = SYSRET CS/SS base selector (user mode: CS = base, SS = base+8)
    //   Bits 47:32 = SYSCALL CS/SS selectors (kernel: CS = value, SS = value+8)
    //   Bits 31:0 = reserved (must be 0 for AMD64)
    //
    // For SYSRET, the CPU uses CS = STAR[63:48]+16  and SS = STAR[63:48]+8
    // We put user CS/SS base at 0x18 (GDT slot 3) — user data SS = 0x20, user code CS = 0x28
    // For now these are placeholders since we have no user segments yet.
    let kernel_cs = KERNEL_CODE_SELECTOR as u64;
    let user_cs_base = KERNEL_DATA_SELECTOR as u64; // placeholder for user segments

    let star = (user_cs_base << 48) | (kernel_cs << 32);
    unsafe {
        wrmsr(IA32_STAR, star);

        // LSTAR: kernel entry point for SYSCALL instruction
        wrmsr(IA32_LSTAR, syscall_entry_stub as u64);

        // SFMASK: RFLAGS bits to clear on SYSCALL
        // Clear IF (bit 9) to disable interrupts during kernel entry
        // Clear TF (bit 8) to disable single-stepping
        // Clear DF (bit 10) to ensure forward string ops
        let sfmask: u64 = (1 << 9) | (1 << 8) | (1 << 10); // IF | TF | DF
        wrmsr(IA32_SFMASK, sfmask);
    }

    log::info!(
        "SYSCALL MSRs programmed: STAR={:#X}, LSTAR={:#X}",
        star, syscall_entry_stub as u64
    );
}

/// Minimal SYSCALL entry point stub.
///
/// When user-mode is implemented, this will save registers, switch stacks,
/// and dispatch to the Rust handler. For now it returns -1 (not implemented)
/// and executes SYSRETQ.
#[unsafe(naked)]
extern "C" fn syscall_entry_stub() {
    core::arch::naked_asm!(
        // RCX = user RIP (saved by CPU)
        // R11 = user RFLAGS (saved by CPU)
        // RAX = syscall number -> will be return value
        //
        // For now: return -6 (NotImplemented) in RAX
        "mov rax, -6",    // SyscallResult::NotImplemented
        "sysretq",
    );
}

/// Main syscall dispatcher (will be called from the SYSCALL entry point)
///
/// Routes syscall numbers to their handler functions.
pub fn dispatch(number: u64, arg1: u64, arg2: u64, _arg3: u64, _arg4: u64) -> i64 {
    match number {
        0 => {
            // SYS_EXIT: terminate the current process
            log::info!("Syscall: exit({})", arg1);
            SyscallResult::NotImplemented as i64
        }
        900 => {
            // SYS_DEBUG_PRINT: print a debug string (for early development)
            // arg1 = pointer to string, arg2 = length
            log::info!("Syscall: debug_print({:#X}, {})", arg1, arg2);
            SyscallResult::NotImplemented as i64
        }
        _ => {
            log::warn!("Unknown syscall: {}", number);
            SyscallResult::InvalidSyscall as i64
        }
    }
}
