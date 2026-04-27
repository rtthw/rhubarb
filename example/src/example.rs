//! # Example Program

#![no_std]

use {
    core::sync::atomic::Ordering,
    framebuffer::Color,
    input::{GLOBAL_INPUT_QUEUE, InputEvent},
};

const TEST_PAGE_FAULT: bool = false;
const TEST_WRITE_TIME: bool = false;

const POINTER_HEIGHT: usize = 16;
const POINTER_WIDTH: usize = 10;
static POINTER_IMAGE: [[Color; POINTER_HEIGHT]; POINTER_WIDTH] = {
    const O: Color = Color::rgb(0x2B, 0x2B, 0x33); // Background.
    const F: Color = Color::BLACK; // Cursor face.
    const B: Color = Color::WHITE; // Cursor border.

    [
        [B, B, B, B, B, B, B, B, B, B, B, B, B, B, O, O],
        [O, B, F, F, F, F, F, F, F, F, F, F, F, B, O, O],
        [O, O, B, F, F, F, F, F, F, F, F, F, F, B, O, O],
        [O, O, O, B, F, F, F, F, F, F, F, F, F, B, O, O],
        [O, O, O, O, B, F, F, F, F, F, F, F, F, F, B, O],
        [O, O, O, O, O, B, F, F, F, F, F, F, F, F, F, B],
        [O, O, O, O, O, O, B, F, F, F, F, F, F, F, F, B],
        [O, O, O, O, O, O, O, B, F, F, F, B, B, B, B, B],
        [O, O, O, O, O, O, O, O, B, F, B, O, O, O, O, O],
        [O, O, O, O, O, O, O, O, O, B, O, O, O, O, O, O],
    ]
};

pub extern "C" fn main() -> ! {
    if TEST_PAGE_FAULT {
        let ptr = 0xab0de as *mut u8;
        unsafe {
            ptr.write(43);
        }
    }
    if TEST_WRITE_TIME {
        unsafe {
            time::set_monotonic_clock_period(1);
        }
    }

    if !time::monotonic_clock_ready() {
        panic!("CLOCK NOT READY");
    }

    let mut framebuffer = framebuffer::Framebuffer::global().unwrap();
    let display_width = framebuffer::FRAMEBUFFER_WIDTH.load(Ordering::Relaxed);
    let display_height = framebuffer::FRAMEBUFFER_HEIGHT.load(Ordering::Relaxed);
    let mut input_state = InputState {
        mouse_x: display_width as u32 / 2,
        mouse_y: display_height as u32 / 2,
    };

    'main_loop: loop {
        for event in GLOBAL_INPUT_QUEUE.lock().drain() {
            match event {
                InputEvent::KeyPress { code } => {
                    if code == 16 {
                        break 'main_loop;
                    }
                }
                InputEvent::MouseMove { delta_x, delta_y } => {
                    let old_x = input_state.mouse_x as i32;
                    let old_y = input_state.mouse_y as i32;

                    for y in old_y..(old_y + POINTER_HEIGHT as i32) {
                        for x in old_x..(old_x + POINTER_WIDTH as i32) {
                            framebuffer.draw_pixel(x, y, Color::rgb(0x2B, 0x2B, 0x33));
                        }
                    }

                    let new_x = 0.max((display_width as i32 - 1).min(old_x + delta_x));
                    let new_y = 0.max((display_height as i32 - 1).min(old_y + delta_y));

                    for y in new_y..(new_y + POINTER_HEIGHT as i32) {
                        for x in new_x..(new_x + POINTER_WIDTH as i32) {
                            let color = POINTER_IMAGE[(x - new_x) as usize][(y - new_y) as usize];
                            framebuffer.draw_pixel(x, y, color);
                        }
                    }

                    input_state.mouse_x = new_x as u32;
                    input_state.mouse_y = new_y as u32;
                }
                _ => {}
            }
        }

        process::defer();
    }

    process::exit(0);
}

#[cfg(not(test))]
#[panic_handler]
pub fn panic_handler(_info: &core::panic::PanicInfo<'_>) -> ! {
    process::exit(-1)
}

struct InputState {
    mouse_x: u32,
    mouse_y: u32,
}
