//! # Size

use {
    crate::Axis,
    core::ops::{Add, AddAssign, Mul, Neg, Sub},
};



/// Shorthand for [`Size::new`].
#[inline]
pub const fn size(width: f32, height: f32) -> Size {
    Size::new(width, height)
}



#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

// Constructors.
impl Size {
    pub const ZERO: Self = Self::new(0.0, 0.0);
    pub const ONE: Self = Self::new(1.0, 1.0);

    #[inline]
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    /// Shorthand for `Self::new(value, value)`.
    #[inline]
    pub const fn splat(value: f32) -> Self {
        Self::new(value, value)
    }

    #[inline]
    pub const fn from_width(width: f32) -> Self {
        Self { width, height: 0.0 }
    }

    #[inline]
    pub const fn from_height(height: f32) -> Self {
        Self { width: 0.0, height }
    }
}

// Utilities.
impl Size {
    #[inline]
    pub const fn axis_value(self, axis: Axis) -> f32 {
        match axis {
            Axis::Horizontal => self.width,
            Axis::Vertical => self.height,
        }
    }

    #[inline]
    pub const fn scale(self, value: f32) -> Self {
        Self::new(self.width * value, self.height * value)
    }

    #[inline]
    pub const fn const_add(self, other: Self) -> Self {
        Self::new(self.width + other.width, self.height + other.height)
    }

    #[inline]
    pub const fn const_sub(self, other: Self) -> Self {
        Self::new(self.width - other.width, self.height - other.height)
    }

    #[inline]
    pub const fn min(self, other: Self) -> Self {
        Self::new(self.width.min(other.width), self.height.min(other.height))
    }

    #[inline]
    pub const fn max(self, other: Self) -> Self {
        Self::new(self.width.max(other.width), self.height.max(other.height))
    }

    /// Shorthand for `self.max(min).min(max)`.
    #[inline]
    pub const fn clamp(self, min: Self, max: Self) -> Self {
        self.max(min).min(max)
    }

    #[inline]
    pub const fn lerp(self, other: Self, value: f32) -> Self {
        self.scale(1.0 - value).const_add(other.scale(value))
    }
}



impl Add for Size {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        self.const_add(rhs)
    }
}

impl AddAssign for Size {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.width += rhs.width;
        self.height += rhs.height;
    }
}

impl Sub for Size {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        self.const_sub(rhs)
    }
}

impl Mul<f32> for Size {
    type Output = Self;

    #[inline]
    fn mul(self, value: f32) -> Self::Output {
        self.scale(value)
    }
}

impl Neg for Size {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self::Output {
        Self::new(-self.width, -self.height)
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        assert_eq!(Size::ONE - Size::ONE, Size::ZERO);
        assert_eq!(Size::ONE + (-Size::ONE), Size::ZERO);
    }
}
