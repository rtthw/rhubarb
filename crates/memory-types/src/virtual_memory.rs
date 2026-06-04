//! # Virtual Memory

use {
    crate::{
        ENTRIES_PER_PAGE_TABLE, PAGE_SIZE, PAGE_TABLE_INDEX_WIDTH, PAGE_TABLE_OFFSET_WIDTH,
        align_down, align_up,
    },
    core::{
        fmt,
        ops::{Add, AddAssign, Deref, Sub, SubAssign},
    },
    l4_constants::*,
};

pub const MAX_VIRTUAL_ADDR: usize = usize::MAX; // 0xFFFF_FFFF_FFFF_FFFF
pub const VIRTUAL_MEMORY_SHIFT: usize = 47;
pub const VIRTUAL_MEMORY_OFFSET: usize = MAX_VIRTUAL_ADDR << VIRTUAL_MEMORY_SHIFT;

const L1_INDEX_SHIFT: usize = PAGE_TABLE_OFFSET_WIDTH + PAGE_TABLE_INDEX_WIDTH * 0;
const L2_INDEX_SHIFT: usize = PAGE_TABLE_OFFSET_WIDTH + PAGE_TABLE_INDEX_WIDTH * 1;
const L3_INDEX_SHIFT: usize = PAGE_TABLE_OFFSET_WIDTH + PAGE_TABLE_INDEX_WIDTH * 2;
const L4_INDEX_SHIFT: usize = PAGE_TABLE_OFFSET_WIDTH + PAGE_TABLE_INDEX_WIDTH * 3;

pub mod l4_constants {
    use super::{L4_INDEX_SHIFT, VIRTUAL_MEMORY_SHIFT};

    /// The maximum valid level 4 page table index a virtual address can have
    /// and be classified as a "physical" address (i.e. correspond to an
    /// identity-mapped virtual address).
    ///
    /// The top 12 bits of all physical addresses must be 0, so the maximum L4
    /// index is one that will never be sign-extended past bit 47 when converted
    /// to a virtual address. Therefore, the valid bit width of the L4 index
    /// range for physical memory is:
    ///
    /// ```rust,no_run
    /// VIRTUAL_MEMORY_SHIFT - L4_INDEX_SHIFT
    /// ```
    ///
    /// So, the max L4 index is:
    ///
    /// ```rust,no_run
    /// (1 << (VIRTUAL_MEMORY_SHIFT - L4_INDEX_SHIFT)) - 1
    /// ```
    pub const MAX_PHYSICAL_L4_INDEX: usize = (1 << (VIRTUAL_MEMORY_SHIFT - L4_INDEX_SHIFT)) - 1;
    pub const USER_STACK_L4_INDEX: usize = 507;
    pub const USER_HEAP_L4_INDEX: usize = 508;
    pub const KERNEL_HEAP_L4_INDEX: usize = 509;
    pub const KERNEL_MAPPING_L4_INDEX: usize = 510;
}

/// A virtual memory page.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Page {
    number: usize,
}

impl Page {
    #[inline]
    pub const fn new(number: usize) -> Self {
        Self { number }
    }

    #[inline]
    pub const fn containing_addr(addr: VirtualAddress) -> Self {
        Self {
            number: addr.to_raw() / PAGE_SIZE,
        }
    }

    #[inline]
    pub const fn from_table_indices(
        l4_index: usize,
        l3_index: usize,
        l2_index: usize,
        l1_index: usize,
    ) -> Self {
        Self::containing_addr(VirtualAddress::from_table_indices(
            l4_index, l3_index, l2_index, l1_index,
        ))
    }

    #[inline]
    pub const fn number(self) -> usize {
        self.number
    }

    #[inline]
    pub const fn base_addr(self) -> VirtualAddress {
        unsafe { VirtualAddress::new_unchecked(self.number * PAGE_SIZE) }
    }

    #[inline]
    pub const fn l1_index(self) -> usize {
        self.base_addr().l1_index()
    }

    #[inline]
    pub const fn l2_index(self) -> usize {
        self.base_addr().l2_index()
    }

    #[inline]
    pub const fn l3_index(self) -> usize {
        self.base_addr().l3_index()
    }

