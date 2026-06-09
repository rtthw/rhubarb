//! # Advanced Configuration and Power Interface (ACPI)

use {
    crate::apic,
    acpi::{AcpiTables, platform::ProcessorState},
    boot_info::BootInfo,
    core::ptr::NonNull,
    log::{debug, info, warn},
    time::MICROS_PER_SECOND,
};



pub fn init(boot_info: &BootInfo) {
    let Some(rsdp_addr) = boot_info.rsdp_address else {
        warn!("No RSDP found, skipping ACPI initialization...");
        return;
    };

    info!("Initializing ACPI @ {rsdp_addr:#x}...");

    match unsafe { AcpiTables::from_rsdp(AcpiHandler, rsdp_addr as usize) } {
        Ok(tables) => {
            if let Some(fadt) = tables.find_table::<acpi::sdt::fadt::Fadt>() {
                if let Ok(Some(pm_timer_block)) = fadt.pm_timer_block() {
                    info!(
                        "ACPI PM timer @ {:#x} ({:?})",
                        pm_timer_block.address, pm_timer_block.address_space,
                    );
                    match pm_timer_block.address_space {
                        acpi::address::AddressSpace::SystemIo => unsafe {
                            PM_TIMER_PORT = Some(pm_timer_block.address as u16);
                        },
                        _ => unimplemented!(),
                    }
                } else {
                    info!("No ACPI PM timer available");
                }
            } else {
                panic!("No FADT found");
            }

            let Ok(platform_info) = acpi::platform::AcpiPlatform::new(tables, AcpiHandler) else {
                panic!("No ACPI platform found");
            };

            if let Some(processor_info) = platform_info.processor_info {
                log_processor_info(&processor_info.boot_processor);
                assert!(processor_info.boot_processor.state == ProcessorState::Running);
                for processor in processor_info.application_processors.iter() {
                    log_processor_info(processor);

                    // None of the application processors should be running at this point.
                    assert!(processor.state != ProcessorState::Running);
                }
            }

            // Initialize the interrupt controller.
            match platform_info.interrupt_model {
                acpi::platform::InterruptModel::Apic(apic_info) => {
                    apic::init(apic_info);
                }
                _ => {
                    panic!("legacy 8259 PIC not yet supported")
                }
            }
        }
        Err(_) => {
            warn!("Could not find ACPI tables for RDSP @ {rsdp_addr:#x}");
        }
    };
}

const PM_TIMER_FREQ: u32 = 3579545;
static mut PM_TIMER_PORT: Option<u16> = None;

#[allow(unused)]
pub fn pm_timer_sleep(microseconds: u32) -> Result<(), &'static str> {
    unsafe {
        let Some(port) = PM_TIMER_PORT else {
            return Err("ACPI PM timer unavailable");
        };
        let start = x86_port::read_u32(port);
        let end = start + ((PM_TIMER_FREQ * microseconds) / MICROS_PER_SECOND as u32);

        while x86_port::read_u32(port) < end {}

        Ok(())
    }
}

#[derive(Clone, Copy)]
struct AcpiHandler;

impl acpi::Handler for AcpiHandler {
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> acpi::PhysicalMapping<Self, T> {
        let ptr = NonNull::new(physical_address as *mut T).unwrap();

        acpi::PhysicalMapping {
            physical_start: physical_address,
            virtual_start: ptr,
            region_length: size,
            mapped_length: size,
            handler: Self,
        }
    }

    fn unmap_physical_region<T>(_region: &acpi::PhysicalMapping<Self, T>) {}

    fn read_u8(&self, address: usize) -> u8 {
        unsafe { *(address as *const _) }
    }

    fn read_u16(&self, address: usize) -> u16 {
        unsafe { *(address as *const _) }
    }

    fn read_u32(&self, address: usize) -> u32 {
        unsafe { *(address as *const _) }
    }

    fn read_u64(&self, address: usize) -> u64 {
        unsafe { *(address as *const _) }
    }

    fn write_u8(&self, _address: usize, _value: u8) {
        unimplemented!()
    }

    fn write_u16(&self, _address: usize, _value: u16) {
        unimplemented!()
    }

    fn write_u32(&self, _address: usize, _value: u32) {
        unimplemented!()
    }

    fn write_u64(&self, _address: usize, _value: u64) {
        unimplemented!()
    }

    fn read_io_u8(&self, _port: u16) -> u8 {
        unimplemented!()
    }

    fn read_io_u16(&self, _port: u16) -> u16 {
        unimplemented!()
    }

    fn read_io_u32(&self, _port: u16) -> u32 {
        unimplemented!()
    }

    fn write_io_u8(&self, _port: u16, _value: u8) {
        unimplemented!()
    }

    fn write_io_u16(&self, _port: u16, _value: u16) {
        unimplemented!()
    }

    fn write_io_u32(&self, _port: u16, _value: u32) {
        unimplemented!()
    }

    fn read_pci_u8(&self, _address: acpi::PciAddress, _offset: u16) -> u8 {
        unimplemented!()
    }

    fn read_pci_u16(&self, _address: acpi::PciAddress, _offset: u16) -> u16 {
        unimplemented!()
    }

    fn read_pci_u32(&self, _address: acpi::PciAddress, _offset: u16) -> u32 {
        unimplemented!()
    }

    fn write_pci_u8(&self, _address: acpi::PciAddress, _offset: u16, _value: u8) {
        unimplemented!()
    }

    fn write_pci_u16(&self, _address: acpi::PciAddress, _offset: u16, _value: u16) {
        unimplemented!()
    }

    fn write_pci_u32(&self, _address: acpi::PciAddress, _offset: u16, _value: u32) {
        unimplemented!()
    }

    fn nanos_since_boot(&self) -> u64 {
        unimplemented!()
    }

    fn stall(&self, _microseconds: u64) {
        unimplemented!()
    }

    fn sleep(&self, _milliseconds: u64) {
        unimplemented!()
    }

    fn create_mutex(&self) -> acpi::Handle {
        unimplemented!()
    }

    fn acquire(&self, _mutex: acpi::Handle, _timeout: u16) -> Result<(), acpi::aml::AmlError> {
        unimplemented!()
    }

    fn release(&self, _mutex: acpi::Handle) {
        unimplemented!()
    }
}

fn log_processor_info(processor: &acpi::platform::Processor) {
    let kind = if processor.is_ap { "AP" } else { "BP" };
    let state = match processor.state {
        ProcessorState::Disabled => "disabled",
        ProcessorState::WaitingForSipi => "waiting",
        ProcessorState::Running => "running",
    };
    debug!(
        "CPU {} ({}, {}) = APIC_{}",
        processor.processor_uid, kind, state, processor.local_apic_id,
    );
}
