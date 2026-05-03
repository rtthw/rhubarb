//! # Memory Management

use {
    alloc::{string::String, vec::Vec},
    boot_info::{BootInfo, MemoryMap, MemoryRegionKind},
    core::{
        fmt,
        sync::atomic::{AtomicUsize, Ordering},
    },
    hashbrown::HashMap,
    linked_list_allocator::LockedHeap,
    log::{debug, error, info, trace, warn},
    memory_types::{
        Frame, FrameAllocator as DynFrameAllocator, GIBIBYTE, Level4PageTable, MEBIBYTE, PAGE_SIZE,
        Page, PageRange, PageTable, PageTableFlags, PhysicalAddress, VirtualAddress,
        paging::MappingError,
    },
    spin_mutex::Mutex,
    x86_64::{
        instructions::interrupts::without_interrupts,
        registers::control::{Cr0, Cr0Flags, Cr3, Cr3Flags},
    },
};


const HEAP_BASE: usize = (509 << (12 + (9 * 3))) | 0xFFFF_0000_0000_0000;
const KERNEL_MAPPING_BASE: usize = (510 << (12 + (9 * 3))) | 0xFFFF_0000_0000_0000;

#[global_allocator]
static mut ALLOCATOR: LockedHeap = LockedHeap::empty();
static FRAME_ALLOCATOR: Mutex<FrameAllocator> = Mutex::new(FrameAllocator::new());

pub static mut FRAMEBUFFER_MAPPING: Option<KernelMapping> = None;

pub fn init(boot_info: &BootInfo) {
    info!("Initializing memory management...");

    TRACKER.lock().init(boot_info);

    init_kernel_address_space();
    let addr_space = kernel_address_space();

    assert!(addr_space.is_current());

    // Make sure write protection is off so we don't page fault when we try to write
    // to the read-only UEFI page tables.
    unsafe {
        let cr0 = Cr0::read();
        debug!("CR0: {cr0:?}");
        if cr0.contains(Cr0Flags::WRITE_PROTECT) {
            Cr0::write(cr0 & !Cr0Flags::WRITE_PROTECT);
            info!("Cleared CR0.WP");
        }
    }

    let free_frames = {
        let mut frame_allocator = FRAME_ALLOCATOR.lock();
        frame_allocator.init(&boot_info.memory_map);
        frame_allocator.reserve_range(PhysicalAddress::new(0), MEBIBYTE);
        frame_allocator.reserve_range(
            PhysicalAddress::new(boot_info.kernel_start),
            boot_info.kernel_end - boot_info.kernel_start,
        );
        frame_allocator.reserve_range(
            PhysicalAddress::new(boot_info.display_info.framebuffer_addr as usize),
            boot_info.display_info.framebuffer_addr as usize
                + boot_info.display_info.framebuffer_size,
        );

        let free_frames = frame_allocator.free_frames;
        let total_frames = frame_allocator.total_frames;
        let free_memory = (free_frames * PAGE_SIZE) / MEBIBYTE;
        let total_memory = (total_frames * PAGE_SIZE) / MEBIBYTE;
        info!(
            "Physical memory allocator initialized\n\
            \tFree frames: {free_frames} / {total_frames}\n\
            \tFree memory: {free_memory} MiB / {total_memory} MiB",
        );

        match frame_allocator.allocate() {
            Ok(frame) => {
                debug!("    Allocated frame: {frame}");
                if let Err(error) = frame_allocator.deallocate(frame) {
                    error!("    Failed to deallocate frame: {error:?}");
                } else {
                    debug!("    Deallocated frame successfully");
                }
            }
            Err(error) => {
                error!("    Failed to allocate frame: {error:?}");
            }
        }

        free_frames
    };

    let physical_mem_size = free_frames * PAGE_SIZE;
    assert!(
        physical_mem_size > 64 * MEBIBYTE,
        "Not enough physical memory",
    );

    let heap_size = 32 * MEBIBYTE;
    let heap_start = VirtualAddress::new(HEAP_BASE);
    let heap_pages = PageRange::from_base_size(heap_start, heap_size);

    // Note that we can't use `addr_space.map_pages` here because it requires a heap
    // to already be initialized (internally calls `Vec::push`).
    {
        let mut frame_allocator = FRAME_ALLOCATOR.lock();
        let mut page_table = addr_space.page_table.lock();

        for page in heap_pages {
            let frame = frame_allocator.allocate_frame().unwrap();
            page_table
                .map_to(
                    page,
                    frame,
                    PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                    &mut *frame_allocator,
                )
                .unwrap()
                .flush();
        }
    }

    debug!(
        "Initializing heap at {:#x} ({} pages, {} MiB)...",
        addr_space
            .translate_address(heap_start)
            .expect("should be able to translate HEAP_BASE after mapping it"),
        heap_size / PAGE_SIZE,
        heap_size / MEBIBYTE,
    );

    unsafe {
        ALLOCATOR.lock().init(HEAP_BASE, heap_size);
    }

    // Make sure the heap allocator actually works.
    initial_heap_test();

    info!("Heap initialized successfully");

    let framebuffer_addr = VirtualAddress::new(boot_info.display_info.framebuffer_addr as usize);
    let framebuffer_size = boot_info.display_info.framebuffer_size;
    let framebuffer_pages = PageRange::from_base_size(framebuffer_addr, framebuffer_size);

    unsafe {
        // The framebuffer should already be mapped into the kernel address space, so we
        // just manually create it here.
        FRAMEBUFFER_MAPPING = Some(KernelMapping {
            name: "framebuffer".into(),
            size: framebuffer_size,
            pages: framebuffer_pages,
            flags: PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
        });
    }
}

