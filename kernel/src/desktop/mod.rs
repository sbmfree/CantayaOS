// CantayaOS Desktop Environment
//
// A graphical desktop environment with a window manager, taskbar, start menu,
// and built-in applications. Inspired by Windows 95/98/2000 aesthetics.
//
// Architecture:
//   Desktop (background + icons)
//     └── Window Manager (manages windows, focus, z-order)
//           ├── Taskbar (always on top, bottom of screen)
//           │     ├── Start Button
//           │     ├── Running App Buttons
//           │     └── Clock
//           ├── Start Menu (overlay when Start is active)
//           └── Application Windows
//
// Navigation is fully keyboard-driven:
//   - Tab/Shift+Tab: cycle through windows/icons
//   - Arrow keys: move within app / navigate menus
//   - Enter: activate / select
//   - Alt+F4 / Escape: close window / exit desktop
//   - F1: open Start menu

pub mod wm;
pub mod taskbar;
pub mod apps;
mod drawing;
mod icons;
mod cursor;
mod input;
mod wallpaper;

pub use drawing::{draw_text, draw_text_bg, draw_raised_rect, draw_sunken_rect, fill_rect, draw_icon_16, present, screen_size};
pub use icons::*;
pub use input::{InputResult, MouseClick};

extern crate alloc;

use alloc::vec::Vec;

use crate::graphics::framebuffer::{Color, FRAMEBUFFER};
use crate::graphics::font::{CHAR_WIDTH, CHAR_HEIGHT};
use crate::hal::keyboard::{self, KeyEvent};

// ============================================================================
// Color Palette (Windows 95/2000 inspired)
// ============================================================================

pub const DESKTOP_BG: Color = Color::rgb(0, 128, 128);        // Classic teal
pub const TASKBAR_BG: Color = Color::rgb(0xC0, 0xC0, 0xC0);   // Silver
pub const TASKBAR_BORDER: Color = Color::rgb(0xFF, 0xFF, 0xFF);
pub const START_BTN_BG: Color = Color::rgb(0xC0, 0xC0, 0xC0);
pub const START_BTN_TEXT: Color = Color::rgb(0, 0, 0);
pub const WINDOW_BG: Color = Color::rgb(0xC0, 0xC0, 0xC0);    // Window chrome
pub const WINDOW_CLIENT: Color = Color::rgb(0xFF, 0xFF, 0xFF); // Client area
pub const TITLE_ACTIVE: Color = Color::rgb(0x00, 0x00, 0x80);  // Active title bar
pub const TITLE_INACTIVE: Color = Color::rgb(0x80, 0x80, 0x80);
pub const TITLE_TEXT: Color = Color::rgb(0xFF, 0xFF, 0xFF);
pub const MENU_BG: Color = Color::rgb(0xC0, 0xC0, 0xC0);
pub const MENU_HIGHLIGHT: Color = Color::rgb(0x00, 0x00, 0x80);
pub const MENU_TEXT: Color = Color::rgb(0, 0, 0);
pub const MENU_TEXT_HI: Color = Color::rgb(0xFF, 0xFF, 0xFF);
pub const SHADOW: Color = Color::rgb(0x80, 0x80, 0x80);
pub const HIGHLIGHT: Color = Color::rgb(0xFF, 0xFF, 0xFF);
pub const ICON_TEXT: Color = Color::rgb(0xFF, 0xFF, 0xFF);
pub const CLOCK_TEXT: Color = Color::rgb(0, 0, 0);
pub const BUTTON_FACE: Color = Color::rgb(0xC0, 0xC0, 0xC0);
pub const BUTTON_SHADOW: Color = Color::rgb(0x80, 0x80, 0x80);
pub const BUTTON_HIGHLIGHT: Color = Color::rgb(0xFF, 0xFF, 0xFF);

/// Taskbar height in pixels
pub const TASKBAR_HEIGHT: u32 = 28;

/// Title bar height in pixels
pub const TITLEBAR_HEIGHT: u32 = 20;

/// Window border width
pub const BORDER_WIDTH: u32 = 3;

// ============================================================================
// Desktop Entry Point
// ============================================================================

/// Desktop icon descriptor
struct DesktopIcon {
    name: &'static str,
    icon: &'static [u16; 16],
    app_id: apps::AppId,
    x: u32,
    y: u32,
}

