//! x86_64 Architecture Support

pub mod cpu;
pub mod exceptions;
pub mod mmu;
pub mod boot;

/// Initialize x86_64 specific features
pub fn init() {
    cpu::init();
    exceptions::init();
    mmu::init();
}