    #[inline]
    pub const fn l4_index(self) -> usize {
        self.base_addr().l4_index()
    }
}

impl fmt::Debug for Page {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Page #{} @ {:#x}", self.number(), self.base_addr())
    }
}

impl fmt::Display for Page {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Add<usize> for Page {
    type Output = Self;

    #[inline]
    fn add(self, rhs: usize) -> Self::Output {
        Self::new(self.number.checked_add(rhs).unwrap())
    }
}

impl AddAssign<usize> for Page {
    #[inline]
    fn add_assign(&mut self, rhs: usize) {
        *self = *self + rhs;
    }
}

impl Sub<usize> for Page {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: usize) -> Self::Output {
        Self::new(self.number.checked_sub(rhs).unwrap())
    }
}

impl SubAssign<usize> for Page {
    #[inline]
    fn sub_assign(&mut self, rhs: usize) {
        *self = *self - rhs;
    }
}



/// An address in virtual memory.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct VirtualAddress(usize);

impl VirtualAddress {
    #[inline]
    pub const fn new(addr: usize) -> Self {
        // Sign-extend the value by doing a right shift on it as an isize.
        Self(((addr << 16) as isize >> 16) as usize)
    }

    #[inline]
    pub const unsafe fn new_unchecked(addr: usize) -> Self {
        Self(addr)
    }

    #[inline]
    pub const fn from_table_indices(
        l4_index: usize,
        l3_index: usize,
        l2_index: usize,
        l1_index: usize,
    ) -> Self {
        Self::new(
            0 | (l4_index << L4_INDEX_SHIFT)
                | (l3_index << L3_INDEX_SHIFT)
                | (l2_index << L2_INDEX_SHIFT)
                | (l1_index << L1_INDEX_SHIFT),
        )
    }

    #[inline]
    pub const fn to_raw(self) -> usize {
        self.0
    }

    #[inline]
    pub const fn range(&self) -> AddressRange {
        match self.l4_index() {
            ..=MAX_PHYSICAL_L4_INDEX => AddressRange::Physical,
            USER_STACK_L4_INDEX => AddressRange::UserStack,
            USER_HEAP_L4_INDEX => AddressRange::UserHeap,
            KERNEL_HEAP_L4_INDEX => AddressRange::KernelHeap,
            KERNEL_MAPPING_L4_INDEX => AddressRange::KernelMapping,

            _ => AddressRange::Invalid,
        }
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
    pub const fn page(self) -> Page {
        Page::containing_addr(self)
    }

    #[inline]
    pub const fn page_offset(self) -> usize {
        self.0 % (1 << PAGE_TABLE_OFFSET_WIDTH)
    }

    #[inline]
    pub const fn l1_index(self) -> usize {
        (self.0 >> L1_INDEX_SHIFT) % ENTRIES_PER_PAGE_TABLE
    }

    #[inline]
    pub const fn l2_index(self) -> usize {
        (self.0 >> L2_INDEX_SHIFT) % ENTRIES_PER_PAGE_TABLE
    }

    #[inline]
    pub const fn l3_index(self) -> usize {
        (self.0 >> L3_INDEX_SHIFT) % ENTRIES_PER_PAGE_TABLE
    }

    #[inline]
    pub const fn l4_index(self) -> usize {
        (self.0 >> L4_INDEX_SHIFT) % ENTRIES_PER_PAGE_TABLE
    }
}

impl Deref for VirtualAddress {
    type Target = usize;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<usize> for VirtualAddress {
    #[inline]
    fn from(value: usize) -> Self {
        Self::new(value)
    }
}

impl fmt::Debug for VirtualAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x} (VIR)", self.0)
    }
}

impl fmt::Display for VirtualAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

impl fmt::Binary for VirtualAddress {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Binary::fmt(&self.0, f)
    }
}

impl fmt::Octal for VirtualAddress {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Octal::fmt(&self.0, f)
    }
}

impl fmt::LowerHex for VirtualAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::UpperHex for VirtualAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

impl Add<usize> for VirtualAddress {
    type Output = Self;

    #[inline]
    fn add(self, rhs: usize) -> Self::Output {
        Self::new(self.0.checked_add(rhs).unwrap())
    }
}

