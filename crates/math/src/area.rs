//! # Area

use crate::{Point, Size};



/// Shorthand for [`Area::new`].
#[inline]
pub const fn area(pos: Point, size: Size) -> Area {
    Area::new(pos, size)
}



#[derive(Clone, Copy, Debug, Default, PartialEq)]
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

    #[inline]
    pub const fn with_pos(self, pos: Point) -> Self {
        Self {
            pos,
            size: self.size,
        }
    }

    #[inline]
    pub const fn with_size(self, size: Size) -> Self {
        Self {
            pos: self.pos,
            size,
        }
    }

    #[inline]
    pub const fn set_pos(&mut self, pos: Point) {
        self.pos = pos;
    }

    #[inline]
    pub const fn set_size(&mut self, size: Size) {
        self.size = size;
    }
}

impl Area {
    #[inline]
    pub const fn size(&self) -> Size {
        self.size
    }

    #[inline]
    pub const fn width(&self) -> f32 {
        self.size.width
    }

    #[inline]
    pub const fn height(&self) -> f32 {
        self.size.height
    }

    #[inline]
    pub const fn position(&self) -> Point {
        self.pos
    }

    #[inline]
    pub const fn x(&self) -> f32 {
        self.pos.x
    }

    #[inline]
    pub const fn y(&self) -> f32 {
        self.pos.y
    }

    #[inline]
    pub const fn max_x(&self) -> f32 {
        self.pos.x + self.size.width
    }

    #[inline]
    pub const fn max_y(&self) -> f32 {
        self.pos.y + self.size.height
    }
}

impl Area {
    /// # Examples
    ///
    /// ```
    /// use math::{Area, Point, Size};
    ///
    /// let area = Area::from_size(Size::new(1.0, 1.0));
    ///
    /// assert!(area.contains(Point::ORIGIN));
    /// assert!(!area.contains(Point::new(1.0, 1.0)));
    /// assert!(!area.contains(Point::new(0.0, 1.0)));
    /// assert!(!area.contains(Point::new(1.0, 0.0)));
    /// ```
    #[inline]
    pub const fn contains(&self, point: Point) -> bool {
        (self.x() <= point.x)
            & (self.y() <= point.y)
            & (point.x < self.max_x())
            & (point.y < self.max_y())
    }

    /// # Examples
    ///
    /// ```
    /// use math::{Area, Point, Size};
    ///
    /// let area_1 = Area::new(Point::new(-1.0, -1.0), Size::new(2.0, 2.0));
    /// let area_2 = Area::new(Point::new(0.0, 0.0), Size::new(1.0, 1.0));
    /// let area_3 = Area::new(Point::new(1.0, 1.0), Size::new(1.0, 1.0));
    ///
    /// assert!(area_1.intersects(&area_2));
    /// assert!(!area_1.intersects(&area_3));
    /// ```
    #[inline]
    pub const fn intersects(&self, other: &Self) -> bool {
        (self.x() < other.max_x())
            & (self.y() < other.max_y())
            & (self.max_x() > other.x())
            & (self.max_y() > other.y())
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn containment() {
        let area = Area::new(Point::new(-5.0, -2.0), Size::new(10.0, 10.0));
        assert!(area.contains(Point::new(1.0, 1.0)));
        assert!(area.contains(Point::new(-5.0, -2.0)));
        assert!(area.contains(Point::new(4.0, 7.0)));
        assert!(!area.contains(Point::new(5.0, 8.0)));
        assert!(!area.contains(Point::new(-5.0, -3.0)));
        assert!(!area.contains(Point::new(-6.0, -2.0)));
    }

    #[test]
    fn intersection() {
        let area_1 = Area::new(Point::new(-5.0, -2.0), Size::new(10.0, 10.0));
        let area_2 = Area::new(Point::new(4.0, 7.0), Size::new(1.0, 1.0));
        let area_3 = Area::new(Point::new(5.0, 8.0), Size::new(1.0, 1.0));
        assert!(area_1.intersects(&area_2));
        assert!(!area_1.intersects(&area_3));
        assert!(!area_2.intersects(&area_3));
    }
}
