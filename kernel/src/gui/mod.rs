//! GUI subsystem — compositor thread, init, and module declarations.
//!
//! The compositor runs as a kernel thread at ~30 fps. Each frame it:
//! 1. Drains input events and dispatches them to the window manager
//! 2. Redraws: background → windows → taskbar → cursor
//! 3. Flushes the back buffer to the front buffer

pub mod cursor;
pub mod desktop;
pub mod event;
pub mod window;

use crate::drivers::framebuffer;
use event::MouseState;
use window::WM;

extern crate alloc;

/// Initialise the GUI subsystem and spawn the compositor thread.
pub fn init() {
    crate::kprintln!("[gui] starting compositor...");

    // Spawn compositor as a kernel task (fn() -> !)
    use crate::process::scheduler::PRIORITY_NORMAL;
    if let Some(pid) = crate::process::spawn_kernel_task("compositor", compositor_entry, PRIORITY_NORMAL) {
        crate::kprintln!("[gui] compositor thread PID {}", pid);
    } else {
        crate::kprintln!("[gui] ERROR: failed to spawn compositor");
    }
}

/// Entry point for the compositor kernel thread.
fn compositor_entry() -> ! {
    // Wait a moment for framebuffer to be ready
    crate::process::scheduler::sleep_current_ms(100);

    let mut mouse = MouseState::new();

    // Create welcome windows via global WM
    {
        let mut wm = WM.lock();
        let w1 = wm.create_window("Welcome", 150, 100, 400, 200);
        wm.set_content(w1, "Welcome to CantayaOS!\n\nThis is a graphical desktop.\nDrag windows by their title bar.\nClick X to close.\n\nEnjoy!");

        let w2 = wm.create_window("System Info", 300, 200, 350, 150);
        let ram_mb = crate::mm::physical::total_memory() / 1024 / 1024;
        let info = alloc::format!("CantayaOS v0.1\nArch: AArch64 (Cortex-A72)\nRAM: {} MB\nDisplay: 800x600 XRGB8888\nInput: virtio-input", ram_mb);
        wm.set_content(w2, &info);
    }

    // Compositor loop
    loop {
        // 1. Drain input events
        let events = event::drain_input(&mut mouse);

        // 2. Dispatch to window manager
        {
            let mut wm = WM.lock();
            for ev in &events {
                wm.handle_event(ev);
            }
        }

        // 3. Redraw everything into back buffer
        {
            let mut fb = framebuffer::FB.lock();
            let wm = WM.lock();

            // Background
            desktop::draw_background(&mut fb);

            // Windows (bottom to top)
            wm.draw_all(&mut fb);

            // Taskbar
            desktop::draw_taskbar(&mut fb, wm.count());

            // Cursor on top
            cursor::draw_cursor(&mut fb, mouse.x, mouse.y);

            // 4. Flush to front buffer
            fb.flush();
        }

        // ~30 fps
        crate::process::scheduler::sleep_current_ms(33);
    }
}
