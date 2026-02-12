//! AArch64 (ARM64) Architecture Support

pub mod cpu;
pub mod exceptions;
pub mod mmu;
pub mod boot;

/// Initialize AArch64 specific features
pub fn init() {
    cpu::init();
    exceptions::init();
    mmu::init();
}
