//! # Physical Memory Address

use {
    crate::{PAGE_SIZE, align_down, align_up},
    core::{
        fmt,
        ops::{Add, AddAssign, Deref, Sub, SubAssign},
    },
};

pub const MAX_PHYSICAL_ADDR: usize = 0x000F_FFFF_FFFF_FFFF;

/// A physical memory frame.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Frame {
    number: usize,
}

impl Frame {
    #[inline]
    pub const fn new(number: usize) -> Self {
        Self { number }
    }

    #[inline]
    pub const fn from_base_addr(addr: PhysicalAddress) -> Option<Self> {
        if addr.is_page_aligned() {
            Some(Self::containing_addr(addr))
        } else {
            None
        }
    }

    #[inline]
    pub const fn containing_addr(addr: PhysicalAddress) -> Self {
        Self {
            number: addr.to_raw() / PAGE_SIZE,
        }
    }

    #[inline]
    pub const fn number(self) -> usize {
        self.number
    }

    #[inline]
    pub const fn base_addr(self) -> PhysicalAddress {
        unsafe { PhysicalAddress::new_unchecked(self.number * PAGE_SIZE) }
    }
}

impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Frame #{} @ {:#x}", self.number(), self.base_addr())
    }
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}



/// An address in physical memory.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct PhysicalAddress(usize);

impl PhysicalAddress {
    #[inline]
    pub const fn new(addr: usize) -> Self {
        Self(addr & MAX_PHYSICAL_ADDR)
    }

    #[inline]
    pub const unsafe fn new_unchecked(addr: usize) -> Self {
        Self(addr)
    }

    #[inline]
    pub const fn to_raw(self) -> usize {
        self.0
    }

    #[inline]
    pub const fn is_page_aligned(self) -> bool {
        self.0 & (PAGE_SIZE - 1) == 0
    }

    #[inline]
    pub const fn page_align_down(self) -> Self {
        Self(align_down(self.0, PAGE_SIZE))
    }

    #[inline]
    pub const fn page_align_up(self) -> Self {
        Self(align_up(self.0, PAGE_SIZE))
    }

    #[inline]
    pub const fn frame(self) -> Frame {
        Frame::containing_addr(self)
    }
}

impl Deref for PhysicalAddress {
    type Target = usize;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<usize> for PhysicalAddress {
    #[inline]
    fn from(value: usize) -> Self {
        Self::new(value)
    }
}

impl fmt::Debug for PhysicalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x} (PHY)", self.0)
    }
}

impl fmt::Display for PhysicalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

impl fmt::Binary for PhysicalAddress {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Binary::fmt(&self.0, f)
    }
}

impl fmt::Octal for PhysicalAddress {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Octal::fmt(&self.0, f)
    }
}

impl fmt::LowerHex for PhysicalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::UpperHex for PhysicalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

impl Add<usize> for PhysicalAddress {
    type Output = Self;

    #[inline]
    fn add(self, rhs: usize) -> Self::Output {
        Self::new(self.0.checked_add(rhs).unwrap())
    }
}

impl AddAssign<usize> for PhysicalAddress {
    #[inline]
    fn add_assign(&mut self, rhs: usize) {
        *self = *self + rhs;
    }
}

impl Sub<usize> for PhysicalAddress {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: usize) -> Self::Output {
        Self::new(self.0.checked_sub(rhs).unwrap())
    }
}

impl SubAssign<usize> for PhysicalAddress {
    #[inline]
    fn sub_assign(&mut self, rhs: usize) {
        *self = *self - rhs;
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        let addr = PhysicalAddress::new(PAGE_SIZE);
        assert!(addr.is_page_aligned());
        assert_eq!(*addr, PAGE_SIZE);

        let addr = PhysicalAddress::new(0x3777);
        assert!(!addr.is_page_aligned());
        assert_eq!(*addr.page_align_down(), 0x3000);
        assert_eq!(*addr.page_align_up(), 0x4000);

        let frame = Frame::new(7);
        assert_eq!(frame.number(), 7);
        assert_eq!(*frame.base_addr(), 7 * PAGE_SIZE);

        let frame = Frame::containing_addr(PhysicalAddress::new(0x300111));
        assert_eq!(frame.number(), 0x300);
    }

    #[test]
    fn addr_truncates() {
        let addr = PhysicalAddress::new(0x111F_FFFF_FFFF_FFFF);
        assert_eq!(*addr, MAX_PHYSICAL_ADDR);
        let addr = PhysicalAddress::new(MAX_PHYSICAL_ADDR);
        assert_eq!(*addr, MAX_PHYSICAL_ADDR);
        let addr = PhysicalAddress::new(MAX_PHYSICAL_ADDR + 1);
        assert_eq!(*addr, 0);
        let addr = PhysicalAddress::new(MAX_PHYSICAL_ADDR - 1);
        assert_eq!(*addr, MAX_PHYSICAL_ADDR - 1);
    }

    #[test]
    #[should_panic]
    fn addr_overflow() {
        _ = PhysicalAddress::new(MAX_PHYSICAL_ADDR) + 0xFFF0_0000_0000_0001;
    }

    #[test]
    #[should_panic]
    fn addr_underflow() {
        _ = PhysicalAddress::new(0) - 1;
    }
}
