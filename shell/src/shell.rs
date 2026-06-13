//! # Shell

#![no_std]

use {
    core::{alloc::Layout, fmt::Write as _, sync::atomic::Ordering},
    framebuffer::Color,
    heap::{alloc::alloc::alloc_zeroed, string::ToString as _, Allocator},
    input::{InputEvent, GLOBAL_INPUT_QUEUE},
    math::{Area, Point, Size},
    spin_mutex::Mutex,
};

const BG_COLOR: Color = Color::rgb(0x1E, 0x1E, 0x22);
const FG_COLOR: Color = Color::rgb(0xA7, 0xA7, 0xAD);
const PANEL_COLOR: Color = Color::rgb(0x2B, 0x2B, 0x33);

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

#[global_allocator]
static ALLOCATOR: Allocator = Allocator::new();

static SERIAL2: Mutex<uart_16550::Device> = Mutex::new(uart_16550::Device::COM2);

pub extern "C" fn main() -> ! {
    unsafe {
        ALLOCATOR.init(heap::BASE_ADDR + 0x1000, heap::DEFAULT_SIZE);
    }

    unsafe { SERIAL2.lock().init() };
    log::set_logger(&SerialLogger).unwrap();

    log::info!("Logging ready");

    {
        let string = "EXAMPLE".to_string();
        if string.chars().nth(2) != Some('A') {
            panic!("???")
        }

        let phys_addr = process::translate_address(string.as_ptr().addr()).unwrap();
        assert_ne!(string.as_ptr().addr(), phys_addr);
    }

    if !time::monotonic_clock_ready() {
        panic!("CLOCK NOT READY");
    }

    let mut framebuffer = framebuffer::Framebuffer::global().unwrap();
    framebuffer.clear_screen(BG_COLOR);

    let mouse_fb_layout =
        Layout::from_size_align(POINTER_WIDTH * POINTER_HEIGHT * 4, 4096).unwrap();
    let mouse_fb_ptr = unsafe { alloc_zeroed(mouse_fb_layout) };
    let mut mouse_fb =
        framebuffer::Framebuffer::new(mouse_fb_ptr.addr(), POINTER_WIDTH, POINTER_HEIGHT)
            .to_color_buffer();
    for y in 0..POINTER_HEIGHT {
        for x in 0..POINTER_WIDTH {
            let color = POINTER_IMAGE[x as usize][y as usize];
            mouse_fb.draw_pixel(Point::new(x as f32, y as f32), color);
        }
    }

    let fb_layout = Layout::from_size_align(framebuffer.size_in_bytes(), 4096).unwrap();
    let top_fb_ptr = unsafe { alloc_zeroed(fb_layout) };
    let bottom_fb_ptr = unsafe { alloc_zeroed(fb_layout) };

    let mut top_fb = framebuffer
        .with_new_addr(top_fb_ptr.addr())
        .to_color_buffer();
    let mut bottom_fb = framebuffer
        .with_new_addr(bottom_fb_ptr.addr())
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

    let display_width = framebuffer::FRAMEBUFFER_WIDTH.load(Ordering::Relaxed) as f32;
    let display_height = framebuffer::FRAMEBUFFER_HEIGHT.load(Ordering::Relaxed) as f32;

    let view_width = display_width / 1.31;
    let view_height = display_height / 1.07;
    let view_x_offset = display_width - view_width;
    let view_y_offset = (display_height - view_height) / 2.0;

    bottom_fb.fill_area(
        Area::new(
            Point::new(view_x_offset, view_y_offset),
            Size::new(view_width, view_height),
        ),
        PANEL_COLOR,
    );

    let bar_height = view_y_offset;
    let bar_text_y_offset = (bar_height - framebuffer::font::CHAR_HEIGHT as f32) / 2.0;

    for (char_column, ch) in "Rhubarb v0.0.0".char_indices() {
        top_fb.draw_ascii_char(
            ch,
            FG_COLOR,
            Color::NONE,
            Point::new(5.0, bar_text_y_offset),
            char_column,
            0,
        );
    }

    let mut input_state = InputState {
        mouse_pos: Point::new(display_width / 2.0, display_height / 2.0),
    };

    framebuffer::composite(
        [
            (&mut bottom_fb, Point::ORIGIN),
            (&mut top_fb, Point::ORIGIN),
            (&mut mouse_fb, input_state.mouse_pos),
        ],
        &mut framebuffer,
    );

    // let mut seen_events = hashbrown::HashSet::with_hasher(rustc_hash::FxBuildHasher);
    'main_loop: loop {
        for event in GLOBAL_INPUT_QUEUE.lock().drain() {
            // if seen_events.insert(event) {
            //     log::info!("New input event: {event:?}");
            // }
            let render = match event {
                InputEvent::KeyPress { code } => {
                    if code == 16 {
                        break 'main_loop;
                    }
                    false
                }
                InputEvent::MouseMove { delta_x, delta_y } => {
                    input_state.mouse_pos = Point::new(
                        (display_width - 1.0)
                            .min(input_state.mouse_pos.x + delta_x as f32)
                            .max(0.0),
                        (display_height - 1.0)
                            .min(input_state.mouse_pos.y + delta_y as f32)
                            .max(0.0),
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



pub struct SerialLogger;

impl log::Log for SerialLogger {
    fn log(
        &self,
        level: log::LogLevel,
        target: &str,
        _module_path: &'static str,
        _location: &'static core::panic::Location,
        args: core::fmt::Arguments,
    ) {
        const ANSI_SGR_DIM: u8 = 2;
        const ANSI_SGR_FG_RED: u8 = 31;
        const ANSI_SGR_FG_GREEN: u8 = 32;
        const ANSI_SGR_FG_YELLOW: u8 = 33;
        const ANSI_SGR_FG_BLUE: u8 = 34;

        let level_color_code = match level {
            log::LogLevel::Error => ANSI_SGR_FG_RED,
            log::LogLevel::Warn => ANSI_SGR_FG_YELLOW,
            log::LogLevel::Info => ANSI_SGR_FG_GREEN,
            log::LogLevel::Debug => ANSI_SGR_FG_BLUE,
            log::LogLevel::Trace => ANSI_SGR_DIM,
        };

        serial_println!(
            "\x1b[2m[{}] \x1b[{}m{}\x1b[0m",
            target,
            level_color_code,
            args,
        );
    }
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    SERIAL2
        .lock()
        .write_fmt(args)
        .expect("failed to write to serial port COM2");
}

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! serial_println {
    () => {
        $crate::serial_print!("\n")
    };
    ($fmt:expr) => {
        $crate::serial_print!(concat!($fmt, "\n"))
    };
    ($fmt:expr, $($arg:tt)*) => {
        $crate::serial_print!(concat!($fmt, "\n"), $($arg)*)
    };
}
