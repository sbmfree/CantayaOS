// CantayaOS Shell — In-Shell Text Editor

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use crate::graphics::console;
use crate::hal::keyboard::{self, KeyCode};
use core::fmt::Write;

pub(crate) fn cmd_edit(args: &str) {
    use crate::storage::vfs;

    if args.is_empty() {
        console::println("Usage: edit <file>");
        return;
    }

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let filename = args.trim();

    // Load existing file content or start empty
    let initial = vfs::read_file(filename).unwrap_or_default();
    let text = core::str::from_utf8(&initial).unwrap_or("");
    let mut lines: Vec<String> = if text.is_empty() {
        alloc::vec![String::new()]
    } else {
        text.lines().map(|l| l.into()).collect()
    };
    if lines.is_empty() { lines.push(String::new()); }

    let mut cursor_row: usize = 0;
    let mut cursor_col: usize = 0;
    let mut scroll_offset: usize = 0;
    let mut modified = false;
    let mut running = true;
    let mut message = String::from("Ctrl+S=Save  Ctrl+Q=Quit  Ctrl+G=Goto");

    let (screen_cols, screen_rows) = console::dimensions();
    let edit_rows = screen_rows.saturating_sub(2); // Reserve for header + footer

    while running {
        // Draw editor
        console::clear();

        // Header bar
        console::set_color(0x00, 0x00, 0x00);
        console::set_bg_color(0xAA, 0xAA, 0xAA);
        let mut header = String::new();
        write!(header, " EDIT: {} {} [{}/{}]",
            filename,
            if modified { "(modified)" } else { "" },
            cursor_row + 1,
            lines.len()
        ).ok();
        while header.len() < screen_cols { header.push(' '); }
        console::print(&header);
        console::print("\n");

        // Reset colors for content
        console::set_color(0xFF, 0xFF, 0xFF);
        console::set_bg_color(0x00, 0x00, 0x00);

        // Draw visible lines
        for i in 0..edit_rows {
            let line_idx = scroll_offset + i;
            if line_idx < lines.len() {
                let line = &lines[line_idx];
                let display = if line.len() > screen_cols - 5 {
                    &line[..screen_cols - 5]
                } else {
                    line.as_str()
                };
                // Line number
                let mut num = String::new();
                console::set_color(0x55, 0x55, 0x55);
                write!(num, "{:>3} ", line_idx + 1).ok();
                console::print(&num);
                console::set_color(0xFF, 0xFF, 0xFF);
                console::println(display);
            } else {
                console::set_color(0x55, 0x55, 0x55);
                console::println("  ~ ");
                console::set_color(0xFF, 0xFF, 0xFF);
            }
        }

        // Footer / status bar
        console::set_color(0x00, 0x00, 0x00);
        console::set_bg_color(0xAA, 0xAA, 0xAA);
        let mut footer = String::new();
        write!(footer, " {}", message).ok();
        while footer.len() < screen_cols { footer.push(' '); }
        console::print(&footer);

        // Reset colors
        console::set_color(0xFF, 0xFF, 0xFF);
        console::set_bg_color(0x00, 0x00, 0x00);

        // Wait for input
        let event = loop {
            if let Some(e) = keyboard::try_read_char() {
                break e;
            }
            unsafe { core::arch::asm!("hlt"); }
        };

        message.clear();
        write!(message, "Ctrl+S=Save  Ctrl+Q=Quit  Ctrl+G=Goto").ok();

        match event.ascii {
            // Ctrl+S — save
            0x13 => {
                let mut content = String::new();
                for (i, line) in lines.iter().enumerate() {
                    content.push_str(line);
                    if i < lines.len() - 1 {
                        content.push('\n');
                    }
                }
                if vfs::write_file(filename, content.as_bytes()) {
                    modified = false;
                    message.clear();
                    write!(message, "Saved {} bytes to '{}'", content.len(), filename).ok();
                    crate::hal::speaker::beep(880, 50);
                } else {
                    message.clear();
                    write!(message, "ERROR: Failed to save!").ok();
                    crate::hal::speaker::error_beep();
                }
            }

            // Ctrl+Q — quit
            0x11 => {
                if modified {
                    message.clear();
                    write!(message, "Unsaved changes! Press Ctrl+Q again to confirm quit.").ok();
                    // Redraw with message, wait for next key
                    console::clear();
                    console::set_color(0xFF, 0xFF, 0x55);
                    console::println(&message);
                    console::set_color(0xFF, 0xFF, 0xFF);

                    let confirm = loop {
                        if let Some(e) = keyboard::try_read_char() { break e; }
                        unsafe { core::arch::asm!("hlt"); }
                    };
                    if confirm.ascii == 0x11 {
                        running = false;
                    }
                } else {
                    running = false;
                }
            }

            // Ctrl+G — goto line
            0x07 => {
                message.clear();
                write!(message, "Goto line (type number then Enter):").ok();
                // Simple line number input — for now just jump to top/bottom
                cursor_row = 0;
                cursor_col = 0;
                scroll_offset = 0;
            }

            // Enter — insert new line
            b'\n' => {
                let current = &lines[cursor_row];
                let rest = current[cursor_col..].to_string();
                lines[cursor_row] = current[..cursor_col].to_string();
                lines.insert(cursor_row + 1, rest);
                cursor_row += 1;
                cursor_col = 0;
                modified = true;
            }

            // Backspace
            0x08 => {
                if cursor_col > 0 {
                    lines[cursor_row].remove(cursor_col - 1);
                    cursor_col -= 1;
                    modified = true;
                } else if cursor_row > 0 {
                    let current_line = lines.remove(cursor_row);
                    cursor_row -= 1;
                    cursor_col = lines[cursor_row].len();
                    lines[cursor_row].push_str(&current_line);
                    modified = true;
                }
            }

            // Tab
            b'\t' => {
                lines[cursor_row].insert_str(cursor_col, "    ");
                cursor_col += 4;
                modified = true;
            }

            // Printable character
            c @ 0x20..=0x7E => {
                lines[cursor_row].insert(cursor_col, c as char);
                cursor_col += 1;
                modified = true;
            }

            _ => {
                // Arrow keys
                match event.key {
                    KeyCode::Up => {
                        if cursor_row > 0 {
                            cursor_row -= 1;
                            cursor_col = cursor_col.min(lines[cursor_row].len());
                        }
                    }
                    KeyCode::Down => {
                        if cursor_row < lines.len() - 1 {
                            cursor_row += 1;
                            cursor_col = cursor_col.min(lines[cursor_row].len());
                        }
                    }
                    KeyCode::Left => {
                        if cursor_col > 0 {
                            cursor_col -= 1;
                        }
                    }
                    KeyCode::Right => {
                        if cursor_col < lines[cursor_row].len() {
                            cursor_col += 1;
                        }
                    }
                    _ => {}
                }
            }
        }

        // Adjust scroll
        if cursor_row < scroll_offset {
            scroll_offset = cursor_row;
        }
        if cursor_row >= scroll_offset + edit_rows {
            scroll_offset = cursor_row - edit_rows + 1;
        }
    }

    // Restore console
    console::clear();
    console::set_color(0xFF, 0xFF, 0xFF);
}
