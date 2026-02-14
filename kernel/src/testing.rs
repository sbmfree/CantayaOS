// CantayaOS Test Harness
//
// A custom test framework for bare-metal kernel testing.
// Uses QEMU's ISA debug-exit device (port 0xf4) to report results
// and serial output for test logging.
//
// To run tests:
//   1. Build with `--features test-mode`
//   2. Launch QEMU with `-device isa-debug-exit,iobase=0xf4,iosize=0x04`
//   3. QEMU exit code: 0x21 (33) = success, 0x23 (35) = failure
//
// Note: QEMU computes exit code as `(value << 1) | 1`, so:
//   - Writing 0x10 to port 0xf4 → exit code 0x21 (33) → SUCCESS
//   - Writing 0x11 to port 0xf4 → exit code 0x23 (35) → FAILURE

use crate::hal::port::outd;

// Re-import serial macros (they're #[macro_export] so they live at crate root)
use crate::{serial_print, serial_println};

/// Exit QEMU with a success code (0x21 / 33)
pub fn exit_qemu_success() -> ! {
    unsafe { outd(0xf4, 0x10); }
    // If we're not running in QEMU, just halt
    loop { core::hint::spin_loop(); }
}

/// Exit QEMU with a failure code (0x23 / 35)
pub fn exit_qemu_failure() -> ! {
    unsafe { outd(0xf4, 0x11); }
    loop { core::hint::spin_loop(); }
}

/// A test function descriptor
pub struct TestCase {
    pub name: &'static str,
    pub test_fn: fn(),
}

/// Run all registered tests and report results via serial
pub fn run_tests(tests: &[TestCase]) {
    serial_println!("\n========================================");
    serial_println!("  CantayaOS Test Runner");
    serial_println!("  Running {} test(s)...", tests.len());
    serial_println!("========================================\n");

    let mut passed = 0u32;
    let mut failed = 0u32;

    for test in tests {
        serial_print!("  test {} ... ", test.name);

        // We use a simple panic-catch approach:
        // If the test panics, our panic handler will detect test mode and report failure.
        // If it returns normally, it passed.
        (test.test_fn)();

        serial_println!("ok");
        passed += 1;
    }

    serial_println!("\n========================================");
    serial_println!("  Results: {} passed, {} failed", passed, failed);
    serial_println!("========================================\n");

    if failed > 0 {
        exit_qemu_failure();
    } else {
        exit_qemu_success();
    }
}

/// Macro to define a test case
#[macro_export]
macro_rules! kernel_test {
    ($name:ident, $body:expr) => {
        $crate::testing::TestCase {
            name: stringify!($name),
            test_fn: || { $body },
        }
    };
}

// ============================================================================
// Built-in kernel tests
// ============================================================================

/// Basic sanity tests that don't need full kernel initialization
pub fn builtin_tests() -> alloc::vec::Vec<TestCase> {
    extern crate alloc;
    use alloc::vec;

    vec![
        kernel_test!(trivial_assertion, {
            assert_eq!(1 + 1, 2);
        }),
        kernel_test!(kernel_error_display, {
            use crate::error::KernelError;
            let err = KernelError::OutOfMemory;
            let code = err.to_status_code();
            assert!(code < 0, "Error status codes should be negative");
        }),
        kernel_test!(kernel_error_variants, {
            use crate::error::KernelError;
            // Verify different errors have different status codes
            let e1 = KernelError::OutOfMemory;
            let e2 = KernelError::InvalidAddress(0xDEAD);
            assert_ne!(e1.to_status_code(), e2.to_status_code());
        }),
        kernel_test!(heap_alloc_and_free, {
            use alloc::vec::Vec;
            let mut v: Vec<u32> = Vec::new();
            for i in 0..100 {
                v.push(i);
            }
            assert_eq!(v.len(), 100);
            assert_eq!(v[99], 99);
            // v is dropped here, freeing memory
        }),
        kernel_test!(string_formatting, {
            use alloc::format;
            let s = format!("CantayaOS v{}.{}", 0, 1);
            assert!(s.contains("CantayaOS"));
            assert!(s.contains("0.1"));
        }),
        kernel_test!(btree_map_operations, {
            use alloc::collections::BTreeMap;
            let mut map = BTreeMap::new();
            map.insert("key1", 42);
            map.insert("key2", 84);
            assert_eq!(map.get("key1"), Some(&42));
            assert_eq!(map.get("key2"), Some(&84));
            assert_eq!(map.get("key3"), None);
            assert_eq!(map.len(), 2);
        }),
    ]
}
