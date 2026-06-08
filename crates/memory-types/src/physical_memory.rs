//! # Physical Memory Address

use {
    crate::{Address, PAGE_SIZE},
    core::fmt,
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
    pub const fn from_base_addr(addr: Address) -> Option<Self> {
        if addr.is_page_aligned() {
            Some(Self::containing_addr(addr))
        } else {
            None
        }
    }

    #[inline]
    pub const fn containing_addr(addr: Address) -> Self {
        assert!(addr.is_physical());
        Self {
            number: addr.to_raw() / PAGE_SIZE,
        }
    }

    #[inline]
    pub const fn number(self) -> usize {
        self.number
    }

    #[inline]
    pub const fn base_addr(self) -> Address {
        unsafe { Address::new_unchecked(self.number * PAGE_SIZE) }
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



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        let frame = Frame::new(7);
        assert_eq!(frame.number(), 7);
        assert_eq!(*frame.base_addr(), 7 * PAGE_SIZE);

        let frame = Frame::containing_addr(Address::new(0x300111));
        assert_eq!(frame.number(), 0x300);
    }
}
