//! # Paging

use {
    crate::{Address, Frame, FrameAllocator, PAGE_SIZE, Page},
    core::{fmt, ops},
};


/// The size of an entry in a [`PageTable`].
pub const PAGE_TABLE_ENTRY_SIZE: usize = size_of::<PageTableEntry>();
/// The number of entries in a [`PageTable`].
pub const ENTRIES_PER_PAGE_TABLE: usize = PAGE_SIZE / PAGE_TABLE_ENTRY_SIZE;
/// The bit width of each page table index (9 bits).
pub const PAGE_TABLE_INDEX_WIDTH: usize = ENTRIES_PER_PAGE_TABLE.trailing_zeros() as usize;
/// The bit width of each page table offset (12 bits).
pub const PAGE_TABLE_OFFSET_WIDTH: usize = PAGE_SIZE.trailing_zeros() as usize;

/// The size of a huge page in a level 2 [`PageTable`].
pub const L2_HUGE_PAGE_SIZE: usize =
    ENTRIES_PER_PAGE_TABLE * ENTRIES_PER_PAGE_TABLE * PAGE_TABLE_ENTRY_SIZE;
/// The size of a huge page in a level 3 page table.
pub const L3_HUGE_PAGE_SIZE: usize = ENTRIES_PER_PAGE_TABLE
    * ENTRIES_PER_PAGE_TABLE
    * ENTRIES_PER_PAGE_TABLE
    * PAGE_TABLE_ENTRY_SIZE;

/// How many pages can fit within a huge page in a level 2 [`PageTable`].
pub const PAGES_PER_L2_HUGE_PAGE: usize = L2_HUGE_PAGE_SIZE / PAGE_SIZE;
/// How many pages can fit within a huge page in a level 3 [`PageTable`].
pub const PAGES_PER_L3_HUGE_PAGE: usize = L3_HUGE_PAGE_SIZE / PAGE_SIZE;

/// A table of [`Page`] mappings and permissions.
#[repr(align(4096))]
#[repr(C)]
#[derive(Clone)]
pub struct PageTable {
    entries: [PageTableEntry; ENTRIES_PER_PAGE_TABLE],
}

impl PageTable {
    /// Create an empty page table.
    #[inline]
    pub const fn new() -> Self {
        const EMPTY_ENTRY: PageTableEntry = PageTableEntry::new();

        Self {
            entries: [EMPTY_ENTRY; ENTRIES_PER_PAGE_TABLE],
        }
    }

    /// Set all entries to [`PageTableEntry::UNUSED`].
    #[inline]
    pub fn clear(&mut self) {
        for entry in self.iter_mut() {
            entry.set_unused();
        }
    }

    #[inline]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut PageTableEntry> {
        let ptr = self.entries.as_mut_ptr();
        (0..ENTRIES_PER_PAGE_TABLE).map(move |i| unsafe { &mut *ptr.add(i) })
    }
}

impl ops::Index<usize> for PageTable {
    type Output = PageTableEntry;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index]
    }
}

impl ops::IndexMut<usize> for PageTable {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.entries[index]
    }
}

bit_utils::bit_flags! {
    pub struct PageTableFlags: usize {
        PRESENT @ 0,
        WRITABLE @ 1,
        USER_ACCESSIBLE @ 2,
        WRITE_THROUGH @ 3,
        NO_CACHE @ 4,
        ACCESSED @ 5,
        DIRTY @ 6,
        HUGE_PAGE @ 7,
        GLOBAL @ 8,
        NO_EXECUTE @ 63,
    }
}

/// An entry within a [`PageTable`].
#[derive(Clone)]
#[repr(transparent)]
pub struct PageTableEntry {
    value: usize,
}

impl PageTableEntry {
    const ADDRESS_MASK: usize = 0x000F_FFFF_FFFF_F000;

    pub const UNUSED: Self = Self::new();

    #[inline]
    pub const fn new() -> Self {
        Self { value: 0 }
    }