fn initial_heap_test() {
    {
        let object_1: Vec<u8> = vec![1, 2, 3];
        let object_1_addr = object_1.as_ptr().addr();

        assert!(object_1_addr == HEAP_BASE);
    }

    let object_2: Vec<u8> = vec![4, 5, 6];
    let object_2_addr = object_2.as_ptr().addr();

    // If object 1 failed to deallocate, then this would fail.
    assert!(object_2_addr == HEAP_BASE);

    let object_3: Vec<u8> = vec![7, 8, 9];
    let object_3_addr = object_3.as_ptr().addr();

    // The heap should start at `HEAP_START` and grow upwards, so this object should
    // have a higher virtual address.
    assert!(object_3_addr > HEAP_BASE);
}



static KERNEL_MAPPING_OFFSET: AtomicUsize = AtomicUsize::new(KERNEL_MAPPING_BASE);

/// A set of mapped pages within the kernel's [`AddressSpace`].
#[derive(Debug)]
pub struct KernelMapping {
    pub name: String,
    pub size: usize,
    pub pages: PageRange,
    pub flags: PageTableFlags,
}

impl KernelMapping {
    pub fn new(name: impl Into<String>, size_in_bytes: usize, flags: PageTableFlags) -> Self {
        let name = name.into();
        let addr = VirtualAddress::new(KERNEL_MAPPING_OFFSET.fetch_add(
            size_in_bytes.div_ceil(PAGE_SIZE) * PAGE_SIZE,
            Ordering::SeqCst,
        ));
        let pages = PageRange::from_base_len(addr, size_in_bytes.div_ceil(PAGE_SIZE));

        assert_eq!(pages.len(), size_in_bytes.div_ceil(PAGE_SIZE));

        kernel_address_space().map_pages(&name, pages, flags);

        Self {
            name,
            size: size_in_bytes,
            pages,
            flags,
        }
    }

