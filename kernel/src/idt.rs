//! # Interrupt Descriptor Table (IDT)

use {
    crate::{
        apic, gdt,
        loader::global_loader,
        memory::{FRAMEBUFFER_MAPPING, kernel_address_space},
        scheduler::{self, AccessPolicy, with_scheduler},
    },
    log::{error, info},
    memory_types::{PageTableFlags, VirtualAddress},
    x86_64::{
        registers::control::{Cr2, Cr3},
        set_general_handler,
        structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode},
    },
};



static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();

pub fn init() {
    info!("Initializing IDT...");

    unsafe {
        set_general_handler!(&mut IDT, unhandled_interrupt);

        IDT.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(gdt::DOUBLE_FAULT_IST);
        IDT.page_fault
            .set_handler_fn(page_fault_handler)
            .set_stack_index(gdt::PAGE_FAULT_IST);
        IDT.general_protection_fault
            .set_handler_fn(general_protection_fault_handler)
            .set_stack_index(gdt::GENERAL_PROTECTION_FAULT_IST);

        IDT[apic::TIMER_INDEX]
            .set_handler_fn(apic::timer_interrupt_handler)
            .set_stack_index(gdt::LOCAL_APIC_TIMER_IST);

        IDT[scheduler::DEFER_INTERRUPT_NUMBER]
            .set_handler_fn(scheduler::defer_interrupt_handler)
            .set_privilege_level(x86_64::PrivilegeLevel::Ring3)
            .set_stack_index(gdt::USER_IST);
        IDT[scheduler::EXIT_INTERRUPT_NUMBER]
            .set_handler_fn(scheduler::exit_interrupt_handler)
            .set_privilege_level(x86_64::PrivilegeLevel::Ring3)
            .set_stack_index(gdt::USER_IST);

        IDT.load();
    }
}

fn unhandled_interrupt(stack_frame: InterruptStackFrame, index: u8, error_code: Option<u64>) {
    panic!("UNHANDLED INTERRUPT: {index} ({error_code:?}) : {stack_frame:#?}");
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    let addr_space_frame = Cr3::read_raw().0;
    let ins_ptr = stack_frame.instruction_pointer.as_ptr::<u8>();
    let opcode = unsafe { ins_ptr.read() };

    panic!("#DF({error_code}) at `{opcode:x}` in {addr_space_frame:?} : {stack_frame:#?}");
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    let addr = Cr2::read_raw() as usize;
    let addr_space_frame = Cr3::read_raw().0;

    let user_mode = error_code.contains(PageFaultErrorCode::USER_MODE);
    let caused_by_write = error_code.contains(PageFaultErrorCode::CAUSED_BY_WRITE);

    if !user_mode {
        panic!("#PF({error_code:?}) at {addr:#x} in {addr_space_frame:?} : {stack_frame:#?}");
    }

    let Some(section) = global_loader()
        .get_section_for_addr(VirtualAddress::new(addr))
        .and_then(|weak| weak.upgrade())
    else {
        let mut exit_process = false;
        with_scheduler(|scheduler| unsafe {
            let address_space = scheduler
                .current_address_space_mut()
                .expect("should have an address space during user page fault");
            let fb_mapping = FRAMEBUFFER_MAPPING.as_mut().unwrap();
            let fb_addr = fb_mapping.addr.to_raw();
            let fb_mapping_end = fb_addr + fb_mapping.size;

            if addr >= fb_addr && addr < fb_mapping_end {
                // The framebuffer is already mapped into the address space, but it is not
                // accessible from userspace. Granting access just requires setting the flags of
                // the framebuffer's pages.
                if let Err(error) = address_space.set_flags(
                    fb_mapping.pages,
                    fb_mapping.flags | PageTableFlags::USER_ACCESSIBLE,
                ) {
                    error!(
                        "Failed to map framebuffer into `{}`: {error}",
                        address_space.name(),
                    );

                    exit_process = true;
                } else {
                    info!("Added framebuffer access to `{}`", address_space.name());
                }
            } else {
                error!(
                    "Userspace process `{}` tried to access a nonexistent section at {addr:x}",
                    address_space.name(),
                );

                exit_process = true;
            }
        });

        if exit_process {
            scheduler::exit(-2);
        }

        return;
    };

    let mut exit_process = false;
    with_scheduler(|scheduler| {
        let access_policy = scheduler
            .current_access_policy()
            .expect("current process should exist");
        let address_space = scheduler
            .current_address_space()
            .expect("should have an address space during user page fault");
        let address_space_name = address_space.name();

        let mapping = section.mapping.lock();
        match access_policy {
            AccessPolicy::All => {
                if let Err(error) = mapping.map_into(address_space, mapping.pages, mapping.flags) {
                    error!(
                        "Failed to map `{}` into `{}` for `{}` at {:x}: {error}",
                        mapping.name, address_space_name, section.name, section.addr,
                    );

                    exit_process = true;
                }
            }
            AccessPolicy::Normal => {
                if caused_by_write {
                    error!(
                        "`{}` attempted to write to `{}` for `{}` at {:x} without permission",
                        address_space_name, mapping.name, section.name, section.addr,
                    );

                    exit_process = true;
                } else {
                    // HACK: At the moment, we map dependencies as read-only. More design work is
                    //       needed to determine how dependency permissions are calculated.
                    let flags = mapping.flags & !PageTableFlags::WRITABLE;
                    if let Err(error) = mapping.map_into(address_space, mapping.pages, flags) {
                        error!(
                            "Failed to map `{}` into `{}` for `{}` at {:x}: {error}",
                            mapping.name, address_space_name, section.name, section.addr,
                        );

                        exit_process = true;
                    } else {
                        info!(
                            "Added `{}` to `{}` for `{}` at {:x}",
                            mapping.name, address_space_name, section.name, section.addr,
                        );
                    }
                }
            }
        }
    });

    if exit_process {
        scheduler::exit(-2);
    }
}

extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    let addr_space_frame = Cr3::read_raw().0;
    let ins_ptr = stack_frame.instruction_pointer.as_ptr::<u8>();
    let opcode = unsafe { ins_ptr.read() };

    if IO_PORT_OPCODES.contains(&opcode) {
        assert!(
            !kernel_address_space().is_current(),
            "Somehow, kernel failed an I/O operation! This shouldn't be possible!",
        );
        error!(
            "Attempted to use an I/O port without permission at `{opcode:x}` in \
            {addr_space_frame:?}",
        );
        scheduler::exit(-2);
    } else {
        panic!(
            "#GP at `{opcode:x}` in {addr_space_frame:?}{} : {stack_frame:#?}",
            if error_code != 0 {
                format!(" for SEGMENT {error_code}")
            } else {
                format!("")
            },
        );
    }
}

const IO_PORT_OPCODES: &[u8] = &[0xE4, 0xE5, 0xE6, 0xE7, 0xEC, 0xED, 0xEE, 0xEF];
