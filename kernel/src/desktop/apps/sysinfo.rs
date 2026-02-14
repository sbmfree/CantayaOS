// System Information Application

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::desktop::{InputResult, draw_text};
use crate::desktop::wm::Window;
use crate::graphics::framebuffer::Color;
use crate::graphics::font::{CHAR_WIDTH, CHAR_HEIGHT};
use crate::hal::keyboard::{KeyCode, KeyEvent};

pub struct SysInfoState {
    pub(super) lines: Vec<String>,
    pub(super) scroll: usize,
}

impl SysInfoState {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            scroll: 0,
        }
    }
}

pub(super) fn sysinfo_init(state: &mut SysInfoState) {
    let mut lines = Vec::new();

    // Header
    lines.push(String::from("  === CantayaOS System Information ==="));
    lines.push(String::new());

    // Kernel version
    let mut s = String::new();
    write!(s, "  Kernel:     CantayaOS v{}", env!("CARGO_PKG_VERSION")).ok();
    lines.push(s);
    lines.push(String::from("  Arch:       x86_64 (AMD64)"));
    lines.push(String::from("  Build:      Rust nightly, no_std"));
    lines.push(String::new());

    // Memory
    let free_frames = crate::memory::frame_allocator::free_frame_count();
    let total_frames = crate::memory::frame_allocator::total_frame_count();
    let used_frames = total_frames - free_frames;

    let mut s = String::new();
    write!(s, "  Total RAM:  {} KiB ({} frames)", total_frames * 4, total_frames).ok();
    lines.push(s);
    let mut s = String::new();
    write!(s, "  Used:       {} KiB ({} frames)", used_frames * 4, used_frames).ok();
    lines.push(s);
    let mut s = String::new();
    write!(s, "  Free:       {} KiB ({} frames)", free_frames * 4, free_frames).ok();
    lines.push(s);
    lines.push(String::new());

    // Uptime
    let ticks = crate::shell::ticks();
    let ms = crate::hal::pit::ticks_to_ms(ticks);
    let secs = ms / 1000;
    let mins = secs / 60;
    let hours = mins / 60;
    let mut s = String::new();
    write!(s, "  Uptime:     {}h {}m {}s", hours, mins % 60, secs % 60).ok();
    lines.push(s);
    lines.push(String::new());

    // CPU info
    lines.push(String::from("  CPU:        x86_64 (AMD64)"));
    let mut s = String::new();
    let cr0 = crate::hal::cpu::read_cr0();
    write!(s, "  CR0:        {:#010X}", cr0).ok();
    lines.push(s);
    let mut s = String::new();
    let cr4 = crate::hal::cpu::read_cr4();
    write!(s, "  CR4:        {:#010X}", cr4).ok();
    lines.push(s);
    lines.push(String::new());

    // PCI Devices
    let pci_count = crate::hal::pci::device_count();
    let mut s = String::new();
    write!(s, "  PCI Devices: {}", pci_count).ok();
    lines.push(s);

    let devices = crate::hal::pci::device_list();
    for dev in devices.iter().take(10) {
        let mut s = String::new();
        write!(s, "    {:02X}:{:02X}.{} {:04X}:{:04X} class={:02X}.{:02X}",
            dev.bus, dev.device, dev.function,
            dev.vendor_id, dev.device_id,
            dev.class_code, dev.subclass
        ).ok();
        lines.push(s);
    }

    lines.push(String::new());
    lines.push(String::from("  [Up/Down to scroll]"));

    state.lines = lines;
}

pub(super) fn sysinfo_draw(win: &Window, state: &SysInfoState) {
    let cx = win.client_x();
    let cy = win.client_y();
    let cw = win.client_width();
    let ch = win.client_height();

    let visible_lines = (ch / CHAR_HEIGHT) as usize;

    for (i, line) in state.lines.iter().skip(state.scroll).take(visible_lines).enumerate() {
        let y = cy + i as u32 * CHAR_HEIGHT;
        let max_chars = (cw / CHAR_WIDTH) as usize;
        let display: &str = if line.len() > max_chars { &line[..max_chars] } else { line };
        draw_text(cx, y, display, Color::BLACK);
    }
}

pub(super) fn sysinfo_input(state: &mut SysInfoState, event: &KeyEvent) -> InputResult {
    match event.key {
        KeyCode::Up => {
            if state.scroll > 0 {
                state.scroll -= 1;
            }
            InputResult::Redraw
        }
        KeyCode::Down => {
            if state.scroll + 1 < state.lines.len() {
                state.scroll += 1;
            }
            InputResult::Redraw
        }
        KeyCode::Home => {
            state.scroll = 0;
            InputResult::Redraw
        }
        KeyCode::End => {
            if state.lines.len() > 5 {
                state.scroll = state.lines.len() - 5;
            }
            InputResult::Redraw
        }
        _ => InputResult::Continue,
    }
}
