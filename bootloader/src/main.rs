#![no_std]
#![no_main]

#[macro_use]
extern crate alloc;

mod serial;

use {
    alloc::{string::ToString, vec::Vec},
    boot_info::{
        BootInfo, DisplayInfo, MAX_OBJECT_NAME_LEN, MemoryRegion, MemoryRegionKind, RootObjectInfo,
    },
    elf::{ElfFile, ProgramHeaderType},
    log::{debug, info, warn},
    memory_types::PAGE_SIZE,
    uefi::{
        CStr16, Status,
        boot::{self, AllocateType, MemoryType},
        cstr16, entry,
        mem::memory_map::MemoryMap as _,
        proto::{
            console::gop::GraphicsOutput,
            media::{file::*, fs::SimpleFileSystem},
        },
        system,
        table::cfg::ConfigTableEntry,
    },
};


const KERNEL_PATH: &CStr16 = cstr16!("kernel");

#[entry]
fn main() -> Status {
    log::set_max_level(log::LevelFilter::Trace);
    log::set_logger(&serial::SerialLogger).unwrap();

    info!("BOOT");

    let (kernel_start, kernel_end, kernel_entry_point) = load_kernel();
    info!("Kernel entry point @ {:#x}", kernel_entry_point);

    let root_objects = load_root_objects();

    let display_info = get_display_info();

    let rsdp_address = system::with_config_table(|e| {
        let acpi2_entry = e.iter().find(|e| e.guid == ConfigTableEntry::ACPI2_GUID);
        acpi2_entry.map(|e| e.address as u64)
    });
    if let Some(addr) = rsdp_address {
        info!("RSDP @ {addr:#x}");
    } else {
        warn!("RSDP not found");
    }

    info!("Jumping to kernel...");

    let mut memory_regions = Vec::with_capacity(
        boot::memory_map(MemoryType::RUNTIME_SERVICES_DATA)
            .unwrap()
            .len()
            + 8, // Make sure there is enough space for the descriptors.
    );

    // After this point, we cannot allocate any memory.
    let memory_map = unsafe { boot::exit_boot_services(Some(MemoryType::RUNTIME_SERVICES_DATA)) };

    assert!(
        memory_regions.capacity() >= memory_map.len(),
        "failed to allocate enough memory for the physical memory map",
    );

    for desc in memory_map.entries() {
        memory_regions.push(MemoryRegion {
            base: desc.phys_start as usize,
            size: desc.page_count as usize * PAGE_SIZE,
            kind: match desc.ty {
                MemoryType::CONVENTIONAL
                | MemoryType::LOADER_CODE
                | MemoryType::LOADER_DATA
                | MemoryType::BOOT_SERVICES_CODE
                | MemoryType::BOOT_SERVICES_DATA => MemoryRegionKind::Free,
                tag => MemoryRegionKind::Uefi(tag.0),
            },
        });
    }

    let boot_info = BootInfo {
        rsdp_address,
        kernel_start,
        kernel_end,
        memory_map: memory_regions.leak().into(),
        root_object_map: root_objects.leak().into(),
        display_info,
    };

    let entry_point: extern "sysv64" fn(*const BootInfo) =
        unsafe { core::mem::transmute(kernel_entry_point) };
    entry_point(&boot_info);

    Status::SUCCESS
}

