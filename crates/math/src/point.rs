//! # Point

use {
    crate::{Axis, Size},
    core::ops::{Add, AddAssign, Neg, Sub},
};



/// Shorthand for [`Point::new`].
#[inline]
pub const fn point(x: f32, y: f32) -> Point {
    Point::new(x, y)
}



#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

// Constructors.
impl Point {
    pub const ORIGIN: Self = Self::new(0.0, 0.0);
    pub const ONE_ONE: Self = Self::new(1.0, 1.0);

    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Shorthand for `Self::new(value, value)`.
    #[inline]
    pub const fn splat(value: f32) -> Self {
        Self::new(value, value)
    }
}

// Utilities.
impl Point {
    #[inline]
    pub const fn axis_value(self, axis: Axis) -> f32 {
        match axis {
            Axis::Horizontal => self.x,
            Axis::Vertical => self.y,
        }
    }

    #[inline]
    pub const fn const_add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }

    #[inline]
    pub const fn const_sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }

    #[inline]
    pub const fn min(self, other: Self) -> Self {
        Self::new(self.x.min(other.x), self.y.min(other.y))
    }

    #[inline]
    pub const fn max(self, other: Self) -> Self {
        Self::new(self.x.min(other.x), self.y.min(other.y))
    }

    /// Shorthand for `self.max(min).min(max)`.
    #[inline]
    pub const fn clamp(self, min: Self, max: Self) -> Self {
        self.max(min).min(max)
    }

    #[inline]
    pub const fn lerp(self, other: Self, value: f32) -> Self {
        let inv = 1.0 - value;

        Self::new(
            inv * self.x + value * other.x,
            inv * self.y + value * other.y,
        )
    }
}



impl Add for Point {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        self.const_add(rhs)
    }
}

impl Add<Size> for Point {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Size) -> Self::Output {
        Self::new(self.x + rhs.width, self.y + rhs.height)
    }
}

impl AddAssign<Size> for Point {
    #[inline]
    fn add_assign(&mut self, rhs: Size) {
        self.x += rhs.width;
        self.y += rhs.height;
    }
}

impl Sub for Point {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        self.const_sub(rhs)
    }
}

impl Neg for Point {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self::Output {
        Self::new(-self.x, -self.y)
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        assert_eq!(Point::ONE_ONE - Point::ONE_ONE, Point::ORIGIN);
        assert_eq!(Point::ONE_ONE + (-Point::ONE_ONE), Point::ORIGIN);
    }
}