    #[inline]
    pub const fn is_unused(&self) -> bool {
        self.value == 0
    }

    #[inline]
    pub const fn is_present(&self) -> bool {
        self.flags().get(PageTableFlags::PRESENT)
    }

    #[inline]
    pub const fn is_writable(&self) -> bool {
        self.flags().get(PageTableFlags::WRITABLE)
    }

    #[inline]
    pub const fn is_huge_page(&self) -> bool {
        self.flags().get(PageTableFlags::HUGE_PAGE)
    }

    /// The [`Address`] mapped by this entry, might be zero.
    #[inline]
    pub fn addr(&self) -> Address {
        Address::new(self.value & Self::ADDRESS_MASK)
    }

    /// The [`Frame`] mapped by this entry.
    #[inline]
    pub fn frame(&self) -> Result<Frame, EntryFrameError> {
        if !self.is_present() {
            Err(EntryFrameError::NotPresent)
        } else if self.is_huge_page() {
            Err(EntryFrameError::HugePage)
        } else {
            Ok(Frame::containing_addr(self.addr()))
        }
    }

    /// The [`PageTableFlags`] of this entry.
    #[inline]
    pub const fn flags(&self) -> PageTableFlags {
        PageTableFlags(self.value & !Self::ADDRESS_MASK)
    }

    /// Set this entry to [`PageTableEntry::UNUSED`].
    #[inline]
    pub fn set_unused(&mut self) {
        self.value = 0;
    }

    /// Set this entry's [`Address`] with the given [`PageTableFlags`].
    #[inline]
    pub fn set_addr(&mut self, addr: Address, flags: PageTableFlags) {
        assert!(addr.is_page_aligned() && addr.is_physical());
        self.value = (addr.to_raw()) | flags.bits();
    }

    /// Set this entry's [`Frame`] with the given [`PageTableFlags`].
    #[inline]
    pub fn set_frame(&mut self, frame: Frame, flags: PageTableFlags) {
        assert!(flags & PageTableFlags::HUGE_PAGE != PageTableFlags::HUGE_PAGE);
        self.set_addr(frame.base_addr(), flags)
    }

    /// Set this entry's [`PageTableFlags`].
    #[inline]
    pub fn set_flags(&mut self, flags: PageTableFlags) {
        self.value = self.addr().to_raw() | flags.bits();
    }
}

impl fmt::Debug for PageTableEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut f = f.debug_struct("PageTableEntry");
        f.field("addr", &self.addr());
        f.field("flags", &self.flags());
        f.finish()
    }
}



/// An **exclusive** range of [`Page`]s.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PageRange {
    pub start: Page,
    pub end: Page,
}

impl PageRange {
    /// Create a page range that starts at the given start page, and ends
    /// **before** the given end page.
    #[inline]
    pub const fn new(start: Page, end: Page) -> Self {
        Self { start, end }
    }

    /// Create a page range that starts at the given page, with the given
    /// length.
    #[inline]
    pub fn from_start_len(start: Page, len: usize) -> Self {
        Self {
            start,
            end: start + len,
        }
    }

    /// Create a page range that ends **before** the given end page, with the
    /// given size.
    #[inline]
    pub fn from_end_size(end: Page, size: usize) -> Self {
        let len = size.div_ceil(PAGE_SIZE);
        Self {
            start: end - len,
            end,
        }
    }

    /// Create a page range that starts at the page containing the given base
    /// address, with the given size.
    #[inline]
    pub fn from_base_size(base: Address, size: usize) -> Self {
        let start = Page::containing_addr(base);
        let len = size.div_ceil(PAGE_SIZE);
        Self {
            start,
            end: start + len,
        }
    }

    /// Create a page range that starts at the page containing the given base
    /// address, with the given length.
    #[inline]
    pub fn from_base_len(base: Address, len: usize) -> Self {
        Self {
            start: Page::containing_addr(base),
            end: Page::containing_addr(base) + len,
        }
    }

