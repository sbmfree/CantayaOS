// Graphics Subsystem
//
// This module provides all visual output for CantayaOS, from low-level
// pixel rendering to the high-level window manager.
//
// Layer stack (bottom to top):
//   1. Framebuffer — raw pixel access to the GOP framebuffer
//   2. Font — built-in bitmap font for text rendering
//   3. Console — text console with scrolling (like Windows cmd.exe blue screen)
//   4. (Future) Window Manager — compositing, windows, taskbar, desktop
//
// The framebuffer is memory-mapped by the UEFI bootloader via GOP.
// No GPU driver is needed — we just write pixels to memory.
//
// In Windows NT, this stack is:
//   - Framebuffer → Dxgkrnl (Display miniport driver)  
//   - Font/Console → Bootvid (Boot Video driver)
//   - Window Manager → Win32k (Window manager kernel subsystem)

pub mod framebuffer;
pub mod font;
pub mod console;

use cantaya_shared::boot_info::FramebufferInfo;

/// Initialize the graphics subsystem.
pub fn init(fb_info: &FramebufferInfo) {
    framebuffer::init(fb_info);
    console::init();
    log::info!("Graphics subsystem initialized");
}
