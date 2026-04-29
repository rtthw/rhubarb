//! # Framebuffer Management

#![no_std]

pub mod font;

use {
    crate::font::{CHAR_HEIGHT, CHAR_WIDTH},
    boot_info::{DisplayInfo, PixelFormat},
    core::sync::atomic::{AtomicUsize, Ordering},
};


pub static FRAMEBUFFER_ADDR: AtomicUsize = AtomicUsize::new(0);
pub static FRAMEBUFFER_SIZE: AtomicUsize = AtomicUsize::new(0);
pub static FRAMEBUFFER_WIDTH: AtomicUsize = AtomicUsize::new(0);
pub static FRAMEBUFFER_HEIGHT: AtomicUsize = AtomicUsize::new(0);


pub struct Framebuffer {
    ptr: *mut u32,
    width: usize,
    height: usize,
    format: PixelFormat,
}

impl Framebuffer {
    pub fn global() -> Option<Self> {
        let addr = FRAMEBUFFER_ADDR.load(Ordering::Relaxed);
        if addr == 0 {
            return None;
        }

        let size = FRAMEBUFFER_SIZE.load(Ordering::Relaxed);
        let width = FRAMEBUFFER_WIDTH.load(Ordering::Relaxed);
        let height = FRAMEBUFFER_HEIGHT.load(Ordering::Relaxed);

        assert_eq!(size / 4, width * height);

        Some(Self {
            ptr: addr as *mut u32,
            width,
            height,
            format: PixelFormat::Bgr,
        })
    }

    pub fn from_display_info(display_info: &DisplayInfo) -> Self {
        assert_eq!(
            display_info.framebuffer_size / 4,
            (display_info.stride * display_info.height) as usize,
        );

        Self {
            ptr: display_info.framebuffer_addr as *mut u32,
            width: display_info.stride as usize,
            height: display_info.height as usize,
            format: display_info.format,
        }
    }

    pub fn with_new_addr(&self, addr: usize) -> Self {
        Self {
            ptr: addr as *mut u32,
            width: self.width,
            height: self.height,
            format: self.format,
        }
    }

    pub fn size_in_bytes(&self) -> usize {
        self.width * self.height * 4
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32
    }

    pub fn intersects(&self, x: i32, y: i32, w: usize, h: usize) -> bool {
        x >= -(w as i32)
            && y >= -(h as i32)
            && x < self.width.saturating_sub(w) as i32
            && y < self.height.saturating_sub(h) as i32
    }

    pub fn clear_screen(&mut self, color: Color) {
        let color = color.to_u32(self.format);

        unsafe {
            core::slice::from_raw_parts_mut(self.ptr, self.width * self.height).fill(color);
        }
    }

    pub fn draw_pixel(&mut self, x: i32, y: i32, color: Color) {
        if !self.contains(x, y) {
            return;
        }

        let color = color.to_u32(self.format);

        unsafe {
            let ptr = self.ptr.add(y as usize * self.width + x as usize);
            ptr.write(color);
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color) {
        let color = color.to_u32(self.format);

        let x = x.clamp(0, self.width as _) as usize;
        let y = y.clamp(0, self.height as _) as usize;
        let w = w.clamp(0, self.width.saturating_sub(x) as _) as usize;
        let h = h.clamp(0, self.height.saturating_sub(y) as _) as usize;

        if w == 0 || h == 0 {
            return;
        }

        unsafe {
            let mut ptr = self.ptr.add(y * self.width + x);
            core::slice::from_raw_parts_mut(ptr, w).fill(color);
            for _ in 1..h {
                let src = ptr;
                ptr = ptr.add(self.width);
                src.copy_to_nonoverlapping(ptr, w);
            }
        }
    }

    pub fn draw_ascii_char(
        &mut self,
        ch: char,
        fg_color: Color,
        bg_color: Color,
        x: i32,
        y: i32,
        col: usize,
        row: usize,
    ) {
        let ch = if ch.is_ascii() { ch } else { '?' };

        let start_x = x + (col * CHAR_WIDTH) as i32;
        let start_y = y + (row * CHAR_HEIGHT) as i32;

        if !self.intersects(start_x, start_y, CHAR_WIDTH, CHAR_HEIGHT) {
            return;
        }

        let offset_x = if start_x < 0 { -start_x } else { 0 };
        let offset_y = if start_y < 0 { -start_y } else { 0 };
        let mut x_add = offset_x;
        let mut y_add = offset_y;
        loop {
            let x = start_x + x_add;
            let y = start_y + y_add;

            if self.contains(x, y) {
                let color = if x_add >= 1 {
                    // Leave a 1 pixel gap between characters.
                    let index = x_add - 1;
                    let font_char = font::BASIC_FONT[ch as usize][y_add as usize];
                    // The most significant bit determines whether the pixel's color belongs to the
                    // foreground or background.
                    if font_char & (0x80 >> index) != 0 {
                        fg_color
                    } else {
                        bg_color
                    }
                } else {
                    bg_color
                };
                self.draw_pixel(x, y, color);
            }

            x_add += 1;
            if x_add == CHAR_WIDTH as i32 || start_x + x_add == self.width as i32 {
                y_add += 1;
                if y_add == CHAR_HEIGHT as i32 || start_y + y_add == self.height as i32 {
                    return;
                }
                x_add = offset_x;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const NONE: Self = Self::rgba(0, 0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const RED: Self = Self::rgb(255, 0, 0);
    pub const GREEN: Self = Self::rgb(0, 255, 0);
    pub const BLUE: Self = Self::rgb(0, 0, 255);

    #[inline]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    #[inline]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    #[inline]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn to_u32(self, format: PixelFormat) -> u32 {
        match format {
            PixelFormat::Bgr => (self.r as u32) << 16 | (self.g as u32) << 8 | (self.b as u32) << 0,
            PixelFormat::Rgb => (self.r as u32) << 0 | (self.g as u32) << 8 | (self.b as u32) << 16,
        }
    }
}