/// Desktop state
struct DesktopState {
    icons: Vec<DesktopIcon>,
    selected_icon: Option<usize>,
    start_menu_open: bool,
    start_menu_selection: usize,
    screen_w: u32,
    screen_h: u32,
}

impl DesktopState {
    fn new(screen_w: u32, screen_h: u32) -> Self {
        let icon_spacing_y = 70;
        let icon_x = 20;
        let icon_start_y = 20;

        let icons = alloc::vec![
            DesktopIcon {
                name: "System Info",
                icon: &ICON_COMPUTER,
                app_id: apps::AppId::SystemInfo,
                x: icon_x,
                y: icon_start_y,
            },
            DesktopIcon {
                name: "Task Manager",
                icon: &ICON_COMPUTER,
                app_id: apps::AppId::TaskManager,
                x: icon_x,
                y: icon_start_y + icon_spacing_y,
            },
            DesktopIcon {
                name: "Notepad",
                icon: &ICON_NOTEPAD,
                app_id: apps::AppId::Notepad,
                x: icon_x,
                y: icon_start_y + icon_spacing_y * 2,
            },
            DesktopIcon {
                name: "Calculator",
                icon: &ICON_CALC,
                app_id: apps::AppId::Calculator,
                x: icon_x,
                y: icon_start_y + icon_spacing_y * 3,
            },
            DesktopIcon {
                name: "About",
                icon: &ICON_INFO,
                app_id: apps::AppId::About,
                x: icon_x,
                y: icon_start_y + icon_spacing_y * 4,
            },
            DesktopIcon {
                name: "Terminal",
                icon: &ICON_TERMINAL,
                app_id: apps::AppId::Terminal,
                x: icon_x,
                y: icon_start_y + icon_spacing_y * 5,
            },
            DesktopIcon {
                name: "Files",
                icon: &ICON_FOLDER,
                app_id: apps::AppId::FileBrowser,
                x: icon_x,
                y: icon_start_y + icon_spacing_y * 6,
            },
            DesktopIcon {
                name: "Paint",
                icon: &ICON_PAINT,
                app_id: apps::AppId::Paint,
                x: icon_x,
                y: icon_start_y + icon_spacing_y * 7,
            },
            DesktopIcon {
                name: "Minesweeper",
                icon: &ICON_MINE,
                app_id: apps::AppId::Minesweeper,
                x: icon_x,
                y: icon_start_y + icon_spacing_y * 8,
            },
            DesktopIcon {
                name: "Snake",
                icon: &ICON_SNAKE,
                app_id: apps::AppId::Snake,
                x: icon_x,
                y: icon_start_y + icon_spacing_y * 9,
            },
            DesktopIcon {
                name: "Settings",
                icon: &ICON_SETTINGS,
                app_id: apps::AppId::Settings,
                x: icon_x,
                y: icon_start_y + icon_spacing_y * 10,
            },
            DesktopIcon {
                name: "Clock",
                icon: &ICON_CLOCK,
                app_id: apps::AppId::Clock,
                x: icon_x,
                y: icon_start_y + icon_spacing_y * 11,
            },
        ];

        Self {
            icons,
            selected_icon: None,
            start_menu_open: false,
            start_menu_selection: 0,
            screen_w,
            screen_h,
        }
    }
}

