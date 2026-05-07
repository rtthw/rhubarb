//! # Heap
//!
//! Userspace heap allocation.

#![no_std]
#![allow(internal_features)]
#![feature(rustc_attrs)]

pub extern crate alloc;

use {
    core::alloc::{GlobalAlloc, Layout},
    memory_types::{PageRange, VirtualAddress},
};

pub use alloc::*;


const HEAP_BASE_ADDR: usize = 0x2222_0000_0000;

pub fn init() -> Result<(), AllocPagesError> {
    alloc_pages(1)?;
    Ok(())
}



#[unsafe(export_name = "__rustc::__rust_no_alloc_shim_is_unstable_v2")]
pub fn __rust_no_alloc_shim_is_unstable_v2() {}

#[rustc_std_internal_symbol]
pub fn __rust_alloc_error_handler_should_panic() -> u8 {
    0
}

#[rustc_std_internal_symbol]
pub fn __rust_alloc_error_handler(_size: usize, _align: usize) {}



/// Allocate `page_count` pages on the heap.
pub fn alloc_pages(page_count: u64) -> Result<PageRange, AllocPagesError> {
    let mut addr: i64 = 3;
    unsafe {
        core::arch::asm!(
            "int 0x42",
            inout("rax") addr,
            in("rdi") page_count,
            options(nostack),
        );
    }

    if addr < 0 {
        Err(match addr {
            -1 => AllocPagesError::InvalidPageCount,
            -2 => AllocPagesError::InternalMappingFailure,
            _ => unreachable!(),
        })
    } else {
        let base = VirtualAddress::new(addr as usize);
        let len = page_count as usize;

        Ok(PageRange::from_base_len(base, len))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AllocPagesError {
    InvalidPageCount,
    InternalMappingFailure,
}

impl fmt::Display for AllocPagesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                AllocPagesError::InvalidPageCount =>
                    "attempted to allocate an invalid number of pages (must be between 1 and 4096)",
                AllocPagesError::InternalMappingFailure =>
                    "kernel failed to map allocated pages into this address space",
            }
        )
    }
}

impl core::error::Error for AllocPagesError {}



#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

struct Allocator;

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        HEAP_BASE_ADDR as *mut _
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}
