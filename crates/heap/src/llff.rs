/*
    Copyright (c) 2016 Philipp Oppermann

    Permission is hereby granted, free of charge, to any
    person obtaining a copy of this software and associated
    documentation files (the "Software"), to deal in the
    Software without restriction, including without
    limitation the rights to use, copy, modify, merge,
    publish, distribute, sublicense, and/or sell copies of
    the Software, and to permit persons to whom the Software
    is furnished to do so, subject to the following
    conditions:

    The above copyright notice and this permission notice
    shall be included in all copies or substantial portions
    of the Software.

    THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
    ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
    TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
    PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
    SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
    CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
    OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
    IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
    DEALINGS IN THE SOFTWARE.
*/

//! # Linked-List First-Fit (LLFF)
//!
//! Implementation adapted from [the `linked_list_allocator` crate].
//!
//! [the `linked_list_allocator` crate]: https://crates.io/crates/linked_list_allocator

use {
    core::{alloc::Layout, ptr::NonNull},
    memory_types::align_up,
};


const MIN_ALLOC_SIZE: usize = size_of::<Hole>();

pub struct Heap {
    used: usize,
    holes: HoleList,
}

impl Heap {
    pub const fn empty() -> Self {
        Self {
            used: 0,
            holes: HoleList::empty(),
        }
    }

    pub unsafe fn init(&mut self, heap_bottom: usize, heap_size: usize) {
        self.used = 0;
        unsafe {
            self.holes.init(heap_bottom, heap_size);
        }
    }

    pub fn allocate_first_fit(&mut self, layout: Layout) -> Result<NonNull<u8>, ()> {
        match self.holes.allocate_first_fit(layout) {
            Ok((ptr, aligned_layout)) => {
                self.used += aligned_layout.size();
                Ok(ptr)
            }
            Err(err) => Err(err),
        }
    }

    pub unsafe fn deallocate(&mut self, ptr: NonNull<u8>, layout: Layout) {
        self.used -= unsafe { self.holes.deallocate(ptr, layout).size() };
    }
}

pub struct HoleList {
    first: Hole,
}

impl HoleList {
    pub const fn empty() -> Self {
        Self {
            first: Hole {
                size: 0,
                next: None,
            },
        }
    }

    unsafe fn init(&mut self, hole_addr: usize, hole_size: usize) {
        let aligned_hole_addr = align_up(hole_addr, align_of::<Hole>());
        let ptr = aligned_hole_addr as *mut Hole;
        unsafe {
            ptr.write(Hole {
                size: hole_size.saturating_sub(aligned_hole_addr - hole_addr),
                next: None,
            });
        }

        self.first = Hole {
            size: 0,
            next: Some(unsafe { &mut *ptr }),
        };
    }

    fn allocate_first_fit(&mut self, layout: Layout) -> Result<(NonNull<u8>, Layout), ()> {
        let aligned_layout = align_layout_for_hole(layout);

        allocate_first_fit(&mut self.first, aligned_layout).map(|hole_info| {
            (
                NonNull::new(hole_info.addr as *mut u8).unwrap(),
                aligned_layout,
            )
        })
    }

    unsafe fn deallocate(&mut self, ptr: NonNull<u8>, layout: Layout) -> Layout {
        let aligned_layout = align_layout_for_hole(layout);
        deallocate(
            &mut self.first,
            ptr.as_ptr() as usize,
            aligned_layout.size(),
        );
        aligned_layout
    }
}

fn align_layout_for_hole(layout: Layout) -> Layout {
    let mut size = layout.size();
    if size < MIN_ALLOC_SIZE {
        size = MIN_ALLOC_SIZE;
    }
    let size = align_up(size, align_of::<Hole>());
    let layout = Layout::from_size_align(size, layout.align()).unwrap();

    layout
}

struct Hole {
    size: usize,
    next: Option<&'static mut Hole>,
}

impl Hole {
    fn info(&self) -> HoleInfo {
        HoleInfo {
            addr: self as *const _ as usize,
            size: self.size,
        }
    }
}

struct HoleInfo {
    addr: usize,
    size: usize,
}

struct Allocation {
    info: HoleInfo,
    front_padding: Option<HoleInfo>,
    back_padding: Option<HoleInfo>,
}

fn split_hole(hole: HoleInfo, required_layout: Layout) -> Option<Allocation> {
    let required_size = required_layout.size();
    let required_align = required_layout.align();

    let (aligned_addr, front_padding) = if hole.addr == align_up(hole.addr, required_align) {
        // Hole already has the required alignment.
        (hole.addr, None)
    } else {
        // The required alignment needs some padding before the allocation.
        let aligned_addr = align_up(hole.addr + MIN_ALLOC_SIZE, required_align);
        (
            aligned_addr,
            Some(HoleInfo {
                addr: hole.addr,
                size: aligned_addr - hole.addr,
            }),
        )
    };

    let aligned_hole = {
        if aligned_addr + required_size > hole.addr + hole.size {
            // The hole is too small.
            return None;
        }
        HoleInfo {
            addr: aligned_addr,
            size: hole.size - (aligned_addr - hole.addr),
        }
    };

    let back_padding = if aligned_hole.size == required_size {
        // The aligned hole has exactly the size that's needed, no padding accrues.
        None
    } else if aligned_hole.size - required_size < MIN_ALLOC_SIZE {
        // We can't use this hole since its remains would form a new one that's too
        // small.
        return None;
    } else {
        // The hole is bigger than necessary, so there is some padding behind the
        // allocation.
        Some(HoleInfo {
            addr: aligned_hole.addr + required_size,
            size: aligned_hole.size - required_size,
        })
    };

    Some(Allocation {
        info: HoleInfo {
            addr: aligned_hole.addr,
            size: required_size,
        },
        front_padding: front_padding,
        back_padding: back_padding,
    })
}

