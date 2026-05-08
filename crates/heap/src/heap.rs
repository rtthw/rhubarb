//! # Heap
//!
//! Userspace heap allocation.

#![no_std]
#![allow(internal_features)]
#![feature(rustc_attrs)] // Needed for `alloc` shim.

pub extern crate alloc;

mod alloc_shim;

use core::alloc::{GlobalAlloc, Layout};

pub use alloc::*;


// TODO: Choose a less arbitraty number.
pub const BASE_ADDR: usize = 0x2222_0000_0000;



#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

struct Allocator;

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        BASE_ADDR as *mut _
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}
