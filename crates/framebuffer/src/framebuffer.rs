//! # Framebuffer Management

#![no_std]

pub mod font;

use {
    crate::font::{CHAR_HEIGHT, CHAR_WIDTH},
    boot_info::{DisplayInfo, PixelFormat},
    core::sync::atomic::{AtomicUsize, Ordering},
    math::{Area, Point, Size, area, point, size},
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

    pub const fn with_new_addr(&self, addr: usize) -> Self {
        Self {
            ptr: addr as *mut u32,
            width: self.width,
            height: self.height,
            format: self.format,
        }
    }

    pub const fn to_color_buffer(self) -> ColorBuffer {
        ColorBuffer {
            ptr: self.ptr as *mut Color,
            width: self.width,
            height: self.height,
        }
    }

    pub const fn buffer(&self) -> &[u32] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.width * self.height) }
    }

    pub fn buffer_mut(&mut self) -> &mut [u32] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.width * self.height) }
    }

    pub const fn area(&self) -> Area {
        Area::from_size(self.size())
    }

    pub const fn size(&self) -> Size {
        Size::new(self.width as f32, self.height as f32)
    }

    pub const fn size_in_bytes(&self) -> usize {
        self.width * self.height * 4
    }

    pub const fn point_index(&self, point: Point) -> Option<usize> {
        if self.area().contains(point) {
            Some((self.width * point.y as usize) + point.x as usize)
        } else {
            None
        }
    }

    pub fn clear_screen(&mut self, color: Color) {
        let color = color.to_u32(self.format);
        self.buffer_mut().fill(color);
    }

    pub fn draw_pixel(&mut self, point: Point, color: Color) {
        if !self.area().contains(point) {
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

    pub fn fill_area(&mut self, area: Area, color: Color) {
        let color = color.to_u32(self.format);

        let x = area.x().clamp(0.0, self.width as _) as usize;
        let y = area.y().clamp(0.0, self.height as _) as usize;
        let width = area
            .width()
            .clamp(0.0, self.width.saturating_sub(area.x() as _) as _) as _;
        let height = area
            .height()
            .clamp(0.0, self.height.saturating_sub(area.y() as _) as _) as _;

        if width == 0 || height == 0 {
            return;
        }

        unsafe {
            let mut ptr = self.ptr.add(y * self.width + x);
            core::slice::from_raw_parts_mut(ptr, width).fill(color);
            for _ in 1..height {
                let src = ptr;
                ptr = ptr.add(self.width);
                src.copy_to_nonoverlapping(ptr, width);
            }
        }
    }

    pub fn draw_ascii_char(
        &mut self,
        ch: char,
        fg_color: Color,
        bg_color: Color,
        pos: Point,
        col: usize,
        row: usize,
    ) {
        let ch = if ch.is_ascii() { ch } else { '?' };

        let start_x = pos.x + (col * CHAR_WIDTH) as f32;
        let start_y = pos.y + (row * CHAR_HEIGHT) as f32;

        let area = area(
            point(start_x, start_y),
            size(CHAR_WIDTH as f32, CHAR_HEIGHT as f32),
        );

        if !self.area().intersects(&area) {
            return;
        }

        let offset_x = if start_x < 0.0 { -start_x } else { 0.0 };
        let offset_y = if start_y < 0.0 { -start_y } else { 0.0 };
        let mut x_add = offset_x;
        let mut y_add = offset_y;
        loop {
            let pos = point(start_x + x_add, start_y + y_add);

            if self.area().contains(pos) {
                let color = if x_add >= 1.0 {
                    // Leave a 1 pixel gap between characters.
                    let index = (x_add - 1.0) as usize;
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
                self.draw_pixel(pos, color);
            }

            x_add += 1.0;
            if x_add >= CHAR_WIDTH as f32 || start_x + x_add >= self.width as f32 {
                y_add += 1.0;
                if y_add >= CHAR_HEIGHT as f32 || start_y + y_add >= self.height as f32 {
                    return;
                }
                x_add = offset_x;
            }
        }
    }

    pub fn blend(&mut self, source: &[Color], start_index: usize) {
        let format = self.format;
        let buffer = &mut self.buffer_mut()[start_index..(start_index + source.len())];
        for pixel_index in 0..source.len() {
            buffer[pixel_index] = source[pixel_index].blend(buffer[pixel_index], format);
        }
    }
}

pub struct ColorBuffer {
    ptr: *mut Color,
    width: usize,
    height: usize,
}

impl ColorBuffer {
    pub const fn buffer(&self) -> &[Color] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.width * self.height) }
    }

    pub const fn buffer_mut(&mut self) -> &mut [Color] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.width * self.height) }
    }

    pub const fn area(&self) -> Area {
        Area::from_size(self.size())
    }

    pub const fn size(&self) -> Size {
        Size::new(self.width as f32, self.height as f32)
    }

    pub fn size_in_bytes(&self) -> usize {
        self.width * self.height * 4
    }

    pub fn point_index(&self, point: Point) -> Option<usize> {
        if self.area().contains(point) {
            Some((self.width * point.y as usize) + point.x as usize)
        } else {
            None
        }
    }

    pub fn clear_screen(&mut self, color: Color) {
        self.buffer_mut().fill(color);
    }

    pub fn draw_pixel(&mut self, point: Point, color: Color) {
        if !self.area().contains(point) {
            return;
        }

        unsafe {
            let ptr = self
                .ptr
                .add(point.y as usize * self.width + point.x as usize);
            ptr.write(color);
        }
    }

    pub fn fill_area(&mut self, area: Area, color: Color) {
        let x = area.x().clamp(0.0, self.width as _) as usize;
        let y = area.y().clamp(0.0, self.height as _) as usize;
        let width = area
            .width()
            .clamp(0.0, self.width.saturating_sub(area.x() as _) as _) as _;
        let height = area
            .height()
            .clamp(0.0, self.height.saturating_sub(area.y() as _) as _) as _;

        if width == 0 || height == 0 {
            return;
        }

        unsafe {
            let mut ptr = self.ptr.add(y * self.width + x);
            core::slice::from_raw_parts_mut(ptr, width).fill(color);
            for _ in 1..height {
                let src = ptr;
                ptr = ptr.add(self.width);
                src.copy_to_nonoverlapping(ptr, width);
            }
        }
    }

    pub fn draw_ascii_char(
        &mut self,
        ch: char,
        fg_color: Color,
        bg_color: Color,
        pos: Point,
        col: usize,
        row: usize,
    ) {
        let ch = if ch.is_ascii() { ch } else { '?' };

        let start_x = pos.x + (col * CHAR_WIDTH) as f32;
        let start_y = pos.y + (row * CHAR_HEIGHT) as f32;

        let area = area(
            point(start_x, start_y),
            size(CHAR_WIDTH as f32, CHAR_HEIGHT as f32),
        );

        if !self.area().intersects(&area) {
            return;
        }

        let offset_x = if start_x < 0.0 { -start_x } else { 0.0 };
        let offset_y = if start_y < 0.0 { -start_y } else { 0.0 };
        let mut x_add = offset_x;
        let mut y_add = offset_y;
        loop {
            let pos = point(start_x + x_add, start_y + y_add);

            if self.area().contains(pos) {
                let color = if x_add >= 1.0 {
                    // Leave a 1 pixel gap between characters.
                    let index = (x_add - 1.0) as usize;
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
                self.draw_pixel(pos, color);
            }

            x_add += 1.0;
            if x_add >= CHAR_WIDTH as f32 || start_x + x_add >= self.width as f32 {
                y_add += 1.0;
                if y_add >= CHAR_HEIGHT as f32 || start_y + y_add >= self.height as f32 {
                    return;
                }
                x_add = offset_x;
            }
        }
    }
}