    #[inline]
    pub const fn addr(&self) -> VirtualAddress {
        self.pages.start.base_addr()
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn as_slice_mut(&mut self, offset: usize, len: usize) -> &mut [u8] {
        assert!(kernel_address_space().is_current());
        assert!(
            offset + len <= self.size(),
            "Requested offset and length would overflow kernel mapping",
        );

        let addr = self.addr() + offset;

        unsafe { core::slice::from_raw_parts_mut(addr.to_raw() as *mut _, len) }
    }

    /// Get a mutable reference to a value of type `T` at the given offset
    /// within this mapping.
    pub unsafe fn as_mut<T: Sized>(&mut self, offset: usize) -> &mut T {
        // assert!(kernel_address_space().is_current());
        assert!(
            size_of::<T>() + offset <= self.size(),
            "Requested type and offset would not fit in kernel mapping",
        );

        unsafe {
            ((self.addr() + offset).to_raw() as *mut T)
                .as_mut()
                .unwrap()
        }
    }

    /// Make this mapping available within the given [`AddressSpace`].
    ///
    /// ## Arguments
    ///
    /// - `address_space`, the [`AddressSpace`] that will receive the newly
    ///   mapped pages.
    /// - `pages`, the pages to map within `address_space`.
    /// - `flags`, the [`PageTableFlags`] to apply to the new mapping.
    pub fn map_into(
        &self,
        address_space: &AddressSpace,
        pages: PageRange,
        flags: PageTableFlags,
    ) -> Result<(), MappingError> {
        address_space.map_kernel_pages_to(self.pages, pages, flags)?;
        let mut mm = TRACKER.lock();
        mm.spaces
            .entry((address_space.name.clone(), address_space.frame))
            .or_insert(Vec::new())
            .push((self.name.clone(), pages));

        Ok(())
    }
}



const MAX_PHYSICAL_MEMORY: usize = 1 * GIBIBYTE;
const MAX_FRAMES: usize = MAX_PHYSICAL_MEMORY / PAGE_SIZE;
const BITMAP_LEN: usize = MAX_FRAMES / 64;

pub struct FrameAllocator {
    bitmap: [u64; BITMAP_LEN],
    total_frames: usize,
    free_frames: usize,
    next_free_hint: usize,
}

impl FrameAllocator {
    pub const fn new() -> Self {
        Self {
            bitmap: [0; BITMAP_LEN],
            total_frames: 0,
            free_frames: 0,
            next_free_hint: 0,
        }
    }

    pub fn init(&mut self, memory_map: &MemoryMap) {
        for word in self.bitmap.iter_mut() {
            *word = !0;
        }

        self.total_frames = 0;
        self.free_frames = 0;

        for region in memory_map.iter() {
            if region.kind == MemoryRegionKind::Free {
                let start_frame = Frame::containing_addr(PhysicalAddress::new(region.base));
                let end_addr = region.base + region.size;
                let end_frame = Frame::containing_addr(PhysicalAddress::new(end_addr));

                let first_frame = if PhysicalAddress::new(region.base).is_page_aligned() {
                    start_frame.number()
                } else {
                    start_frame.number() + 1
                };
                let last_frame = end_frame.number();

                for frame_num in first_frame..last_frame {
                    if frame_num < MAX_FRAMES {
                        self.mark_free(frame_num);
                        self.total_frames += 1;
                        self.free_frames += 1;
                    }
                }
            }
        }

        self.next_free_hint = 0;
    }

    pub fn reserve_range(&mut self, base: PhysicalAddress, size: usize) {
        let start_frame = base.frame().number();
        let frame_count = size.div_ceil(PAGE_SIZE);

        for i in 0..frame_count {
            let frame_num = start_frame + i;
            if frame_num < MAX_FRAMES && !self.is_allocated(frame_num) {
                self.mark_used(frame_num);
                if self.free_frames > 0 {
                    self.free_frames -= 1;
                }
            }
        }
    }

    #[inline]
    const fn is_allocated(&self, frame_num: usize) -> bool {
        let word_idx = frame_num / 64;
        let bit_idx = frame_num % 64;

        (self.bitmap[word_idx] & (1 << bit_idx)) != 0
    }

