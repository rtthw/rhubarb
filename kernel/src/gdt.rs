//! # Global Descriptor Table (GDT)

use {
    core::ptr::addr_of,
    log::info,
    memory_types::{PAGE_SIZE, POINTER_SIZE},
    x86_64::{
        VirtAddr,
        instructions::tables::load_tss,
        registers::segmentation::{CS, DS, ES, FS, GS, SS, Segment},
        structures::{
            gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
            tss::TaskStateSegment,
        },
    },
};



static mut GDT: GlobalDescriptorTable = GlobalDescriptorTable::new();
static mut TSS: Tss = Tss::new();
static mut SELECTORS: Selectors = Selectors::NULL;

const INTERRUPT_STACK_SIZE: usize = PAGE_SIZE * 4;
const IOMAP_WIDTH: usize = ((u16::MAX as usize + 1) / POINTER_SIZE) + 1;

pub const DOUBLE_FAULT_IST: u16 = 0;
pub const PAGE_FAULT_IST: u16 = 1;
pub const GENERAL_PROTECTION_FAULT_IST: u16 = 2;
pub const LOCAL_APIC_TIMER_IST: u16 = 3;
pub const USER_IST: u16 = 4;

pub fn init() {
    info!("Initializing GDT...");

    unsafe {
        TSS.inner.interrupt_stack_table[DOUBLE_FAULT_IST as usize] = {
            static mut STACK: [u8; INTERRUPT_STACK_SIZE] = [0; INTERRUPT_STACK_SIZE];
            VirtAddr::from_ptr(addr_of!(STACK)) + INTERRUPT_STACK_SIZE as u64
        };
        TSS.inner.interrupt_stack_table[PAGE_FAULT_IST as usize] = {
            static mut STACK: [u8; INTERRUPT_STACK_SIZE] = [0; INTERRUPT_STACK_SIZE];
            VirtAddr::from_ptr(addr_of!(STACK)) + INTERRUPT_STACK_SIZE as u64
        };
        TSS.inner.interrupt_stack_table[GENERAL_PROTECTION_FAULT_IST as usize] = {
            static mut STACK: [u8; INTERRUPT_STACK_SIZE] = [0; INTERRUPT_STACK_SIZE];
            VirtAddr::from_ptr(addr_of!(STACK)) + INTERRUPT_STACK_SIZE as u64
        };
        TSS.inner.interrupt_stack_table[LOCAL_APIC_TIMER_IST as usize] = {
            static mut STACK: [u8; INTERRUPT_STACK_SIZE] = [0; INTERRUPT_STACK_SIZE];
            VirtAddr::from_ptr(addr_of!(STACK)) + INTERRUPT_STACK_SIZE as u64
        };
        TSS.inner.interrupt_stack_table[USER_IST as usize] = {
            static mut STACK: [u8; INTERRUPT_STACK_SIZE] = [0; INTERRUPT_STACK_SIZE];
            VirtAddr::from_ptr(addr_of!(STACK)) + INTERRUPT_STACK_SIZE as u64
        };

        let kernel_tss =
            GDT.append(Descriptor::tss_segment_with_iomap(&TSS.inner, &TSS.iomap).unwrap());
        let kernel_code = GDT.append(Descriptor::kernel_code_segment());
        let kernel_data = GDT.append(Descriptor::kernel_data_segment());
        let user_code = GDT.append(Descriptor::user_code_segment());
        let user_data = GDT.append(Descriptor::user_data_segment());

        SELECTORS = Selectors {
            kernel_tss,
            kernel_code,
            kernel_data,
            user_code,
            user_data,
        };

        // debug!("SELECTORS:\n{:#?}", SELECTORS);

        GDT.load();

        // Without this, you get a general protection fault during the end-of-interrupt
        // signal of the local APIC timer.
        SS::set_reg(SELECTORS.kernel_data);

        CS::set_reg(SELECTORS.kernel_code);
        DS::set_reg(SELECTORS.kernel_data);
        ES::set_reg(SELECTORS.kernel_data);
        FS::set_reg(SELECTORS.kernel_data);
        GS::set_reg(SELECTORS.kernel_data);

        load_tss(SELECTORS.kernel_tss);
    }
}

pub fn selectors() -> &'static Selectors {
    unsafe { &SELECTORS }
}

#[derive(Debug)]
pub struct Selectors {
    kernel_tss: SegmentSelector,
    pub kernel_code: SegmentSelector,
    pub kernel_data: SegmentSelector,
    pub user_code: SegmentSelector,
    pub user_data: SegmentSelector,
}

impl Selectors {
    const NULL: Self = Self {
        kernel_tss: SegmentSelector::NULL,
        kernel_code: SegmentSelector::NULL,
        kernel_data: SegmentSelector::NULL,
        user_code: SegmentSelector::NULL,
        user_data: SegmentSelector::NULL,
    };
}

/// Set whether ring 3 processes are allowed to perform I/O.
///
/// This has no effect on ring 0 processes.
pub fn set_user_io_allowed(allow_user_io: bool) {
    let offset = if allow_user_io {
        size_of::<TaskStateSegment>() as u16
    } else {
        0xFFFF
    };

    unsafe {
        TSS.inner.iomap_base = offset;
    }
}

#[repr(C)]
struct Tss {
    inner: TaskStateSegment,
    iomap: [u8; IOMAP_WIDTH],
}

impl Tss {
    const fn new() -> Self {
        let mut iomap = [0; IOMAP_WIDTH];
        iomap[IOMAP_WIDTH - 1] = 0xFF;

        Self {
            inner: TaskStateSegment::new(),
            iomap,
        }
    }
}
