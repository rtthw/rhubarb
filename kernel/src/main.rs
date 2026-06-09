#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![allow(static_mut_refs)]

#[macro_use]
extern crate alloc;

mod acpi;
mod apic;
// mod ata;
mod gdt;
mod idt;
mod loader;
mod memory;
mod scheduler;
mod serial;
mod tsc;
// mod vfat;

use {
    alloc::{string::String, vec::Vec},
    boot_info::{BootInfo, RootObjectMap},
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

    log::record!(
        "STARTUP",
        time = startup_time,
        range "{:x?}"
            = ((&raw const __kernel_start) as usize)..((&raw const __kernel_end) as usize),
        text "{:x?}"
            = ((&raw const __text_start) as usize)..((&raw const __text_end) as usize),
        rodata "{:x?}"
            = ((&raw const __rodata_start) as usize)..((&raw const __rodata_end) as usize),
        stack "{:x?}"
            = (KERNEL_STACK.as_ptr() as usize)..(KERNEL_STACK.as_ptr() as usize + KERNEL_STACK_SIZE),
    );

    gdt::init();
    idt::init();
    tsc::init();
    memory::init(boot_info);
    acpi::init(boot_info);
    // ata::init(boot_info);

    loader::init(
        boot_info,
        InitFileSystem {
            root_object_map: &boot_info.root_object_map,
        },
    );

    info!("STARTUP SUCCESSFUL");

    // Run the example programs.
    scheduler::with_scheduler(|scheduler| {
        scheduler.run_user_process("example", None, true, scheduler::AccessPolicy::All);
    });
    scheduler::with_scheduler(|scheduler| {
        scheduler.run_user_process("input_driver", None, true, scheduler::AccessPolicy::All);
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



/// The initial in-memory file system (commonly called `initramfs`) provided to
/// the kernel by the bootloader. It is a set of module objects loaded into
/// memory.
pub struct InitFileSystem {
    root_object_map: &'static RootObjectMap,
}

impl fs::FileSystem for InitFileSystem {
    fn list(&self, dir_path: &str) -> Result<Vec<String>, &'static str> {
        Ok(self
            .root_object_map
            .iter()
            .filter(|obj| {
                obj.name_str()
                    .starts_with(dir_path.strip_prefix("/").unwrap())
            })
            .map(|obj| format!("/{}.o", obj.name_str()))
            .collect())
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>, &'static str> {
        self.root_object_map
            .iter()
            .find(|obj| {
                path.strip_prefix("/").unwrap().strip_suffix(".o").unwrap() == obj.name_str()
            })
            .map(|obj| {
                let mut bytes = [0].repeat(obj.size);
                bytes.clone_from_slice(unsafe {
                    core::slice::from_raw_parts(obj.addr as *const u8, obj.size)
                });
                bytes
            })
            .ok_or("failed to read init object")
    }
}
