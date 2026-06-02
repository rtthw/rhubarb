//! # Example Driver

#![no_std]

extern crate alloc;

use {alloc::vec::Vec, heap::Allocator, input::InputEvent, virtio::virtio_input};



#[global_allocator]
static ALLOCATOR: Allocator = Allocator::new();

pub extern "C" fn main() -> ! {
    unsafe {
        ALLOCATOR.init(heap::BASE_ADDR, heap::DEFAULT_SIZE);
    }

    // FIXME: For some reason, using both the keyboard and mouse here causes a
    //        zero-sized buffer to be sent to one of the devices. This will poll
    //        both devices when I figure out what's going on.
    let mut virtio_inputs = Vec::with_capacity(1);
    virtio_inputs.push(
        pci::enumerate_devices()
            .into_iter()
            .filter(|dev| dev.vendor_id == 0x1af4 && dev.device_id == 0x1040 + 18)
            .skip(1) // Handle mouse only, for now.
            .find_map(|pci_device| virtio_input::Device::new(pci_device).ok())
            .unwrap(),
    );

    log::info!("Input driver starting...");

    loop {
        for input_device in virtio_inputs.iter_mut() {
            for input_event in input_device.poll() {
                if let Some(event) = convert_input_event(input_event) {
                    let exit = matches!(
                        event,
                        InputEvent::KeyPress {
                            code: virtio_input::codes::KEY_Q,
                        },
                    );

                    input::GLOBAL_INPUT_QUEUE.lock().push(event);

                    if exit {
                        process::exit(0);
                    }
                }
            }
        }

        process::defer();
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

            _ => {}
        },

        _ => {}
    }

    None
}
