//! # Advanced Programmable Interrupt Controller (APIC)

use {
    crate::define_interrupt_handler_with_context,
    acpi::platform::interrupt::Apic,
    log::info,
    spin_mutex::Mutex,
    x2apic::lapic::{self, LocalApic},
};



pub const TIMER_INDEX: u8 = 32;
pub const ERROR_INDEX: u8 = 32 + 19;
pub const SPURIOUS_INDEX: u8 = 32 + 31;

static mut LOCAL_APIC: Mutex<Option<LocalApic>> = Mutex::new(None);


pub fn init(info: Apic) {
    info!("Initializing APIC @ {:#x}...", info.local_apic_address);

    unsafe {
        *LOCAL_APIC.lock() = Some(
            lapic::LocalApicBuilder::new()
                .error_vector(ERROR_INDEX as usize)
                .spurious_vector(SPURIOUS_INDEX as usize)
                .timer_vector(TIMER_INDEX as usize)
                .set_xapic_base(info.local_apic_address)
                .build()
                .expect("failed to build lapic"),
        );

        info!("Enabling APIC...");

        LOCAL_APIC
            .lock()
            .as_mut()
            .expect("local APIC exists")
            .enable();
    }
}

define_interrupt_handler_with_context!(|timer_interrupt_handler| {
    end_of_interrupt();
    with_scheduler(|scheduler| scheduler.preempt_current());
});

fn end_of_interrupt() {
    unsafe {
        LOCAL_APIC
            .lock()
            .as_mut()
            .expect("APIC initialized")
            .end_of_interrupt()
    };
}