    #[inline]
    const fn mark_used(&mut self, frame_num: usize) {
        let word_index = frame_num / 64;
        let bit_index = frame_num % 64;
        self.bitmap[word_index] |= 1 << bit_index;
    }

    #[inline]
    const fn mark_free(&mut self, frame_num: usize) {
        let word_index = frame_num / 64;
        let bit_index = frame_num % 64;
        self.bitmap[word_index] &= !(1 << bit_index);
    }

    pub fn allocate(&mut self) -> Result<Frame, FrameAllocatorError> {
        if self.free_frames == 0 {
            return Err(FrameAllocatorError::OutOfMemory);
        }

        let start_word = self.next_free_hint / 64;
        for word_index in start_word..BITMAP_LEN {
            if self.bitmap[word_index] != !0 {
                let free_bit = (!self.bitmap[word_index]).trailing_zeros() as usize;
                let frame_num = word_index * 64 + free_bit;

                self.mark_used(frame_num);
                self.free_frames -= 1;
                self.next_free_hint = frame_num + 1;

                return Ok(Frame::new(frame_num));
            }
        }
        for word_index in 0..start_word {
            if self.bitmap[word_index] != !0 {
                let free_bit = (!self.bitmap[word_index]).trailing_zeros() as usize;
                let frame_num = word_index * 64 + free_bit;

                self.mark_used(frame_num);
                self.free_frames -= 1;
                self.next_free_hint = frame_num + 1;

                return Ok(Frame::new(frame_num));
            }
        }

        Err(FrameAllocatorError::OutOfMemory)
    }

    pub fn deallocate(&mut self, frame: Frame) -> Result<(), FrameAllocatorError> {
        let frame_num = frame.number();

        if frame_num >= MAX_FRAMES {
            return Err(FrameAllocatorError::InvalidFrame);
        }

        if !self.is_allocated(frame_num) {
            warn!("Detected double-free at frame #{frame_num}");
            return Ok(());
        }

        self.mark_free(frame_num);
        self.free_frames += 1;

        if frame_num < self.next_free_hint {
            self.next_free_hint = frame_num;
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameAllocatorError {
    OutOfMemory,
    InvalidFrame,
}

impl DynFrameAllocator for FrameAllocator {
    fn allocate_frame(&mut self) -> Option<Frame> {
        self.allocate().ok()
    }
}



fn init_kernel_address_space() {
    unsafe {
        let (l4_frame, _) = Cr3::read();
        let l4_ptr = l4_frame.start_address().as_u64() as *mut PageTable;

        let page_table = Level4PageTable::new(&mut *l4_ptr);

        KERNEL_ADDRESS_SPACE = Some(AddressSpace {
            name: String::new(),
            frame: Frame::from_base_addr(PhysicalAddress::new(
                l4_frame.start_address().as_u64() as usize
            ))
            .unwrap(),
            frame_allocator: Mutex::new(FrameAllocatorProxy {
                allocated_frames: Vec::new(),
            }),
            page_table: Mutex::new(page_table),
        });
    }
}

pub fn kernel_address_space<'a>() -> &'a AddressSpace {
    unsafe {
        KERNEL_ADDRESS_SPACE
            .as_ref()
            .expect("kernel address space should be initialized")
    }
}

static mut KERNEL_ADDRESS_SPACE: Option<AddressSpace> = None;

pub struct AddressSpace {
    name: String,
    frame: Frame,
    frame_allocator: Mutex<FrameAllocatorProxy>,
    page_table: Mutex<Level4PageTable>,
}

impl fmt::Debug for AddressSpace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AddressSpace @ {:?}", self.frame)
    }
}

impl AddressSpace {
    pub fn new(name: impl Into<String>, inherit: Option<&AddressSpace>) -> Self {
        assert!(kernel_address_space().is_current());

        let name = name.into();
        let mut frame_allocator = FrameAllocatorProxy {
            allocated_frames: Vec::new(),
        };
        let frame = frame_allocator
            .allocate_frame()
            .expect("failed to allocate frame for new address space");

        trace!("New address space at {frame:?} for {name:?}");

        let mut page_table = unsafe {
            let l4_ptr = VirtualAddress::new(frame.base_addr().to_raw()).to_raw() as *mut PageTable;

            Level4PageTable::new(&mut *l4_ptr)
        };
        page_table.clear();

        if let Some(parent) = inherit {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    parent.frame.base_addr().to_raw() as *const u8,
                    frame.base_addr().to_raw() as *mut _,
                    PAGE_SIZE,
                );
            }
        } else {
            let kernel_table = kernel_address_space().page_table.lock();

            // Make sure the kernel is accessible from this address space so when we enter
            // it we can still access memory while in ring 0.
            // TODO: The kernel should be mapped in the higher half.
            page_table[0] = kernel_table[0].clone();
            page_table[509] = kernel_table[509].clone();
        }

