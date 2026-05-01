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
mod window_manager;

use {
    alloc::{string::String, vec::Vec},
    boot_info::BootInfo,
    core::{arch::asm, time::Duration},
    log::{debug, info},
    memory_types::{PAGE_SIZE, PageRange, VirtualAddress},
};


unsafe extern "C" {
    static __text_start: u8;
    static __text_end: u8;
    static __rodata_start: u8;
    static __rodata_end: u8;
    static __kernel_end: u8;
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
        \ttext: {:#x}..{:#x}\n\
        \trodata: {:#x}..{:#x}\n\
        \tend: {:#x}",
        (&raw const __text_start) as usize,
        (&raw const __text_end) as usize,
        (&raw const __rodata_start) as usize,
        (&raw const __rodata_end) as usize,
        (&raw const __kernel_end) as usize,
    );

    gdt::init();
    idt::init();

    memory::init(boot_info);

    acpi::init(boot_info);
    tsc::init();

    let pm_start = time::now();
    if let Ok(()) = acpi::pm_timer_sleep(1_000) {
        let dur = pm_start.elapsed();
        debug!("`acpi::pm_timer_sleep(1ms)`\t: {dur:?}");
    }
    let pit_start = time::now();
    pit::sleep(1_000);
    let dur = pit_start.elapsed();
    debug!("`pit::sleep(1ms)`\t\t: {dur:?}");

    // Register BAR memory with the global memory tracker.
    for pci_device in pci::enumerate_devices() {
        for slot in 0..6 {
            if let Some(bar) = pci_device.bar(slot) {
                let pages = match bar {
                    pci::Bar::Mem32 {
                        address,
                        size,
                        prefetchable: _,
                    } => PageRange::from_base_size(
                        VirtualAddress::new(address as usize),
                        size as usize,
                    ),
                    pci::Bar::Mem64 {
                        address,
                        size,
                        prefetchable: _,
                    } => PageRange::from_base_size(
                        VirtualAddress::new(address as usize),
                        size as usize,
                    ),
                    pci::Bar::Io { address: _ } => {
                        // TODO: Register I/O port addresses?
                        continue;
                    }
                };

                memory::TRACKER.lock().register_pci_bar(
                    format!(
                        "pci_bar.{}:{}:{}.{slot}",
                        pci_device.bus, pci_device.device, pci_device.function,
                    ),
                    pages,
                );
            }
        }
    }

    ata::init(boot_info);

    unsafe {
        BOOT_INFO = Some(boot_info);
    }

    info!("STARTUP SUCCESSFUL");

    // Run the example program.
    scheduler::with_scheduler(|scheduler| {
        scheduler.run_user_process("example", None, true, scheduler::AccessPolicy::All);
    });

    // Run the core kernel processes.
    scheduler::with_scheduler(|scheduler| {
        scheduler.run_kernel_process(
            "clock_update_dispatcher",
            dispatch_clock_updates as *const fn() -> !,
            None,
        )
    });
    scheduler::with_scheduler(|scheduler| {
        scheduler.run_kernel_process(
            "input_event_dispatcher",
            input::dispatch_input_events as *const fn() -> !,
            Some(PAGE_SIZE * 32),
        )
    });

    window_manager::init();

    memory::TRACKER.lock().dump_info();

    scheduler::run()
}

static mut BOOT_INFO: Option<&'static BootInfo> = None;

fn dispatch_clock_updates() -> ! {
    loop {
        let start = time::now();
        window_manager::send_event(window_manager::Event::ClockUpdate);
        while time::now().duration_since(start) < Duration::from_secs(1) {
            scheduler::defer();
        }
    }
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
