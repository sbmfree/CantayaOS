//! Virtio-input driver for keyboard and tablet (absolute pointer) devices.
//!
//! virtio-input (device ID 18) exposes an event virtqueue (queue 0) that
//! delivers `VirtioInputEvent` structs (8 bytes each). The driver pre-fills
//! the queue with device-writable buffers; on IRQ the device has written
//! events into them.
//!
//! Tablet events give absolute coordinates in [0, 32767] which we scale
//! to the framebuffer resolution.

use core::ptr;
use crate::sync::IrqMutex;
use crate::drivers::virtio_mmio::{self, Virtqueue, VIRTIO_DEV_INPUT};
use crate::mm::physical;

extern crate alloc;
use alloc::collections::VecDeque;
use alloc::vec::Vec;

// ---------------------------------------------------------------------------
// Linux input event types / codes (subset we need)
// ---------------------------------------------------------------------------
const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
#[allow(dead_code)]
const EV_REL: u16 = 0x02;
const EV_ABS: u16 = 0x03;

const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;

const BTN_LEFT: u16   = 0x110;
const BTN_RIGHT: u16  = 0x111;
const BTN_MIDDLE: u16 = 0x112;
const BTN_TOUCH: u16  = 0x14A;

// Virtio input config select values
const VIRTIO_INPUT_CFG_ID_NAME: u8 = 0x01;

// ---------------------------------------------------------------------------
// Virtio input event (8 bytes, device writes these to our buffers)
// ---------------------------------------------------------------------------
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct VirtioInputEvent {
    typ:   u16,
    code:  u16,
    value: u32,
}

// ---------------------------------------------------------------------------
// Our high-level input event
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, Debug)]
pub enum InputEvent {
    /// Absolute pointer moved to (x, y) in screen coordinates.
    MouseMove { x: u32, y: u32 },
    /// Mouse button pressed/released. button=0 left, 1 right, 2 middle.
    MouseButton { button: u8, pressed: bool },
    /// Key press/release. `code` is Linux key code, `pressed` true=down.
    Key { code: u16, pressed: bool },
}

// ---------------------------------------------------------------------------
// Global event queue
// ---------------------------------------------------------------------------
static INPUT_QUEUE: IrqMutex<VecDeque<InputEvent>> = IrqMutex::new(VecDeque::new());

/// Poll the next input event (returns None if empty).
pub fn poll_event() -> Option<InputEvent> {
    INPUT_QUEUE.lock().pop_front()
}

/// Check if there are pending events.
pub fn has_events() -> bool {
    !INPUT_QUEUE.lock().is_empty()
}

// ---------------------------------------------------------------------------
// Per-device state
// ---------------------------------------------------------------------------
struct InputDevice {
    base: usize,
    vq: Virtqueue,
    /// Physical addresses of the event buffers we submitted.
    buf_addrs: Vec<(u16, usize)>, // (desc_idx, phys addr)
    /// Accumulated absolute position before SYN
    abs_x: i32,
    abs_y: i32,
    is_tablet: bool,
}

const EVENT_BUF_COUNT: usize = 64;
const EVENT_SIZE: usize = 8; // size of VirtioInputEvent

static DEVICES: IrqMutex<Vec<InputDevice>> = IrqMutex::new(Vec::new());

// ---------------------------------------------------------------------------
// Initialisation
// ---------------------------------------------------------------------------

/// Initialise all virtio-input devices discovered by the transport layer.
pub fn init() {
    let discovered = virtio_mmio::probe();

    for (base, dev_id, irq) in &discovered {
        if *dev_id != VIRTIO_DEV_INPUT {
            continue;
        }
        if let Some(dev) = init_one_device(*base) {
            crate::kprintln!("  virtio-input: {} at {:#X}",
                if dev.is_tablet { "tablet" } else { "keyboard" }, dev.base);
            // Enable this device's SPI in the GIC so interrupts are delivered
            crate::hal::interrupts::configure_spi(*irq, 0xA0);
            DEVICES.lock().push(dev);
        }
    }
}

fn init_one_device(base: usize) -> Option<InputDevice> {
    // Standard virtio init (no special features needed)
    if !virtio_mmio::init_device(base, 0) {
        return None;
    }

    // Set up queue 0 (event queue)
    let mut vq = Virtqueue::new(base, 0)?;

    // Detect if this is a tablet (absolute pointing device) vs keyboard
    let is_tablet = detect_tablet(base);

    // Mark device as ready
    virtio_mmio::driver_ok(base);

    // Pre-fill event queue with device-writable buffers
    let mut buf_addrs = Vec::new();
    let bufs_to_submit = (vq.queue_size as usize).min(EVENT_BUF_COUNT);
    for _ in 0..bufs_to_submit {
        // Allocate a single page for event buffers (shared across a few)
        // Actually, allocate one 8-byte buffer per descriptor
        let phys = match physical::alloc_contiguous_frames(1) {
            Some(p) => p,
            None => break,
        };
        // Zero it
        unsafe { ptr::write_bytes(phys as *mut u8, 0, 0x1000); }

        // We can fit 512 events per page, but we'll submit one desc per event
        let per_page = 0x1000 / EVENT_SIZE;
        for j in 0..per_page {
            if buf_addrs.len() >= bufs_to_submit { break; }
            let ev_phys = phys + j * EVENT_SIZE;
            if let Some(di) = vq.submit_buf(ev_phys, EVENT_SIZE as u32, true) {
                buf_addrs.push((di, ev_phys));
            } else {
                break;
            }
        }
        if buf_addrs.len() >= bufs_to_submit { break; }
    }

    // Notify device that buffers are ready
    vq.notify();

    Some(InputDevice {
        base,
        vq,
        buf_addrs,
        abs_x: 0,
        abs_y: 0,
        is_tablet,
    })
}

