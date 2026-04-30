//! # Example Program

#![no_std]

use {
    core::sync::atomic::Ordering,
    framebuffer::{Color, Point},
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

    let mouse_fb_addr =
        process::mmap((POINTER_WIDTH * POINTER_HEIGHT * 4).div_ceil(4096) as u64).unwrap();
    let mut mouse_fb =
        framebuffer::Framebuffer::new(mouse_fb_addr as usize, POINTER_WIDTH, POINTER_HEIGHT);
    for y in 0..POINTER_HEIGHT {
        for x in 0..POINTER_WIDTH {
            let color = POINTER_IMAGE[x as usize][y as usize];
            mouse_fb.draw_pixel(Point::new(x as i32, y as i32), color);
        }
    }

    let fb_page_count = framebuffer.size_in_bytes().div_ceil(4096);
    let bottom_fb_addr = process::mmap(fb_page_count as u64).unwrap();
    let mut bottom_fb = framebuffer.with_new_addr(bottom_fb_addr as usize);
    bottom_fb.clear_screen(Color::rgb(0x2B, 0x2B, 0x33));

    let display_width = framebuffer::FRAMEBUFFER_WIDTH.load(Ordering::Relaxed);
    let display_height = framebuffer::FRAMEBUFFER_HEIGHT.load(Ordering::Relaxed);
    let mut input_state = InputState {
        mouse_pos: Point::new(display_width as i32 / 2, display_height as i32 / 2),
    };

    'main_loop: loop {
        for event in GLOBAL_INPUT_QUEUE.lock().drain() {
            let render = match event {
                InputEvent::KeyPress { code } => {
                    if code == 16 {
                        break 'main_loop;
                    }
                    false
                }
                InputEvent::MouseMove { delta_x, delta_y } => {
                    input_state.mouse_pos = Point::new(
                        0.max((display_width as i32 - 1).min(input_state.mouse_pos.x + delta_x)),
                        0.max((display_height as i32 - 1).min(input_state.mouse_pos.y + delta_y)),
                    );

                    true
                }
                _ => false,
            };

            if render {
                framebuffer::composite(
                    [
                        (&mut bottom_fb, Point::ORIGIN),
                        (&mut mouse_fb, input_state.mouse_pos),
                    ],
                    &mut framebuffer,
                );
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
    mouse_pos: Point,
}