fn load_kernel() -> (
    /* start: */ usize,
    /* end: */ usize,
    /* entry_point: */ u64,
) {
    let fs = boot::get_handle_for_protocol::<SimpleFileSystem>().unwrap();
    let mut root = boot::open_protocol_exclusive::<SimpleFileSystem>(fs)
        .unwrap()
        .open_volume()
        .unwrap();
    let file_type = root
        .open(KERNEL_PATH, FileMode::Read, FileAttribute::empty())
        .unwrap()
        .into_type()
        .unwrap();
    let mut file = match file_type {
        FileType::Regular(file) => file,
        FileType::Dir(_) => panic!("kernel path does not point to a file"),
    };
    let file_info = file.get_boxed_info::<FileInfo>().unwrap();
    let file_size = file_info.file_size() as usize;

    let mut buf = vec![0; file_size];
    file.read(&mut buf).unwrap();

    let elf = ElfFile::new(&buf).unwrap();

    info!("Searching for kernel program range...");

    let mut start_addr = usize::MAX;
    let mut end_addr = 0;

    for program_header in elf.program_iter() {
        if program_header.get_type().unwrap() != ProgramHeaderType::Load {
            continue;
        }

        // info!(
        //     "PH_LOAD @ {:#x}..{:#x}",
        //     program_header.virtual_addr,
        //     program_header.virtual_addr + program_header.mem_size,
        // );

        start_addr = start_addr.min(program_header.virtual_addr as usize);
        end_addr = end_addr.max((program_header.virtual_addr + program_header.mem_size) as usize);
    }

    info!("Copying kernel data to allocation...");

    let page_count = (end_addr - start_addr).div_ceil(PAGE_SIZE);
    boot::allocate_pages(
        AllocateType::Address(start_addr as u64),
        MemoryType::LOADER_DATA,
        page_count,
    )
    .unwrap();

    for program_header in elf.program_iter() {
        if program_header.get_type().unwrap() != ProgramHeaderType::Load {
            continue;
        }

        let addr = program_header.virtual_addr;
        let offset = program_header.offset as usize;
        let size_in_file = program_header.file_size as usize;
        let size_in_memory = program_header.mem_size as usize;

        info!(
            "\tKernel header @ {addr:#x} ({offset:#x}): FILE={:#x}, MEM={:#x}",
            size_in_file, size_in_memory
        );

        let dst = unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, size_in_memory) };
        dst[..size_in_file].copy_from_slice(&buf[offset..offset + size_in_file]);
        dst[size_in_file..].fill(0);
    }

    info!(
        "Loaded kernel file @ {:#x}..{:#x} ({} bytes)",
        start_addr, end_addr, file_size,
    );

    (start_addr, end_addr, elf.header.body.entry_point)
}

fn load_root_objects() -> Vec<RootObjectInfo> {
    let fs = boot::get_handle_for_protocol::<SimpleFileSystem>().unwrap();
    let mut root = boot::open_protocol_exclusive::<SimpleFileSystem>(fs)
        .unwrap()
        .open_volume()
        .unwrap();

    let mut objects = Vec::new();
    while let Some(info) = root.read_entry_boxed().unwrap() {
        let file_name = info.file_name().to_string();
        if !file_name.ends_with(".o") {
            continue;
        }

        let mut file = root
            .open(info.file_name(), FileMode::Read, FileAttribute::empty())
            .unwrap()
            .into_regular_file()
            .unwrap();

        let size = info.file_size() as usize;
        let page_count = size.div_ceil(PAGE_SIZE);
        let ptr = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, page_count)
            .unwrap();
        let mut buf =
            unsafe { core::slice::from_raw_parts_mut(ptr.as_ptr(), page_count * PAGE_SIZE) };

        let read_end = file.read(&mut buf).unwrap();
        assert_eq!(read_end, size);
        buf[read_end..].fill(0);

        let mut name = [0_u8; MAX_OBJECT_NAME_LEN];
        let actual_name_end = file_name.len() - 2;
        assert!(actual_name_end <= MAX_OBJECT_NAME_LEN);
        name[..actual_name_end].clone_from_slice(&file_name.as_bytes()[..actual_name_end]);

        let addr = ptr.addr().into();

        debug!("Root object: `{file_name}` ({size} bytes) @ {addr:x}");

        objects.push(RootObjectInfo { name, addr, size });
    }

    objects
}

fn get_display_info() -> DisplayInfo {
    let handle = boot::get_handle_for_protocol::<GraphicsOutput>().unwrap();
    let mut gop = boot::open_protocol_exclusive::<GraphicsOutput>(handle).unwrap();

    // let mode = gop
    //     .modes()
    //     .max_by_key(|mode| mode.info().resolution())
    //     .expect("no display mode available");
    // gop.set_mode(&mode).unwrap();

    let mode = gop.current_mode_info();
    let (width, height) = mode.resolution();

    DisplayInfo {
        width: width as u32,
        height: height as u32,
        stride: mode.stride() as u32,
        format: match mode.pixel_format() {
            uefi::proto::console::gop::PixelFormat::Rgb => boot_info::PixelFormat::Rgb,
            uefi::proto::console::gop::PixelFormat::Bgr => boot_info::PixelFormat::Bgr,
            _ => panic!("unsupported pixel format"),
        },
        framebuffer_addr: gop.frame_buffer().as_mut_ptr() as u64,
        framebuffer_size: gop.frame_buffer().size(),
    }
}



#[cfg(target_os = "uefi")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    log::error!("{info}");
    loop {}
}
