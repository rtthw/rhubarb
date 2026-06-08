//! # Memory Types
//!
//! All values are expressed in terms of bytes.

#![no_std]

pub mod paging;

mod physical_memory;
mod virtual_memory;

pub use {
    paging::{
        AddressTranslation, ENTRIES_PER_PAGE_TABLE, Level4PageTable, PAGE_TABLE_INDEX_WIDTH,
        PAGE_TABLE_OFFSET_WIDTH, PageRange, PageTable, PageTableEntry, PageTableFlags,
    },
    physical_memory::{Frame, MAX_PHYSICAL_ADDR},
    virtual_memory::{
        Address, AddressDomain, MAX_VIRTUAL_ADDR, Page, VIRTUAL_MEMORY_OFFSET,
        VIRTUAL_MEMORY_SHIFT, l4_constants::*,
    },
};


/// A "kibibyte", or 1,024 bytes.
pub const KIBIBYTE: usize = 1024;
/// A "mebibyte", or 1,024² bytes.
pub const MEBIBYTE: usize = 1024 * 1024;
/// A "gibibyte", or 1,024³ bytes.
pub const GIBIBYTE: usize = 1024 * 1024 * 1024;
/// A "tebibyte", or 1,024⁴ bytes.
pub const TEBIBYTE: usize = 1024 * 1024 * 1024 * 1024;
/// A "pebibyte", or 1,024⁵ bytes.
pub const PEBIBYTE: usize = 1024 * 1024 * 1024 * 1024 * 1024;

/// A "kilobyte", or 1,000 (one thousand) bytes.
pub const KILOBYTE: usize = 1000;
/// A "megabyte", or 1,000² (one million) bytes.
pub const MEGABYTE: usize = 1000 * 1000;
/// A "gigabyte", or 1,000³ (one billion) bytes.
pub const GIGABYTE: usize = 1000 * 1000 * 1000;
/// A "terabyte", or 1,000⁴ (one trillion) bytes.
pub const TERABYTE: usize = 1000 * 1000 * 1000 * 1000;
/// A "petabyte", or 1,000⁵ (one quadrillion) bytes.
pub const PETABYTE: usize = 1000 * 1000 * 1000 * 1000 * 1000;

/// 4 KiB, or 4,096 bytes.
pub const PAGE_SIZE: usize = 4 * KIBIBYTE;
/// 8 bytes.
pub const POINTER_SIZE: usize = size_of::<usize>();



#[inline]
pub const fn align_down(addr: usize, align: usize) -> usize {
    addr & !(align - 1)
}

#[inline]
pub const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}



pub trait FrameAllocator {
    fn allocate_frame(&mut self) -> Option<Frame>;
}
