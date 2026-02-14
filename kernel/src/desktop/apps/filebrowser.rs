// File Browser Application

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::desktop::{InputResult, draw_text, fill_rect};
use crate::desktop::wm::Window;
use crate::graphics::framebuffer::Color;
use crate::graphics::font::{CHAR_WIDTH, CHAR_HEIGHT};
use crate::hal::keyboard::{KeyCode, KeyEvent};

pub struct FileBrowserState {
    pub(super) current_path: String,
    pub(super) entries: Vec<FileBrowserEntry>,
    pub(super) selected: usize,
    pub(super) scroll: usize,
    status_msg: String,
    preview: Vec<String>,
}

pub(super) struct FileBrowserEntry {
    pub(super) name: String,
    pub(super) is_dir: bool,
    pub(super) size: usize,
}

impl FileBrowserState {
    pub fn new() -> Self {
        Self {
            current_path: String::from("/"),
            entries: Vec::new(),
            selected: 0,
            scroll: 0,
            status_msg: String::from("Enter:Open  Backspace:Up  D:Delete  R:Refresh"),
            preview: Vec::new(),
        }
    }
}

pub(super) fn filebrowser_init(state: &mut FileBrowserState) {
    filebrowser_refresh(state);
}

fn filebrowser_refresh(state: &mut FileBrowserState) {
    use crate::storage::vfs;
    state.entries.clear();
    state.preview.clear();

    if !vfs::is_ready() {
        state.status_msg = String::from("No filesystem mounted!");
        return;
    }

    // Add parent directory entry if not root
    if state.current_path != "/" {
        state.entries.push(FileBrowserEntry {
            name: String::from(".."),
            is_dir: true,
            size: 0,
        });
    }

    if let Some(dir_entries) = vfs::list_dir(&state.current_path) {
        // Sort: directories first, then files
        let mut dirs: Vec<_> = dir_entries.iter().filter(|e| e.is_dir).collect();
        let mut files: Vec<_> = dir_entries.iter().filter(|e| !e.is_dir).collect();
        dirs.sort_by(|a, b| a.name.cmp(&b.name));
        files.sort_by(|a, b| a.name.cmp(&b.name));

        for d in dirs {
            state.entries.push(FileBrowserEntry {
                name: d.name.clone(),
                is_dir: true,
                size: 0,
            });
        }
        for f in files {
            let full = if state.current_path == "/" {
                alloc::format!("/{}", f.name)
            } else {
                alloc::format!("{}/{}", state.current_path, f.name)
            };
            let size = vfs::read_file(&full).map(|d| d.len()).unwrap_or(0);
            state.entries.push(FileBrowserEntry {
                name: f.name.clone(),
                is_dir: false,
                size,
            });
        }
    }

    state.selected = 0;
    state.scroll = 0;
    state.status_msg.clear();
    write!(state.status_msg, "{} - {} items", state.current_path, state.entries.len()).ok();
}

pub(super) fn filebrowser_draw(win: &Window, state: &FileBrowserState) {
    let cx = win.client_x();
    let cy = win.client_y();
    let cw = win.client_width();
    let ch = win.client_height();

    // Header — current path
    fill_rect(cx, cy, cw, CHAR_HEIGHT + 2, Color::rgb(0x00, 0x00, 0x80));
    let mut header = String::new();
    write!(header, " {}", state.current_path).ok();
    draw_text(cx + 2, cy + 1, &header, Color::WHITE);

    // File list
    let list_y = cy + CHAR_HEIGHT + 4;
    let max_cols = (cw / CHAR_WIDTH) as usize;
    
    // Reserve space for preview and status
    let list_height = ch.saturating_sub(CHAR_HEIGHT * 2 + 10);
    let visible = (list_height / CHAR_HEIGHT) as usize;

    for (i, entry) in state.entries.iter().skip(state.scroll).take(visible).enumerate() {
        let y = list_y + i as u32 * CHAR_HEIGHT;
        let actual_idx = state.scroll + i;
        let is_selected = actual_idx == state.selected;

        if is_selected {
            fill_rect(cx, y, cw, CHAR_HEIGHT, Color::rgb(0x00, 0x00, 0x80));
        }

        let mut line = String::new();
        if entry.is_dir {
            write!(line, " [DIR]  {}/", entry.name).ok();
        } else {
            if entry.size < 1024 {
                write!(line, " {:>5}B  {}", entry.size, entry.name).ok();
            } else {
                write!(line, " {:>4}KB  {}", entry.size / 1024, entry.name).ok();
            }
        }

        if line.len() > max_cols { line.truncate(max_cols); }

        let fg = if is_selected {
            Color::WHITE
        } else if entry.is_dir {
            Color::rgb(0x00, 0x00, 0xAA)
        } else {
            Color::BLACK
        };

        draw_text(cx + 2, y, &line, fg);
    }

    // Status bar at bottom
    let status_y = cy + ch - CHAR_HEIGHT - 2;
    fill_rect(cx, status_y, cw, CHAR_HEIGHT + 2, Color::rgb(0xE0, 0xE0, 0xE0));
    draw_text(cx + 2, status_y + 1, &state.status_msg, Color::rgb(0x40, 0x40, 0x40));
}