fn detect_tablet(base: usize) -> bool {
    // Read device name from config: select = ID_NAME (1), subsel = 0
    // Check if it contains "Tablet" or "tablet"
    // virtio-input config:
    //   offset 0x100 + 0 = select (u8)
    //   offset 0x100 + 1 = subsel (u8)
    //   offset 0x100 + 2 = size (u8)
    //   offset 0x100 + 8..+136 = string (128 bytes)
    unsafe {
        ptr::write_volatile((base + 0x100) as *mut u8, VIRTIO_INPUT_CFG_ID_NAME);
        ptr::write_volatile((base + 0x101) as *mut u8, 0);
        // Read size
        let size = ptr::read_volatile((base + 0x102) as *const u8) as usize;
        if size == 0 { return false; }
        let size = size.min(128);
        let mut name = [0u8; 128];
        for i in 0..size {
            name[i] = ptr::read_volatile((base + 0x108 + i) as *const u8);
        }
        // Check for "Tablet" or "tablet"
        let name_str = core::str::from_utf8(&name[..size]).unwrap_or("");
        name_str.contains("ablet") // matches "Tablet" and "tablet"
    }
}

// ---------------------------------------------------------------------------
// IRQ handler
// ---------------------------------------------------------------------------

/// Called from virtio_mmio::handle_irq when device_id == 18.
pub fn handle_irq(base: usize) {
    // Acknowledge the interrupt
    virtio_mmio::ack_interrupt(base);

    let mut devices = DEVICES.lock();
    for dev in devices.iter_mut() {
        if dev.base != base { continue; }
        process_events(dev);
        return;
    }
}

fn process_events(dev: &mut InputDevice) {
    // Drain used ring
    let used = dev.vq.poll_used();
    for (desc_idx, _len) in &used {
        // Find the physical address for this descriptor
        let phys = match dev.buf_addrs.iter().find(|(di, _)| *di == *desc_idx) {
            Some((_, p)) => *p,
            None => continue,
        };

        // Read the event
        let ev: VirtioInputEvent = unsafe { ptr::read_volatile(phys as *const VirtioInputEvent) };
        translate_event(dev, &ev);

        // Re-submit the buffer
        dev.vq.free_desc(*desc_idx);
        if let Some(new_di) = dev.vq.submit_buf(phys, EVENT_SIZE as u32, true) {
            // Update our mapping
            if let Some(entry) = dev.buf_addrs.iter_mut().find(|(di, _)| *di == *desc_idx) {
                entry.0 = new_di;
            }
        }
    }

    if !used.is_empty() {
        dev.vq.notify();
    }
}

fn translate_event(dev: &mut InputDevice, ev: &VirtioInputEvent) {
    match ev.typ {
        EV_ABS => {
            match ev.code {
                ABS_X => dev.abs_x = ev.value as i32,
                ABS_Y => dev.abs_y = ev.value as i32,
                _ => {}
            }
        }
        EV_KEY => {
            let pressed = ev.value != 0;
            match ev.code {
                BTN_LEFT | BTN_TOUCH => {
                    INPUT_QUEUE.lock().push_back(InputEvent::MouseButton {
                        button: 0,
                        pressed,
                    });
                }
                BTN_RIGHT => {
                    INPUT_QUEUE.lock().push_back(InputEvent::MouseButton {
                        button: 1,
                        pressed,
                    });
                }
                BTN_MIDDLE => {
                    INPUT_QUEUE.lock().push_back(InputEvent::MouseButton {
                        button: 2,
                        pressed,
                    });
                }
                code => {
                    INPUT_QUEUE.lock().push_back(InputEvent::Key { code, pressed });
                }
            }
        }
        EV_SYN => {
            // On SYN, emit accumulated pointer position
            if dev.is_tablet {
                // Scale from [0, 32767] to screen coordinates
                let screen_w = crate::drivers::framebuffer::SCREEN_WIDTH as i32;
                let screen_h = crate::drivers::framebuffer::SCREEN_HEIGHT as i32;
                let x = ((dev.abs_x as i64 * screen_w as i64) / 32768) as u32;
                let y = ((dev.abs_y as i64 * screen_h as i64) / 32768) as u32;
                INPUT_QUEUE.lock().push_back(InputEvent::MouseMove {
                    x: x.min(screen_w as u32 - 1),
                    y: y.min(screen_h as u32 - 1),
                });
            }
        }
        _ => {}
    }
}
