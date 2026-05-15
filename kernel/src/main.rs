#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![allow(static_mut_refs)]

#[macro_use]
extern crate alloc;

mod acpi;
mod apic;
mod ata;
mod gdt;
mod hpet;
mod idt;
mod input;
mod loader;
mod memory;
mod scheduler;
mod serial;
mod tsc;
mod vfat;

use {
    alloc::{string::String, vec::Vec},
    boot_info::BootInfo,
    core::arch::asm,
    log::info,
    memory_types::PAGE_SIZE,
};


// Linker offset symbols (see `../kernel_x86_64.ld`).
unsafe extern "C" {
    static __kernel_start: u8;
    static __kernel_end: u8;
    static __text_start: u8;
    static __text_end: u8;
    static __rodata_start: u8;
    static __rodata_end: u8;
}

/// The kernel's entry point.
#[unsafe(no_mangle)]
pub extern "sysv64" fn _start(boot_info: &BootInfo) -> ! {
    unsafe {
        asm!(
            "mov rdi, {}",
            "mov rsp, {}",
            "call {}",
            in(reg) boot_info,
            in(reg) KERNEL_STACK.as_ptr() as u64 + KERNEL_STACK.len() as u64,
            in(reg) main, // See `main` function below.
            options(nomem, nostack),
        );
    }

    unreachable!();
}

pub extern "sysv64" fn main(boot_info: &'static BootInfo) -> ! {
    let startup_time = rtc::Time::now();

    serial::init();

    info!(
        "KERNEL STARTUP @ {startup_time}\n\
        \trange: {:#x}..{:#x}\n\
        \ttext: {:#x}..{:#x}\n\
        \trodata: {:#x}..{:#x}",
        (&raw const __kernel_start) as usize,
        (&raw const __kernel_end) as usize,
        (&raw const __text_start) as usize,
        (&raw const __text_end) as usize,
        (&raw const __rodata_start) as usize,
        (&raw const __rodata_end) as usize,
    );

    gdt::init();
    idt::init();
    tsc::init();
    memory::init(boot_info);
    acpi::init(boot_info);
    ata::init(boot_info);

    for object in boot_info.root_object_map.iter() {
        log::debug!("OBJECT: {object:x?}");
    }

    info!("STARTUP SUCCESSFUL");

    // Run the example program.
    scheduler::with_scheduler(|scheduler| {
        scheduler.run_user_process("example", None, true, scheduler::AccessPolicy::All);
    });

    // Run the core kernel processes.
    scheduler::with_scheduler(|scheduler| {
        scheduler.run_kernel_process("input_dispatcher", input::dispatch_input_events as _, None);
    });

    memory::TRACKER.lock().dump_info();

    scheduler::run()
}

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    log::error!("{info}");
    loop {}
}



const KERNEL_STACK_SIZE: usize = 16 * PAGE_SIZE;
static KERNEL_STACK: KernelStack = KernelStack::new();

#[repr(align(16))] // System V ABI requires 16 byte stack alignment.
struct KernelStack([u8; KERNEL_STACK_SIZE]);

impl KernelStack {
    const fn new() -> Self {
        Self([0; KERNEL_STACK_SIZE])
    }

    const fn len(&self) -> usize {
        self.0.len()
    }

    const fn as_ptr(&self) -> *const u8 {
        self.0.as_ptr()
    }
}



pub trait FileSystem {
    fn list(&self, dir_path: &str) -> Result<Vec<String>, &'static str>;
    fn read(&mut self, path: &str) -> Result<Vec<u8>, &'static str>;
}

pub struct DirectoryEntry {
    pub index: usize,
    pub name: String,
    pub size: usize,
}