impl AddAssign<usize> for VirtualAddress {
    #[inline]
    fn add_assign(&mut self, rhs: usize) {
        *self = *self + rhs;
    }
}

impl Sub<usize> for VirtualAddress {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: usize) -> Self::Output {
        Self::new(self.0.checked_sub(rhs).unwrap())
    }
}

impl SubAssign<usize> for VirtualAddress {
    #[inline]
    fn sub_assign(&mut self, rhs: usize) {
        *self = *self - rhs;
    }
}



#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum AddressRange {
    Physical,
    UserStack,
    UserHeap,
    KernelHeap,
    KernelMapping,
    #[default]
    Invalid,
}



#[cfg(test)]
mod tests {
    use super::*;

    const USER_STACK_BASE: usize =
        VirtualAddress::from_table_indices(USER_STACK_L4_INDEX, 0, 0, 0).to_raw();
    const USER_HEAP_BASE: usize =
        VirtualAddress::from_table_indices(USER_HEAP_L4_INDEX, 0, 0, 0).to_raw();
    const KERNEL_HEAP_BASE: usize =
        VirtualAddress::from_table_indices(KERNEL_HEAP_L4_INDEX, 0, 0, 0).to_raw();
    const KERNEL_MAPPING_BASE: usize =
        VirtualAddress::from_table_indices(KERNEL_MAPPING_L4_INDEX, 0, 0, 0).to_raw();

    #[test]
    fn smoke() {
        let addr = VirtualAddress::new(PAGE_SIZE);
        assert!(addr.is_page_aligned());
        assert_eq!(*addr, PAGE_SIZE);

        let addr = VirtualAddress::new(0x3777);
        assert!(!addr.is_page_aligned());
        assert_eq!(*addr.page_align_down(), 0x3000);
        assert_eq!(*addr.page_align_up(), 0x4000);
        assert_eq!(addr.range(), AddressRange::Physical);

        let page = Page::new(7);
        assert_eq!(page.number(), 7);
        assert_eq!(*page.base_addr(), 7 * PAGE_SIZE);

        let page = Page::containing_addr(VirtualAddress::new(0x300111));
        assert_eq!(page.number(), 0x300);
    }

    #[test]
    fn ranges() {
        assert_eq!(
            VirtualAddress::new(USER_STACK_BASE).range(),
            AddressRange::UserStack,
        );
        assert_eq!(
            VirtualAddress::new(USER_HEAP_BASE).range(),
            AddressRange::UserHeap,
        );
        assert_eq!(
            VirtualAddress::new(KERNEL_HEAP_BASE).range(),
            AddressRange::KernelHeap,
        );
        assert_eq!(
            VirtualAddress::new(KERNEL_MAPPING_BASE).range(),
            AddressRange::KernelMapping,
        );

        assert_eq!(
            VirtualAddress::new(USER_STACK_BASE - 1).range(),
            AddressRange::Invalid,
        );
        assert_eq!(
            VirtualAddress::new(USER_HEAP_BASE - 1).range(),
            AddressRange::UserStack,
        );
        assert_eq!(
            VirtualAddress::new(KERNEL_HEAP_BASE - 1).range(),
            AddressRange::UserHeap,
        );
        assert_eq!(
            VirtualAddress::new(KERNEL_MAPPING_BASE - 1).range(),
            AddressRange::KernelHeap,
        );
    }

    #[test]
    fn addr_truncates() {
        let addr = VirtualAddress::new(0);
        assert_eq!(*addr, 0);
        let addr = VirtualAddress::new(1 << VIRTUAL_MEMORY_SHIFT);
        assert_eq!(*addr, VIRTUAL_MEMORY_OFFSET);
        let addr = VirtualAddress::new(43);
        assert_eq!(*addr, 43);
        let addr = VirtualAddress::new(5555 << VIRTUAL_MEMORY_SHIFT);
        assert_eq!(*addr, VIRTUAL_MEMORY_OFFSET);
    }

    #[test]
    #[should_panic]
    fn addr_overflow() {
        _ = VirtualAddress::new(MAX_VIRTUAL_ADDR) + 1;
    }

    #[test]
    #[should_panic]
    fn addr_underflow() {
        _ = VirtualAddress::new(0) - 1;
    }
}