/// Enter the desktop environment. Returns when the user exits (Escape at desktop).
pub fn run() {
    let (sw, sh) = screen_size();
    let mut desktop = DesktopState::new(sw, sh);
    let mut window_mgr = wm::WindowManager::new(sw, sh);
    let mut mouse_tracker = input::MouseTracker::new(sw, sh);
    let mut cursor_save = cursor::CursorSave::new();

    // Initial full draw
    draw_desktop(&desktop);
    taskbar::draw(&window_mgr, sw, sh);
    cursor::save_under_cursor(&mut cursor_save, mouse_tracker.x(), mouse_tracker.y());
    cursor::draw_cursor(mouse_tracker.x(), mouse_tracker.y());
    present();

    loop {
        let mut needs_full_redraw = false;
        let mut should_exit = false;

        // --- Process mouse events ---
        let (mouse_moved, mouse_click) = mouse_tracker.process_events(sw, sh);

        // Handle mouse button events
        if let Some(click) = mouse_click {
            match click {
                MouseClick::LeftDown => {
                    let mx = mouse_tracker.x();
                    let my = mouse_tracker.y();
                    match input::handle_mouse_click(&mut desktop, &mut window_mgr, mx, my, sw, sh) {
                        InputResult::ExitDesktop => { should_exit = true; }
                        InputResult::Redraw => { needs_full_redraw = true; }
                        InputResult::Continue => {}
                    }
                }
                MouseClick::LeftUp => {
                    if window_mgr.is_dragging() {
                        window_mgr.handle_mouse_up();
                        needs_full_redraw = true;
                    }
                }
                MouseClick::RightDown => {
                    // Right-click on empty desktop = unfocus windows
                    let mx = mouse_tracker.x();
                    let my = mouse_tracker.y();
                    let tb_y = sh - TASKBAR_HEIGHT;
                    if my < tb_y {
                        // Only unfocus if not clicking a window
                        if window_mgr.handle_click(mx, my).is_none() {
                            window_mgr.unfocus_all();
                            desktop.selected_icon = None;
                            needs_full_redraw = true;
                        } else {
                            needs_full_redraw = true;
                        }
                    }
                }
                MouseClick::RightUp => {}
            }
        }

        // Handle continuous mouse movement during drag/resize
        if mouse_moved && window_mgr.is_dragging() {
            let mx = mouse_tracker.x();
            let my = mouse_tracker.y();
            if window_mgr.handle_mouse_move(mx, my) {
                needs_full_redraw = true;
            }
        }

        // --- Process keyboard events ---
        if let Some(event) = keyboard::try_read_char() {
            let result = if desktop.start_menu_open {
                input::handle_start_menu_input(&mut desktop, &mut window_mgr, &event)
            } else if window_mgr.has_focused_window() {
                window_mgr.handle_input(&event)
            } else {
                input::handle_desktop_input(&mut desktop, &mut window_mgr, &event)
            };
            match result {
                InputResult::ExitDesktop => { should_exit = true; }
                InputResult::Redraw => { needs_full_redraw = true; }
                InputResult::Continue => {}
            }
        }

        if should_exit { return; }

        // --- Clock refresh (~1 second) ---
        let now = crate::shell::ticks();
        static mut LAST_CLOCK_TICK: u64 = 0;
        let clock_tick = unsafe {
            if now.wrapping_sub(LAST_CLOCK_TICK) >= 1000 {
                LAST_CLOCK_TICK = now;
                true
            } else {
                false
            }
        };

        // --- Render (consolidated, single present per iteration) ---
        if needs_full_redraw {
            // Full scene redraw — everything gets repainted
            cursor::restore_cursor_save(&mut cursor_save);
            draw_desktop(&desktop);
            if desktop.start_menu_open {
                draw_start_menu(&desktop);
            }
            window_mgr.draw_all();
            taskbar::draw(&window_mgr, sw, sh);
            cursor::save_under_cursor(&mut cursor_save, mouse_tracker.x(), mouse_tracker.y());
            cursor::draw_cursor(mouse_tracker.x(), mouse_tracker.y());
            present();
        } else if mouse_moved {
            // Fast path: only the cursor changed position.
            // Restore old cursor pixels, optionally refresh clock,
            // save new pixels, draw cursor. Dirty rect is tiny (~1 KB).
            cursor::restore_cursor_save(&mut cursor_save);
            if clock_tick {
                taskbar::draw(&window_mgr, sw, sh);
            }
            cursor::save_under_cursor(&mut cursor_save, mouse_tracker.x(), mouse_tracker.y());
            cursor::draw_cursor(mouse_tracker.x(), mouse_tracker.y());
            present();
        } else if clock_tick {
            // Just refresh the clock area
            cursor::restore_cursor_save(&mut cursor_save);
            taskbar::draw(&window_mgr, sw, sh);
            cursor::save_under_cursor(&mut cursor_save, mouse_tracker.x(), mouse_tracker.y());
            cursor::draw_cursor(mouse_tracker.x(), mouse_tracker.y());
            present();
        } else {
            // Nothing to do — sleep until next interrupt
            unsafe { core::arch::asm!("hlt"); }
        }
    }
}

// ============================================================================
// Drawing
// ============================================================================

/// Draw the desktop background and icons.
fn draw_desktop(desktop: &DesktopState) {
    let (sw, sh) = screen_size();
    let desk_h = sh - TASKBAR_HEIGHT;

    // Draw wallpaper pattern instead of flat fill
    wallpaper::draw_wallpaper(sw, desk_h);

    // Draw desktop icons
    for (i, icon) in desktop.icons.iter().enumerate() {
        let selected = desktop.selected_icon == Some(i);
        draw_desktop_icon(icon, selected);
    }
}

