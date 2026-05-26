//! # Example Program

#![no_std]

use {
    core::sync::atomic::Ordering,
    framebuffer::Color,
    heap::string::ToString as _,
    input::{GLOBAL_INPUT_QUEUE, InputEvent},
    math::Point,
};

const BG_COLOR: Color = Color::rgb(0x2B, 0x2B, 0x33);

const POINTER_HEIGHT: usize = 16;
const POINTER_WIDTH: usize = 10;
static POINTER_IMAGE: [[Color; POINTER_HEIGHT]; POINTER_WIDTH] = {
    const O: Color = Color::NONE; // Background.
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
    heap::init();

    let string = "EXAMPLE".to_string();
    if string.chars().nth(2) != Some('A') {
        panic!("???")
    }

    let phys_addr = process::translate_address(string.as_ptr().addr()).unwrap();
    assert_ne!(string.as_ptr().addr(), phys_addr);
    // assert_eq!(phys_addr, 0x232a000);

    if !time::monotonic_clock_ready() {
        panic!("CLOCK NOT READY");
    }

    let mut framebuffer = framebuffer::Framebuffer::global().unwrap();
    framebuffer.clear_screen(BG_COLOR);

    let mouse_fb_size = POINTER_WIDTH * POINTER_HEIGHT * 4;
    let mouse_fb_page_count = mouse_fb_size.div_ceil(4096);
    let mouse_fb_addr = heap::BASE_ADDR + 4096;
    let mut mouse_fb = framebuffer::Framebuffer::new(mouse_fb_addr, POINTER_WIDTH, POINTER_HEIGHT)
        .to_color_buffer();
    for y in 0..POINTER_HEIGHT {
        for x in 0..POINTER_WIDTH {
            let color = POINTER_IMAGE[x as usize][y as usize];
            mouse_fb.draw_pixel(Point::new(x as f32, y as f32), color);
        }
    }

    let top_fb_page_count = framebuffer.size_in_bytes().div_ceil(4096);
    let top_fb_addr = mouse_fb_addr + (mouse_fb_page_count * 4096);
    let bottom_fb_addr = top_fb_addr + (top_fb_page_count * 4096);

    let mut top_fb = framebuffer
        .with_new_addr(top_fb_addr as usize)
        .to_color_buffer();
    let mut bottom_fb = framebuffer
        .with_new_addr(bottom_fb_addr as usize)
        .to_color_buffer();

    // HACK: We draw the bottom right pixel before doing anything with the
    //       framebuffer to trigger a heap extension large enough to fit the full
    //       memory range. Without this, the heap gets extended page-by-page (which
    //       is expensive).
    top_fb.draw_pixel(
        Point::ORIGIN + top_fb.area().size() - Point::ONE_ONE,
        Color::NONE,
    );
    top_fb.clear_screen(Color::NONE);
    bottom_fb.draw_pixel(
        Point::ORIGIN + bottom_fb.area().size() - Point::ONE_ONE,
        Color::NONE,
    );
    bottom_fb.clear_screen(BG_COLOR);

    // bottom_fb.draw_ascii_char('B', Color::WHITE, BG_COLOR, Point::ONE_ONE, 0, 0);
    // top_fb.draw_ascii_char('T', Color::RED, Color::NONE, Point::ONE_ONE, 0, 0);

    let display_width = framebuffer::FRAMEBUFFER_WIDTH.load(Ordering::Relaxed);
    let display_height = framebuffer::FRAMEBUFFER_HEIGHT.load(Ordering::Relaxed);
    let mut input_state = InputState {
        mouse_pos: Point::new(display_width as f32 / 2.0, display_height as f32 / 2.0),
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
                        0_f32.max(
                            (display_width as f32 - 1.0)
                                .min(input_state.mouse_pos.x + delta_x as f32),
                        ),
                        0_f32.max(
                            (display_height as f32 - 1.0)
                                .min(input_state.mouse_pos.y + delta_y as f32),
                        ),
                    );

                    true
                }
                _ => false,
            };

            if render {
                framebuffer::composite(
                    [
                        (&mut bottom_fb, Point::ORIGIN),
                        (&mut top_fb, Point::ORIGIN),
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