pub fn composite<'a>(
    sources: impl IntoIterator<Item = (&'a mut ColorBuffer, Point)>,
    target: &mut Framebuffer,
) {
    for (source, point_in_target) in sources.into_iter() {
        composite_buffer(source, target, point_in_target);
    }
}

fn composite_buffer(source: &ColorBuffer, target: &mut Framebuffer, target_point: Point) {
    let target_size = target.size();
    let source_size = source.size();

    let area_end = target_point + source_size;

    let target_start = Point::new(target_point.x.max(0.0), target_point.y.max(0.0));
    let target_end = Point::new(
        target_size.width.min(area_end.x),
        target_size.height.min(area_end.y),
    );
    if target_end.x < 0.0
        || target_end.y < 0.0
        || target_start.x > target_size.width
        || target_start.y > target_size.height
    {
        return;
    }

    let source_start = target_start - target_point;
    let source_end = target_end - target_point;
    if source_end.x < 0.0
        || source_end.y < 0.0
        || source_start.x > source_size.width
        || source_start.y > source_size.height
    {
        return;
    }

    let source_start_x = source_start.x.max(0.0);
    let source_start_y = source_start.y.max(0.0);

    let area_width = source_size.width.min(source_end.x) - source_start_x;
    let area_height = source_size.height.min(source_end.y) - source_start_y;

    for row_offset in 0..(area_height as i32) {
        let source_start = Point::new(source_start_x, source_start_y + row_offset as f32);
        let Some(source_start_index) = source.point_index(source_start) else {
            break;
        };
        let source_end_index = source_start_index + area_width as usize;
        let target_start = source_start + target_point;
        let Some(target_start_index) = target.point_index(target_start) else {
            break;
        };

        target.blend(
            &source.buffer()[source_start_index..source_end_index],
            target_start_index,
        );
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
            _ => unreachable!(),
        }
    }

    pub const fn blend(self, pixel: u32, format: PixelFormat) -> u32 {
        if self.a == 0 {
            return pixel;
        }
        self.to_u32(format)
    }
}
