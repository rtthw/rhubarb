//! # Heap
//!
//! Userspace heap allocation.

#![no_std]
#![allow(internal_features)]
#![feature(rustc_attrs)] // Needed for `alloc` shim.

pub extern crate alloc;

pub use alloc::*;

mod alloc_shim;
mod llff;

use {
    core::{
        alloc::{GlobalAlloc, Layout},
        ptr::NonNull,
    },
    spin_mutex::Mutex,
};



// TODO: Choose a less arbitraty number.
pub const BASE_ADDR: usize = 0x2222_0000_0000;

pub fn init() {
    unsafe {
        ALLOCATOR.0.lock().init(BASE_ADDR, memory_types::GIBIBYTE);
    }
}

#[global_allocator]
static ALLOCATOR: Allocator = Allocator(Mutex::new(llff::Heap::empty()));

struct Allocator(Mutex<llff::Heap>);

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