/// Draw a single desktop icon with label.
fn draw_desktop_icon(icon: &DesktopIcon, selected: bool) {
    let ix = icon.x;
    let iy = icon.y;

    // Icon background highlight if selected
    if selected {
        fill_rect(ix - 2, iy - 2, 20, 20, TITLE_ACTIVE);
    }

    // Draw the 16x16 icon
    draw_icon_16(ix, iy, icon.icon, Color::WHITE, Some(if selected { TITLE_ACTIVE } else { DESKTOP_BG }));

    // Label (centered under icon)
    let label_x = if icon.name.len() > 6 {
        ix.saturating_sub(((icon.name.len() - 2) as u32 * CHAR_WIDTH) / 2)
    } else {
        ix
    };
    let label_y = iy + 20;

    if selected {
        // Selected: highlight background
        let label_w = icon.name.len() as u32 * CHAR_WIDTH + 4;
        fill_rect(label_x.saturating_sub(2), label_y, label_w, CHAR_HEIGHT, TITLE_ACTIVE);
        draw_text(label_x, label_y, icon.name, Color::WHITE);
    } else {
        draw_text(label_x, label_y, icon.name, ICON_TEXT);
    }
}

/// Start menu items: (label, app_id or None for "Exit")
fn start_menu_items() -> Vec<(&'static str, Option<apps::AppId>)> {
    alloc::vec![
        ("  System Info",   Some(apps::AppId::SystemInfo)),
        ("  Task Manager",  Some(apps::AppId::TaskManager)),
        ("  Notepad",       Some(apps::AppId::Notepad)),
        ("  Calculator",    Some(apps::AppId::Calculator)),
        ("  About CantayaOS", Some(apps::AppId::About)),
        ("  Terminal",      Some(apps::AppId::Terminal)),
        ("  File Browser",  Some(apps::AppId::FileBrowser)),
        ("  Paint",         Some(apps::AppId::Paint)),
        ("  Minesweeper",   Some(apps::AppId::Minesweeper)),
        ("  Snake",         Some(apps::AppId::Snake)),
        ("  Settings",      Some(apps::AppId::Settings)),
        ("  Clock",         Some(apps::AppId::Clock)),
        ("  ─────────────", None), // separator (will skip in input)
        ("  Exit to Shell", None),
    ]
}

/// Draw the start menu popup.
fn draw_start_menu(desktop: &DesktopState) {
    let items = start_menu_items();
    let item_h = CHAR_HEIGHT + 4;
    let menu_w: u32 = 200;
    let menu_h = items.len() as u32 * item_h + 4;
    let (_, sh) = screen_size();
    let menu_x: u32 = 2;
    let menu_y = sh - TASKBAR_HEIGHT - menu_h;

    // Menu background with 3D border
    fill_rect(menu_x, menu_y, menu_w, menu_h, MENU_BG);
    draw_raised_rect(menu_x, menu_y, menu_w, menu_h);

    // Side banner (Windows 95 style blue/gray strip on the left)
    fill_rect(menu_x + 2, menu_y + 2, 22, menu_h - 4, TITLE_ACTIVE);
    // Vertical "CantayaOS" text would go here, but let's just put a colored strip

    // Menu items
    for (i, (label, _)) in items.iter().enumerate() {
        let iy = menu_y + 2 + i as u32 * item_h;
        let ix = menu_x + 26;

        if *label == "  ─────────────" {
            // Separator line
            {
                let mut fb = FRAMEBUFFER.lock();
                fb.fill_rect(menu_x + 26, iy + item_h / 2, menu_w - 30, 1, SHADOW);
                fb.fill_rect(menu_x + 26, iy + item_h / 2 + 1, menu_w - 30, 1, HIGHLIGHT);
            }
            continue;
        }

        let selected = desktop.start_menu_selection == i;
        if selected {
            fill_rect(ix - 2, iy, menu_w - 28, item_h, MENU_HIGHLIGHT);
            draw_text(ix, iy + 2, label, MENU_TEXT_HI);
        } else {
            draw_text(ix, iy + 2, label, MENU_TEXT);
        }
    }
}