        Self {
            name,
            frame,
            frame_allocator: Mutex::new(frame_allocator),
            page_table: Mutex::new(page_table),
        }
    }

    /// Get the name of this address space.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether this address space is currently active.
    pub fn is_current(&self) -> bool {
        Cr3::read_raw().0.start_address().as_u64() as usize == self.frame.base_addr().to_raw()
    }

    /// Make this address space the active one.
    pub fn enter(&self) {
        unsafe {
            Cr3::write(
                x86_64::structures::paging::PhysFrame::from_start_address(x86_64::PhysAddr::new(
                    self.frame.base_addr().to_raw() as u64,
                ))
                .unwrap(),
                Cr3Flags::empty(),
            );
        }
    }

    pub fn map_pages(&self, name: impl Into<String>, pages: PageRange, flags: PageTableFlags) {
        let mut frame_allocator = self.frame_allocator.lock();
        let mut page_table = self.page_table.lock();

        for page in pages {
            let frame = frame_allocator.allocate_frame().unwrap();
            // trace!("MAPPING {page:?} TO {frame:?}");
            page_table
                .map_to(page, frame, flags, &mut *frame_allocator)
                .unwrap()
                .flush();
        }

        TRACKER
            .lock()
            .spaces
            .entry((self.name.clone(), self.frame))
            .or_insert(Vec::new())
            .push((name.into(), pages));
    }

    pub fn set_flags(
        &mut self,
        pages: PageRange,
        flags: PageTableFlags,
    ) -> Result<(), MappingError> {
        let mut frame_allocator = self.frame_allocator.lock();
        let mut page_table = self.page_table.lock();

        page_table.set_flags(pages, flags, &mut *frame_allocator)?;

        Ok(())
    }

    fn map_kernel_pages_to(
        &self,
        kernel_pages: PageRange,
        local_pages: PageRange,
        flags: PageTableFlags,
    ) -> Result<(), MappingError> {
        assert_eq!(kernel_pages.count(), local_pages.count());

        let mut frame_allocator = self.frame_allocator.lock();
        let mut page_table = self.page_table.lock();

        for (local_page, kernel_page) in local_pages.zip(kernel_pages) {
            let frame = kernel_address_space()
                .translate_page(kernel_page)
                .expect("should be a mapped kernel page");

            page_table
                .map_to(local_page, frame, flags, &mut *frame_allocator)?
                .flush();
        }

        Ok(())
    }

    pub fn translate_address(&self, addr: VirtualAddress) -> Option<PhysicalAddress> {
        self.page_table
            .lock()
            .translate(addr)
            .ok()
            .map(|res| res.frame.base_addr() + res.offset)
    }

    pub fn translate_page(&self, page: Page) -> Option<Frame> {
        self.page_table.lock().translate_page(page).ok()
    }
}

impl Drop for AddressSpace {
    fn drop(&mut self) {
        TRACKER
            .lock()
            .spaces
            .remove(&(self.name.clone(), self.frame));
    }
}

/// A proxy to the global [`FrameAllocator`]. Keeps track of allocated frames
/// and deallocates them on drop.
#[derive(Debug)]
pub struct FrameAllocatorProxy {
    allocated_frames: Vec<Frame>,
}