fn allocate_first_fit(mut previous: &mut Hole, layout: Layout) -> Result<HoleInfo, ()> {
    loop {
        let allocation: Option<Allocation> = previous
            .next
            .as_mut()
            .and_then(|current| split_hole(current.info(), layout.clone()));
        match allocation {
            Some(allocation) => {
                // Link the front/back padding.
                // Note that there must be no hole between following pair:
                //      previous - front_padding
                //      front_padding - back_padding
                //      back_padding - previous.next
                previous.next = previous.next.as_mut().unwrap().next.take();
                if let Some(padding) = allocation.front_padding {
                    let ptr = padding.addr as *mut Hole;
                    unsafe {
                        ptr.write(Hole {
                            size: padding.size,
                            next: previous.next.take(),
                        })
                    }
                    previous.next = Some(unsafe { &mut *ptr });
                    previous = move_helper(previous).next.as_mut().unwrap();
                }
                if let Some(padding) = allocation.back_padding {
                    let ptr = padding.addr as *mut Hole;
                    unsafe {
                        ptr.write(Hole {
                            size: padding.size,
                            next: previous.next.take(),
                        })
                    }
                    previous.next = Some(unsafe { &mut *ptr });
                }
                return Ok(allocation.info);
            }
            None if previous.next.is_some() => {
                // Try the next hole.
                previous = move_helper(previous).next.as_mut().unwrap();
            }
            None => {
                // This was the last hole, so no hole is big enough -> allocation not possible.
                return Err(());
            }
        }
    }
}

fn deallocate(mut hole: &mut Hole, addr: usize, mut size: usize) {
    loop {
        assert!(size >= MIN_ALLOC_SIZE);

        let hole_addr = if hole.size == 0 {
            // It's the dummy hole, which is the head of the HoleList. It's somewhere on the
            // stack, so it's address is not the address of the hole. We set the
            // addr to 0 as it's always the first hole.
            0
        } else {
            // It's a real hole in memory and its address is the address of the hole
            hole as *mut _ as usize
        };

        // Each freed block must be handled by the previous hole in memory. Thus the
        // freed address must always be behind the current hole.
        assert!(
            hole_addr + hole.size <= addr,
            "invalid deallocation (probably a double free)",
        );

        // Get information about the next block.
        let next_hole_info = hole.next.as_ref().map(|next| next.info());
        match next_hole_info {
            Some(next) if hole_addr + hole.size == addr && addr + size == next.addr => {
                // Block fills the gap between this hole and the next hole.
                // before:  ___XXX____YYYYY____    where X is this hole and Y the next hole
                // after:   ___XXXFFFFYYYYY____    where F is the freed block

                hole.size += size + next.size; // merge the F and Y blocks to this X block
                hole.next = hole.next.as_mut().unwrap().next.take(); // remove the Y block
            }
            _ if hole_addr + hole.size == addr => {
                // Block is right behind this hole but there is used memory after it.
                // before:  ___XXX______YYYYY____    where X is this hole and Y the next hole
                // after:   ___XXXFFFF__YYYYY____    where F is the freed block

                // OR: Block is right behind this hole and this is the last hole.
                // before:  ___XXX_______________    where X is this hole and Y the next hole
                // after:   ___XXXFFFF___________    where F is the freed block

                hole.size += size; // Merge the F block to this X block.
            }
            Some(next) if addr + size == next.addr => {
                // Block is right before the next hole but there is used memory before it.
                // before:  ___XXX______YYYYY____    where X is this hole and Y the next hole
                // after:   ___XXX__FFFFYYYYY____    where F is the freed block

                hole.next = hole.next.as_mut().unwrap().next.take(); // Remove the Y block.
                size += next.size; // Free the merged F/Y block in next iteration.
                continue;
            }
            Some(next) if next.addr <= addr => {
                // Block is behind the next hole, so we delegate it to the next hole.
                // before:  ___XXX__YYYYY________    where X is this hole and Y the next hole
                // after:   ___XXX__YYYYY__FFFF__    where F is the freed block

                // Start next iteration at next hole.
                hole = move_helper(hole).next.as_mut().unwrap();
                continue;
            }
            _ => {
                // Block is between this and the next hole.
                // before:  ___XXX________YYYYY_    where X is this hole and Y the next hole
                // after:   ___XXX__FFFF__YYYYY_    where F is the freed block

                // OR: This is the last hole.
                // before:  ___XXX_________    where X is this hole
                // after:   ___XXX__FFFF___    where F is the freed block

                let new_hole = Hole {
                    size: size,
                    next: hole.next.take(), // The reference to the Y block (if it exists).
                };
                // Write the new hole to the freed memory.
                debug_assert_eq!(addr % align_of::<Hole>(), 0);
                let ptr = addr as *mut Hole;
                unsafe { ptr.write(new_hole) };
                // Add the F block as the next block of the X block.
                hole.next = Some(unsafe { &mut *ptr });
            }
        }

        break;
    }
}

fn move_helper<T>(x: T) -> T {
    x
}
