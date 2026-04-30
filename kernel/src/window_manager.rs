//! # Window Manager

use {
    crate::{BOOT_INFO, scheduler::with_scheduler},
    crossbeam_queue::ArrayQueue,
    framebuffer::{Color, Framebuffer, Point},
    log::warn,
    memory_types::PAGE_SIZE,
};



static mut EVENT_QUEUE: Option<ArrayQueue<Event>> = None;

pub fn init() {
    unsafe { EVENT_QUEUE = Some(ArrayQueue::new(128)) };
    with_scheduler(|scheduler| {
        scheduler.run_kernel_process(
            "window_manager",
            window_manager as *const fn() -> !,
            Some(PAGE_SIZE * 16),
        );
    });
}

pub fn send_event(event: Event) {
    if let Err(event) = unsafe {
        EVENT_QUEUE
            .as_ref()
            .expect("event queue should be initialized")
            .push(event)
    } {
        warn!("Event queue exceeded its capacity, missed 1 event: {event:?}")
    }
}

#[derive(Debug)]
pub enum Event {
    ClockUpdate,
}

fn window_manager() -> ! {
    let mut wm = WindowManager::new();

    loop {
        while let Some(event) = unsafe {
            EVENT_QUEUE
                .as_ref()
                .expect("event queue should be initialized")
                .pop()
        } {
            wm.handle_event(event);
        }
    }
}

struct WindowManager {
    framebuffer: Framebuffer,
    display_width: usize,
    display_height: usize,
    clock_start_time: rtc::Time,
    clock_start_instant: time::Instant,
}

impl WindowManager {
    fn new() -> Self {
        let boot_info = unsafe {
            BOOT_INFO.expect("window manager should have access to the boot information")
        };

        let mut framebuffer = Framebuffer::from_display_info(&boot_info.display_info);
        framebuffer.clear_screen(Color::rgb(0x2B, 0x2B, 0x33));

        for (col, ch) in "KERNEL v0.0.0".char_indices() {
            framebuffer.draw_ascii_char(
                ch,
                Color::rgb(0xaa, 0xaa, 0xad),
                Color::rgb(0x2B, 0x2B, 0x33),
                Point::new(10, 10),
                col,
                0,
            );
            framebuffer.draw_ascii_char(
                '-',
                Color::rgb(0xaa, 0xaa, 0xad),
                Color::rgb(0x2B, 0x2B, 0x33),
                Point::new(10, 10),
                col,
                1,
            );
        }

        Self {
            framebuffer,
            display_width: boot_info.display_info.width as usize,
            display_height: boot_info.display_info.height as usize,
            clock_start_time: rtc::Time::now(),
            clock_start_instant: time::Instant::now(),
        }
    }

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::ClockUpdate => {
                let dur = self.clock_start_instant.elapsed();
                let current_time = self.clock_start_time + rtc::Time::from_seconds(dur.as_secs());
                let time_string = format!("{current_time}");

                let col_count = self.display_width / framebuffer::font::CHAR_WIDTH;
                let row_count = self.display_height / framebuffer::font::CHAR_HEIGHT;
                let start_col = col_count - 20; // MM/DD/YYYY HH:MM:SS <- 19 chars
                let row = row_count.saturating_sub(2);

                for (col, ch) in time_string.char_indices() {
                    self.framebuffer.draw_ascii_char(
                        ch,
                        Color::rgb(0xaa, 0xaa, 0xad),
                        Color::rgb(0x2B, 0x2B, 0x33),
                        Point::new(10, 10),
                        start_col + col,
                        row,
                    );
                }
            }
        }
    }
}