impl DynFrameAllocator for FrameAllocatorProxy {
    fn allocate_frame(&mut self) -> Option<Frame> {
        let frame = without_interrupts(|| FRAME_ALLOCATOR.lock().allocate_frame());

        if let Some(frame) = frame {
            self.allocated_frames.push(frame);
        }

        frame
    }
}

impl Drop for FrameAllocatorProxy {
    fn drop(&mut self) {
        without_interrupts(|| {
            trace!(
                "Dropping frame allocator proxy with {} allocated frames",
                self.allocated_frames.len(),
            );

            let mut global = FRAME_ALLOCATOR.lock();
            for frame in self.allocated_frames.drain(..) {
                let _ = global.deallocate(frame); // Ignore errors.
            }
        });
    }
}



pub static TRACKER: Mutex<MemoryTracker> = Mutex::new(MemoryTracker::new());

pub struct MemoryTracker {
    spaces: HashMap<(String, Frame), Vec<(String, PageRange)>, rustc_hash::FxBuildHasher>,
    kernel_pages: PageRange,
    framebuffer_pages: PageRange,
    pci_bar_pages: Vec<(String, PageRange)>,
}

impl MemoryTracker {
    const fn new() -> Self {
        Self {
            spaces: HashMap::with_hasher(rustc_hash::FxBuildHasher),
            kernel_pages: PageRange::new(Page::new(0), Page::new(0)),
            framebuffer_pages: PageRange::new(Page::new(0), Page::new(0)),
            pci_bar_pages: Vec::new(),
        }
    }

    pub fn init(&mut self, boot_info: &BootInfo) {
        self.kernel_pages = PageRange::from_base_size(
            VirtualAddress::new(boot_info.kernel_start),
            boot_info.kernel_end - boot_info.kernel_start,
        );
        self.framebuffer_pages = PageRange::from_base_size(
            VirtualAddress::new(boot_info.display_info.framebuffer_addr as usize),
            boot_info.display_info.framebuffer_size,
        );
    }

    pub fn register_pci_bar(&mut self, name: String, pages: PageRange) {
        self.pci_bar_pages.push((name, pages));
        self.pci_bar_pages
            .sort_by_key(|(_name, pages)| pages.start.base_addr());
    }

    pub fn dump_info(&self) {
        debug!(
            "\n--- MEMORY AREAS ---\n    \
            {:0>16x} {:0>16x}    kernel\n    \
            {:0>16x} {:0>16x}    framebuffer\n\
            {}\
            \n--- ADDRESS SPACES ---\
            {}",
            self.kernel_pages.start.base_addr(),
            self.kernel_pages.end.base_addr(),
            self.framebuffer_pages.start.base_addr(),
            self.framebuffer_pages.end.base_addr(),
            self.pci_bar_pages
                .iter()
                .map(|(bar_name, bar_pages)| {
                    format!(
                        "    {:0>16x} {:0>16x}    {bar_name}\n",
                        bar_pages.start.base_addr(),
                        bar_pages.end.base_addr()
                    )
                })
                .collect::<String>(),
            self.spaces
                .iter()
                .map(|((name, frame), mappings)| {
                    let mut mappings = mappings
                        .iter()
                        .map(|(name, mapping)| {
                            (
                                mapping.start.base_addr(),
                                format!(
                                    "\n        {:0>16x} {:0>16x}        {name}",
                                    mapping.start.base_addr(),
                                    mapping.end.base_addr(),
                                ),
                            )
                        })
                        .collect::<Vec<_>>();
                    mappings.sort_by_key(|(key, _string)| *key);

                    format!(
                        "\n    {:0>16x}{:25}{name}{}",
                        frame.base_addr(),
                        " ",
                        mappings
                            .into_iter()
                            .map(|(_key, string)| string)
                            .collect::<String>(),
                    )
                })
                .collect::<String>(),
        );
    }
}