    /// The number of pages in this range.
    #[inline]
    pub const fn len(&self) -> usize {
        self.end.number() - self.start.number()
    }
}

impl Iterator for PageRange {
    type Item = Page;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.start < self.end {
            let page = self.start;
            self.start += 1;
            Some(page)
        } else {
            None
        }
    }
}



pub struct Level4PageTable {
    inner: &'static mut PageTable,
}

impl Level4PageTable {
    pub unsafe fn new(table: &'static mut PageTable) -> Self {
        Self { inner: table }
    }

    pub fn set_flags<A>(
        &mut self,
        pages: PageRange,
        flags: PageTableFlags,
        allocator: &mut A,
    ) -> Result<(), MappingError>
    where
        A: FrameAllocator + ?Sized,
    {
        let parent_table_flags = flags
            & (PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE);

        let l4 = &mut self.inner;

        let mut page_iter = pages.into_iter();
        while let Some(page) = page_iter.next() {
            let l3 = Self::next_table(&mut l4[page.l4_index()], parent_table_flags, allocator)?;
            if l3[page.l3_index()].is_huge_page() {
                l3[page.l3_index()].set_flags(flags | PageTableFlags::HUGE_PAGE);
                FlushMapping(page).flush();

                for _ in 0..PAGES_PER_L3_HUGE_PAGE {
                    if page_iter.next().is_none() {
                        break;
                    }
                }

                continue;
            }

            let l2 = Self::next_table(&mut l3[page.l3_index()], parent_table_flags, allocator)?;
            if l2[page.l2_index()].is_huge_page() {
                l2[page.l2_index()].set_flags(flags | PageTableFlags::HUGE_PAGE);
                FlushMapping(page).flush();

                for _ in 0..PAGES_PER_L2_HUGE_PAGE {
                    if page_iter.next().is_none() {
                        break;
                    }
                }

                continue;
            }

            let l1 = Self::next_table(&mut l2[page.l2_index()], parent_table_flags, allocator)?;
            if !l1[page.l1_index()].is_unused() {
                l1[page.l1_index()].set_flags(flags);
                FlushMapping(page).flush();
            }
        }

        Ok(())
    }

    pub fn map_to<A>(
        &mut self,
        page: Page,
        frame: Frame,
        flags: PageTableFlags,
        allocator: &mut A,
    ) -> Result<FlushMapping, MappingError>
    where
        A: FrameAllocator + ?Sized,
    {
        let parent_table_flags = flags
            & (PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE);

        let l4 = &mut self.inner;
        let l3 = Self::next_table(&mut l4[page.l4_index()], parent_table_flags, allocator)?;
        let l2 = Self::next_table(&mut l3[page.l3_index()], parent_table_flags, allocator)?;
        let l1 = Self::next_table(&mut l2[page.l2_index()], parent_table_flags, allocator)?;

        if !l1[page.l1_index()].is_unused() {
            return Err(MappingError::PageAlreadyMapped { to: frame });
        }
        l1[page.l1_index()].set_frame(frame, flags);

        Ok(FlushMapping(page))
    }