pub(super) fn filebrowser_input(state: &mut FileBrowserState, event: &KeyEvent) -> InputResult {
    match event.key {
        KeyCode::Up => {
            if state.selected > 0 {
                state.selected -= 1;
                if state.selected < state.scroll {
                    state.scroll = state.selected;
                }
            }
            return InputResult::Redraw;
        }
        KeyCode::Down => {
            if state.selected + 1 < state.entries.len() {
                state.selected += 1;
            }
            return InputResult::Redraw;
        }
        KeyCode::Enter => {
            if let Some(entry) = state.entries.get(state.selected) {
                if entry.name == ".." {
                    // Go up
                    if let Some(pos) = state.current_path.rfind('/') {
                        if pos == 0 {
                            state.current_path = String::from("/");
                        } else {
                            state.current_path.truncate(pos);
                        }
                    }
                    filebrowser_refresh(state);
                } else if entry.is_dir {
                    // Enter directory
                    if state.current_path == "/" {
                        state.current_path = alloc::format!("/{}", entry.name);
                    } else {
                        let new_path = alloc::format!("{}/{}", state.current_path, entry.name);
                        state.current_path = new_path;
                    }
                    filebrowser_refresh(state);
                } else {
                    // Preview file content
                    use crate::storage::vfs;
                    let full = if state.current_path == "/" {
                        alloc::format!("/{}", entry.name)
                    } else {
                        alloc::format!("{}/{}", state.current_path, entry.name)
                    };
                    state.status_msg.clear();
                    if let Some(data) = vfs::read_file(&full) {
                        if let Ok(text) = core::str::from_utf8(&data) {
                            write!(state.status_msg, "{} - {} bytes (text)", entry.name, data.len()).ok();
                        } else {
                            write!(state.status_msg, "{} - {} bytes (binary)", entry.name, data.len()).ok();
                        }
                    }
                }
            }
            return InputResult::Redraw;
        }
        KeyCode::Backspace => {
            // Go up one directory
            if state.current_path != "/" {
                if let Some(pos) = state.current_path.rfind('/') {
                    if pos == 0 {
                        state.current_path = String::from("/");
                    } else {
                        state.current_path.truncate(pos);
                    }
                }
                filebrowser_refresh(state);
            }
            return InputResult::Redraw;
        }
        _ => {}
    }

    match event.ascii {
        // 'D' or 'd' — delete selected file
        b'D' | b'd' => {
            if let Some(entry) = state.entries.get(state.selected) {
                if entry.name != ".." {
                    use crate::storage::vfs;
                    let full = if state.current_path == "/" {
                        alloc::format!("/{}", entry.name)
                    } else {
                        alloc::format!("{}/{}", state.current_path, entry.name)
                    };
                    if vfs::delete(&full) {
                        state.status_msg.clear();
                        write!(state.status_msg, "Deleted: {}", entry.name).ok();
                        filebrowser_refresh(state);
                        if state.selected >= state.entries.len() && state.selected > 0 {
                            state.selected -= 1;
                        }
                    } else {
                        state.status_msg.clear();
                        write!(state.status_msg, "Failed to delete: {}", entry.name).ok();
                    }
                }
            }
            InputResult::Redraw
        }
        // 'R' or 'r' — refresh
        b'R' | b'r' => {
            filebrowser_refresh(state);
            InputResult::Redraw
        }
        _ => InputResult::Continue,
    }
}
