//! # User Input

use {
    crate::{loader::global_loader, memory::kernel_address_space, scheduler},
    alloc::vec::Vec,
    input::{InputEvent, InputQueue},
    log::warn,
    memory_types::VirtualAddress,
    spin_mutex::Mutex,
    virtio::virtio_input,
};


pub fn dispatch_input_events() -> ! {
    let mut virtio_inputs = {
        let virtual_to_physical_addr = |vaddr| {
            kernel_address_space()
                .translate_address(VirtualAddress::new(vaddr))
                .expect("should be able to translate virtual addresses to physical ones")
                .to_raw()
        };

        pci::enumerate_devices()
            .into_iter()
            .filter(|dev| dev.vendor_id == 0x1af4 && dev.device_id == 0x1040 + 18)
            .filter_map(|pci_device| {
                let device_num = pci_device.device;
                virtio_input::Device::new(pci_device, &virtual_to_physical_addr)
                    .inspect_err(|err| {
                        warn!("Failed to create input device for PCI device #{device_num}: {err}");
                    })
                    .ok()
            })
            .collect::<Vec<_>>()
    };

    let queue_section = global_loader()
        .get_section("input", "GLOBAL_INPUT_QUEUE")
        .unwrap()
        .upgrade()
        .unwrap();

    loop {
        for input_device in virtio_inputs.iter_mut() {
            for input_event in input_device.poll() {
                // trace!("RAW_INPUT: {input_event:?}");
                if let Some(event) = convert_input_event(input_event) {
                    let exit = matches!(
                        event,
                        InputEvent::KeyPress {
                            code: virtio_input::codes::KEY_Q,
                        },
                    );

                    let mut mapping = queue_section.mapping.lock();
                    unsafe {
                        let queue =
                            mapping.as_mut::<Mutex<InputQueue>>(queue_section.mapping_offset);
                        queue.lock().push(event);
                    }

                    // window_manager::send_event(window_manager::Event::UserInput(event));

                    if exit {
                        scheduler::exit(45);
                    }
                }
            }
        }
        scheduler::defer();
    }
}

fn convert_input_event(event: virtio_input::InputEvent) -> Option<InputEvent> {
    match event.type_ {
        virtio_input::InputEventType::SYN => {
            // Ignore sync events.
        }
        virtio_input::InputEventType::KEY => {
            if event.value == 0 {
                return Some(InputEvent::KeyPress { code: event.code.0 });
            }
        }
        virtio_input::InputEventType::REL => match event.code {
            virtio_input::InputEventCode::REL_X => {
                let delta = event.value as i32;
                return Some(InputEvent::MouseMove {
                    delta_x: delta,
                    delta_y: 0,
                });
            }
            virtio_input::InputEventCode::REL_Y => {
                let delta = event.value as i32;
                return Some(InputEvent::MouseMove {
                    delta_x: 0,
                    delta_y: delta,
                });
            }
            virtio_input::InputEventCode::REL_WHEEL => {
                let delta = event.value as i32;
                return Some(InputEvent::MouseWheel { delta });
            }

            _ => warn!("Unhandled VirtIO pointer input event code {:?}", event.code),
        },

        _ => warn!("Unhandled VirtIO input event type {:?}", event.type_),
    }

    None
}
