//! # Math

#![no_std]

mod area;
mod point;
mod size;

pub use {area::*, point::*, size::*};



#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    pub const fn cross(&self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        }
    }

    #[inline]
    pub const fn pack_point(self, axis_value: f32, cross_value: f32) -> Point {
        match self {
            Self::Horizontal => Point::new(axis_value, cross_value),
            Self::Vertical => Point::new(cross_value, axis_value),
        }
    }

    #[inline]
    pub const fn pack_size(self, axis_value: f32, cross_value: f32) -> Size {
        match self {
            Self::Horizontal => Size::new(axis_value, cross_value),
            Self::Vertical => Size::new(cross_value, axis_value),
        }
    }
}
