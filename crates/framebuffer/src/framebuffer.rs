//! # Framebuffer Management

#![no_std]

pub mod font;

use {
    crate::font::{CHAR_HEIGHT, CHAR_WIDTH},
    boot_info::{DisplayInfo, PixelFormat},
    core::{
        ops::{Add, Sub},
        sync::atomic::{AtomicUsize, Ordering},
    },
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
    pub fn new(addr: usize, width: usize, height: usize) -> Self {
        Self {
            ptr: addr as *mut u32,
            width,
            height,
            format: PixelFormat::Bgr,
        }
    }

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

    pub fn buffer(&self) -> &[u32] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.width * self.height) }
    }

    pub fn buffer_mut(&mut self) -> &mut [u32] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.width * self.height) }
    }

    pub fn size(&self) -> (i32, i32) {
        (self.width as i32, self.height as i32)
    }

    pub fn size_in_bytes(&self) -> usize {
        self.width * self.height * 4
    }

    pub fn contains(&self, point: Point) -> bool {
        point.x >= 0 && point.y >= 0 && point.x < self.width as i32 && point.y < self.height as i32
    }

    pub fn intersects(&self, x: i32, y: i32, w: usize, h: usize) -> bool {
        x >= -(w as i32)
            && y >= -(h as i32)
            && x < self.width.saturating_sub(w) as i32
            && y < self.height.saturating_sub(h) as i32
    }

    pub fn point_index(&self, point: Point) -> Option<usize> {
        if self.contains(point) {
            Some((self.width * point.y as usize) + point.x as usize)
        } else {
            None
        }
    }

    pub fn clear_screen(&mut self, color: Color) {
        let color = color.to_u32(self.format);

        unsafe {
            core::slice::from_raw_parts_mut(self.ptr, self.width * self.height).fill(color);
        }
    }

    pub fn draw_pixel(&mut self, point: Point, color: Color) {
        if !self.contains(point) {
            return;
        }

        let color = color.to_u32(self.format);

        unsafe {
            let ptr = self
                .ptr
                .add(point.y as usize * self.width + point.x as usize);
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
        point: Point,
        col: usize,
        row: usize,
    ) {
        let ch = if ch.is_ascii() { ch } else { '?' };

        let start_x = point.x + (col * CHAR_WIDTH) as i32;
        let start_y = point.y + (row * CHAR_HEIGHT) as i32;

        if !self.intersects(start_x, start_y, CHAR_WIDTH, CHAR_HEIGHT) {
            return;
        }

        let offset_x = if start_x < 0 { -start_x } else { 0 };
        let offset_y = if start_y < 0 { -start_y } else { 0 };
        let mut x_add = offset_x;
        let mut y_add = offset_y;
        loop {
            let point = Point::new(start_x + x_add, start_y + y_add);

            if self.contains(point) {
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
                self.draw_pixel(point, color);
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const ORIGIN: Self = Self::new(0, 0);

    #[inline(always)]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

impl Sub for Point {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl Add for Point {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

pub fn composite<'a>(
    sources: impl IntoIterator<Item = (&'a mut Framebuffer, Point)>,
    target: &mut Framebuffer,
) {
    for (source, point_in_target) in sources.into_iter() {
        composite_buffer(source, target, point_in_target);
    }
}

fn composite_buffer(source: &Framebuffer, target: &mut Framebuffer, target_point: Point) {
    let (target_width, target_height) = target.size();
    let (source_width, source_height) = source.size();

    let area_end = target_point + Point::new(source_width, source_height);

    let target_start = Point::new(0.max(target_point.x), 0.max(target_point.y));
    let target_end = Point::new(target_width.min(area_end.x), target_height.min(area_end.y));
    if target_end.x < 0
        || target_end.y < 0
        || target_start.x > target_width
        || target_start.y > target_height
    {
        return;
    }

    let source_start = target_start - target_point;
    let source_end = target_end - target_point;
    if source_end.x < 0
        || source_end.y < 0
        || source_start.x > source_width
        || source_start.y > source_height
    {
        return;
    }

    let source_start_x = 0.max(source_start.x);
    let source_start_y = 0.max(source_start.y);

    let area_width = source_width.min(source_end.x) - source_start_x;
    let area_height = source_height.min(source_end.y) - source_start_y;

    for row_offset in 0..area_height {
        let source_start = Point::new(source_start_x, source_start_y + row_offset);
        let Some(source_start_index) = source.point_index(source_start) else {
            break;
        };
        let source_end_index = source_start_index + area_width as usize;
        let target_start = source_start + target_point;
        let Some(target_start_index) = target.point_index(target_start) else {
            break;
        };

        let source_slice = &source.buffer()[source_start_index..source_end_index];
        target.buffer_mut()[target_start_index..(target_start_index + source_slice.len())]
            .copy_from_slice(source_slice);
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
