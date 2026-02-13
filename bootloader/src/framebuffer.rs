// Framebuffer Initialization — UEFI GOP (Graphics Output Protocol)
//
// GOP is UEFI's standard interface for display output. It provides a linear
// framebuffer — a simple memory region where each pixel is a contiguous value.
//
// This replaces the legacy VGA BIOS interface and works with modern displays.
// The framebuffer persists after ExitBootServices(), so the kernel can use it
// for display output without needing a GPU driver.
//
// Design Decision:
//   We prefer a high-resolution mode (1024x768 or higher) with 32-bit pixels.
//   If the preferred mode isn't available, we fall back to the current mode.

use cantaya_shared::boot_info::{FramebufferInfo, PixelFormat};
use log::info;
use uefi::boot;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat as GopPixelFormat};

/// Initialize the UEFI Graphics Output Protocol and return framebuffer info.
///
/// This function:
///   1. Locates the GOP protocol handle
///   2. Tries to set a preferred resolution (1280x720 or higher)
///   3. Falls back to the current mode if preferred isn't available
///   4. Returns the framebuffer address, dimensions, and pixel format
pub fn initialize_gop() -> FramebufferInfo {
    // Open the GOP protocol
    let gop_handle = boot::get_handle_for_protocol::<GraphicsOutput>()
        .expect("UEFI GOP not available — cannot initialize display");

    let mut gop = boot::open_protocol_exclusive::<GraphicsOutput>(gop_handle)
        .expect("Failed to open GOP protocol");

    // Try to find a good video mode (prefer 1280x720 or 1024x768 at 32bpp)
    let preferred_mode = find_preferred_mode(&gop);
    if let Some(mode) = preferred_mode {
        gop.set_mode(&mode).expect("Failed to set video mode");
        info!("Set video mode: {}x{}", mode.info().resolution().0, mode.info().resolution().1);
    } else {
        info!("Using current video mode");
    }

    // Get the framebuffer info from the current mode
    let mode_info = gop.current_mode_info();
    let (width, height) = mode_info.resolution();
    let stride = mode_info.stride();

    let pixel_format = match mode_info.pixel_format() {
        GopPixelFormat::Bgr => PixelFormat::Bgr,
        GopPixelFormat::Rgb => PixelFormat::Rgb,
        _ => PixelFormat::Unknown,
    };

    let mut fb = gop.frame_buffer();

    FramebufferInfo {
        address: fb.as_mut_ptr() as u64,
        size: fb.size() as u64,
        width: width as u32,
        height: height as u32,
        stride: stride as u32,
        pixel_format,
    }
}

/// Search for a preferred video mode.
///
/// We prefer modes with:
///   - Resolution >= 1024x768
///   - 32-bit BGR or RGB pixel format (not bitmask or BltOnly)
///   - Landscape orientation
fn find_preferred_mode(gop: &GraphicsOutput) -> Option<uefi::proto::console::gop::Mode> {
    let preferred_resolutions = [
        (1920, 1080),
        (1280, 720),
        (1024, 768),
    ];

    for &(target_w, target_h) in &preferred_resolutions {
        for mode in gop.modes() {
            let info = mode.info();
            let (w, h) = info.resolution();

            if w == target_w
                && h == target_h
                && matches!(
                    info.pixel_format(),
                    GopPixelFormat::Bgr | GopPixelFormat::Rgb
                )
            {
                return Some(mode);
            }
        }
    }

    None
}