    fn next_table<'a, A>(
        entry: &'a mut PageTableEntry,
        flags: PageTableFlags,
        allocator: &mut A,
    ) -> Result<&'a mut PageTable, MappingError>
    where
        A: FrameAllocator + ?Sized,
    {
        let new = entry.is_unused();
        if new {
            if let Some(frame) = allocator.allocate_frame() {
                entry.set_frame(frame, flags);
            } else {
                return Err(MappingError::FrameAllocationFailed);
            }
        } else {
            if flags != PageTableFlags::NONE && entry.flags() & flags != flags {
                entry.set_flags(entry.flags() | flags);
            }
        }

        let page_table = match Self::get_table(entry) {
            Ok(table) => table,
            Err(EntryFrameError::HugePage) => {
                return Err(MappingError::MappedToHugePage);
            }
            Err(EntryFrameError::NotPresent) => panic!("entry should be mapped at this point"),
        };

        if new {
            page_table.clear();
        }

        Ok(page_table)
    }

    fn get_table(entry: &PageTableEntry) -> Result<&mut PageTable, EntryFrameError> {
        let page_table_ptr = entry.frame()?.base_addr().to_raw() as *mut PageTable;
        Ok(unsafe { &mut *page_table_ptr })
    }

    pub fn translate_page(&self, page: Page) -> Result<Frame, TranslationError> {
        let l4 = &self.inner;
        let l3 = Self::get_table(&l4[page.l4_index()])?;
        let l2 = Self::get_table(&l3[page.l3_index()])?;
        let l1 = Self::get_table(&l2[page.l2_index()])?;

        let l1_entry = &l1[page.l1_index()];
        if l1_entry.is_unused() {
            return Err(TranslationError::NotMapped);
        }

        Frame::from_base_addr(l1_entry.addr())
            .ok_or(TranslationError::InvalidFrameAddress(l1_entry.addr()))
    }

    pub fn translate_addr(&self, addr: Address) -> Result<AddressTranslation, TranslationError> {
        let l4 = &self.inner;
        let l3 = Self::get_table(&l4[addr.l4_index()])?;
        let l2 = Self::get_table(&l3[addr.l3_index()])?;
        let l1 = Self::get_table(&l2[addr.l2_index()])?;

        let l1_entry = &l1[addr.l1_index()];
        if l1_entry.is_unused() {
            return Err(TranslationError::NotMapped);
        }

        Ok(AddressTranslation {
            frame: Frame::from_base_addr(l1_entry.addr())
                .ok_or(TranslationError::InvalidFrameAddress(l1_entry.addr()))?,
            offset: addr.page_offset(),
            flags: l1_entry.flags(),
        })
    }
}

impl ops::Deref for Level4PageTable {
    type Target = PageTable;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl ops::DerefMut for Level4PageTable {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

/// The result of calling [`Level4PageTable::translate`].
#[derive(Debug)]
pub struct AddressTranslation {
    pub frame: Frame,
    pub offset: usize,
    pub flags: PageTableFlags,
}



#[derive(Debug)]
#[must_use = "changes to page tables must be flushed or ignored"]
pub struct FlushMapping(Page);

impl FlushMapping {
    /// Flush the page from the TLB to ensure that the newest mapping is used.
    #[inline]
    pub fn flush(self) {
        unsafe {
            core::arch::asm!(
                "invlpg [{}]",
                in(reg) self.0.base_addr().to_raw(),
                options(nostack, preserves_flags),
            );
        }
    }

    /// Don't flush the TLB and silence the “must be used” warning.
    #[inline]
    pub fn ignore(self) {}

    /// The page to be flushed.
    #[inline]
    pub fn page(&self) -> Page {
        self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub enum MappingError {
    PageAlreadyMapped { to: Frame },
    FrameAllocationFailed,
    MappedToHugePage,
}

impl fmt::Display for MappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PageAlreadyMapped { to } => {
                f.write_fmt(format_args!("page already mapped to {to}"))
            }
            Self::FrameAllocationFailed => f.write_str("failed to allocate frame for page"),
            Self::MappedToHugePage => f.write_str("got huge page where standard page was expected"),
        }
    }
}

impl core::error::Error for MappingError {}

#[derive(Debug)]
pub enum TranslationError {
    NotMapped,
    HugePage,
    InvalidFrameAddress(Address),
}

impl From<EntryFrameError> for TranslationError {
    fn from(value: EntryFrameError) -> Self {
        match value {
            EntryFrameError::NotPresent => Self::NotMapped,
            EntryFrameError::HugePage => Self::HugePage,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum EntryFrameError {
    NotPresent,
    HugePage,
}

impl fmt::Display for EntryFrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotPresent => f.write_str("entry does not have the `PRESENT` flag set"),
            Self::HugePage => f.write_str("entry has the `HUGE_PAGE` flag set"),
        }
    }
}

impl core::error::Error for EntryFrameError {}
