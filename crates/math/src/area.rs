//! # Area

use crate::{Point, Size};



/// Shorthand for [`Area::new`].
#[inline]
pub const fn area(pos: Point, size: Size) -> Area {
    Area::new(pos, size)
}



pub struct Area {
    pub pos: Point,
    pub size: Size,
}

impl Area {
    pub const ZERO: Self = Self::new(Point::ORIGIN, Size::ZERO);

    #[inline]
    pub const fn new(pos: Point, size: Size) -> Self {
        Self { pos, size }
    }

    #[inline]
    pub const fn from_pos(pos: Point) -> Self {
        Self {
            pos,
            size: Size::ZERO,
        }
    }

    #[inline]
    pub const fn from_size(size: Size) -> Self {
        Self {
            pos: Point::ORIGIN,
            size,
        }
    }
}
