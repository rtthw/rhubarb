//! # Heap
//!
//! Userspace heap allocation.

#![no_std]
#![allow(internal_features)]
#![feature(rustc_attrs)] // Needed for `alloc` shim.

pub extern crate alloc;

mod alloc_shim;
mod llff;

use {
    core::{
        alloc::{GlobalAlloc, Layout},
        ptr::NonNull,
    },
    memory_types::AddressDomain,
    spin_mutex::Mutex,
};

pub use alloc::*;



// TODO: Choose a less arbitrary number.
pub const BASE_ADDR: usize = AddressDomain::UserHeap.base_addr().to_raw();
pub const DEFAULT_SIZE: usize = 8 * memory_types::MEBIBYTE;

pub struct Allocator(Mutex<llff::Heap>);

impl Allocator {
    pub const fn new() -> Self {
        Self(Mutex::new(llff::Heap::empty()))
    }

    pub unsafe fn init(&self, base_addr: usize, size: usize) {
        unsafe {
            self.0.lock().init(base_addr, size);
        }
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.0
            .lock()
            .allocate_first_fit(layout)
            .ok()
            .map_or(0 as *mut u8, |allocation| allocation.as_ptr())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe {
            self.0
                .lock()
                .deallocate(NonNull::new_unchecked(ptr), layout)
        }
    }
}
