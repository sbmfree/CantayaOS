// Settings Application

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::desktop::{InputResult, TITLE_ACTIVE, draw_text, fill_rect};
use crate::desktop::wm::Window;
use crate::graphics::framebuffer::Color;
use crate::graphics::font::{CHAR_WIDTH, CHAR_HEIGHT};
use crate::hal::keyboard::{KeyCode, KeyEvent};

pub struct SettingsState {
    selected: usize,
    items: Vec<SettingsItem>,
}

struct SettingsItem {
    label: String,
    kind: SettingsKind,
}

enum SettingsKind {
    Toggle(bool),
    Value(String),
}

impl SettingsState {
    pub fn new() -> Self {
        Self {
            selected: 0,
            items: Vec::new(),
        }
    }
}

pub fn settings_init(state: &mut SettingsState) {
    state.items.clear();

    // Read current theme from disk
    let theme = if let Some(data) = crate::storage::vfs::read_file("/system/theme.cfg") {
        String::from(core::str::from_utf8(&data).unwrap_or("default").trim())
    } else {
        String::from("default")
    };
    state.items.push(SettingsItem {
        label: String::from("Console Theme"),
        kind: SettingsKind::Value(String::from(theme)),
    });

    // Memory info
    let free = crate::memory::frame_allocator::free_frame_count();
    let total = crate::memory::frame_allocator::total_frame_count();
    let mut mem_str = String::new();
    write!(mem_str, "{}/{} KiB", (total - free) * 4, total * 4).ok();
    state.items.push(SettingsItem {
        label: String::from("Memory Usage"),
        kind: SettingsKind::Value(mem_str),
    });

    // Uptime
    let ticks = crate::shell::ticks();
    let ms = crate::hal::pit::ticks_to_ms(ticks);
    let secs = ms / 1000;
    let mut up_str = String::new();
    write!(up_str, "{}s", secs).ok();
    state.items.push(SettingsItem {
        label: String::from("Uptime"),
        kind: SettingsKind::Value(up_str),
    });

    // Storage
    let disk_ready = crate::storage::vfs::is_ready();
    state.items.push(SettingsItem {
        label: String::from("Disk Mounted"),
        kind: SettingsKind::Toggle(disk_ready),
    });

    state.items.push(SettingsItem {
        label: String::from("Screen Resolution"),
        kind: SettingsKind::Value(String::from("1920x1080")),
    });

    state.items.push(SettingsItem {
        label: String::from("OS Version"),
        kind: SettingsKind::Value(String::from(env!("CARGO_PKG_VERSION"))),
    });
}

pub fn settings_draw(win: &Window, state: &SettingsState) {
    let cx = win.client_x();
    let cy = win.client_y();
    let cw = win.client_width();
    let ch = win.client_height();

    // Title bar
    fill_rect(cx, cy, cw, CHAR_HEIGHT + 4, TITLE_ACTIVE);
    draw_text(cx + 8, cy + 2, "  System Settings", Color::WHITE);

    let row_h = CHAR_HEIGHT + 8;
    let start_y = cy + CHAR_HEIGHT + 8;

    for (i, item) in state.items.iter().enumerate() {
        let y = start_y + i as u32 * row_h;
        if y + row_h > cy + ch { break; }

        let selected = i == state.selected;
        if selected {
            fill_rect(cx, y, cw, row_h, Color::rgb(0xD0, 0xD0, 0xFF));
        }

        draw_text(cx + 8, y + 4, &item.label, Color::BLACK);

        let val_str = match &item.kind {
            SettingsKind::Toggle(v) => if *v { "[ON]" } else { "[OFF]" },
            SettingsKind::Value(s) => s.as_str(),
        };

        // Right-align value
        let val_x = cx + cw - 8 - val_str.len() as u32 * CHAR_WIDTH;
        draw_text(val_x, y + 4, val_str, Color::rgb(0x00, 0x00, 0x80));
    }

    // Footer
    let footer_y = cy + ch - CHAR_HEIGHT - 4;
    draw_text(cx + 4, footer_y, "[Up/Down] Navigate  [R] Refresh", Color::rgb(0x80, 0x80, 0x80));
}

pub fn settings_input(state: &mut SettingsState, event: &KeyEvent) -> InputResult {
    match event.key {
        KeyCode::Up => {
            if state.selected > 0 { state.selected -= 1; }
            InputResult::Redraw
        }
        KeyCode::Down => {
            if state.selected + 1 < state.items.len() { state.selected += 1; }
            InputResult::Redraw
        }
        _ => {
            if event.ascii == b'r' || event.ascii == b'R' {
                settings_init(state);
                InputResult::Redraw
            } else {
                InputResult::Continue
            }
        }
    }
}

pub fn settings_click(state: &mut SettingsState, local_y: u32) -> InputResult {
    let header_h = CHAR_HEIGHT + 8;
    if local_y < header_h { return InputResult::Continue; }
    let row_h = CHAR_HEIGHT + 8;
    let row = ((local_y - header_h) / row_h) as usize;
    if row < state.items.len() {
        state.selected = row;
        InputResult::Redraw
    } else {
        InputResult::Continue
    }
}
