//! # Virtual I/O Input Device

use {
    crate::{Virtqueue, VirtqueueMessage},
    alloc::vec::Vec,
    core::ops::{Deref, DerefMut},
};


const INPUT_EVENT_SIZE: usize = size_of::<InputEvent>();

pub struct Device {
    virtio_device: crate::Device,
    event_queue: Virtqueue<64, INPUT_EVENT_SIZE>,
}

impl Device {
    /// Create a new VirtIO input device from the given [PCI
    /// device](pci::Device).
    ///
    /// Returns `Err` if the given PCI device is not a VirtIO input device (i.e.
    /// it doesn't have the correct configuration, or isn't an input
    /// device).
    pub fn new(pci_device: pci::Device) -> Result<Self, &'static str> {
        let mut virtio_device = crate::Device::new(pci_device)?;
        let mut event_queue = virtio_device.initialize(0, |dev| dev.initialize_queue(0));

        let msg = [VirtqueueMessage::<InputEvent>::DeviceWrite];
        unsafe { while event_queue.push(&msg).is_ok() {} };

        Ok(Self {
            virtio_device,
            event_queue,
        })
    }

    pub fn poll(&mut self) -> Vec<InputEvent> {
        let mut out = Vec::new();

        while let Some(resp_list) = unsafe { self.event_queue.pop::<1, _>() } {
            let event = resp_list.into_iter().next().unwrap();
            out.push(event.unwrap());

            unsafe {
                self.event_queue
                    .push(&[VirtqueueMessage::<InputEvent>::DeviceWrite])
                    .unwrap();
            }
        }

        out
    }
}

impl Deref for Device {
    type Target = crate::Device;

    fn deref(&self) -> &Self::Target {
        &self.virtio_device
    }
}

impl DerefMut for Device {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.virtio_device
    }
}

#[derive(Clone, Debug, Default)]
#[repr(C)]
pub struct InputEvent {
    pub type_: InputEventType,
    pub code: InputEventCode,
    pub value: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct InputEventType(pub u16);

impl InputEventType {
    pub const SYN: Self = Self(codes::EV_SYN);
    pub const KEY: Self = Self(codes::EV_KEY);
    pub const REL: Self = Self(codes::EV_REL);
    pub const ABS: Self = Self(codes::EV_ABS);
    pub const MSC: Self = Self(codes::EV_MSC);
    pub const SW: Self = Self(codes::EV_SW);
    pub const LED: Self = Self(codes::EV_LED);
    pub const SND: Self = Self(codes::EV_SND);
    pub const REP: Self = Self(codes::EV_REP);
    pub const FF: Self = Self(codes::EV_FF);
    pub const PWR: Self = Self(codes::EV_PWR);
    pub const FF_STATUS: Self = Self(codes::EV_FF_STATUS);
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct InputEventCode(pub u16);

impl InputEventCode {
    pub const REL_X: Self = Self(codes::REL_X);
    pub const REL_Y: Self = Self(codes::REL_Y);
    pub const REL_Z: Self = Self(codes::REL_Z);
    pub const REL_RX: Self = Self(codes::REL_RX);
    pub const REL_RY: Self = Self(codes::REL_RY);
    pub const REL_RZ: Self = Self(codes::REL_RZ);
    pub const REL_HWHEEL: Self = Self(codes::REL_HWHEEL);
    pub const REL_DIAL: Self = Self(codes::REL_DIAL);
    pub const REL_WHEEL: Self = Self(codes::REL_WHEEL);
    pub const REL_MISC: Self = Self(codes::REL_MISC);
    pub const REL_RESERVED: Self = Self(codes::REL_RESERVED);
    pub const REL_WHEEL_HI_RES: Self = Self(codes::REL_WHEEL_HI_RES);
    pub const REL_HWHEEL_HI_RES: Self = Self(codes::REL_HWHEEL_HI_RES);

    pub const KEY_RESERVED: Self = Self(codes::KEY_RESERVED);
    pub const KEY_ESC: Self = Self(codes::KEY_ESC);
    pub const KEY_1: Self = Self(codes::KEY_1);
    pub const KEY_2: Self = Self(codes::KEY_2);
    pub const KEY_3: Self = Self(codes::KEY_3);
    pub const KEY_4: Self = Self(codes::KEY_4);
    pub const KEY_5: Self = Self(codes::KEY_5);
    pub const KEY_6: Self = Self(codes::KEY_6);
    pub const KEY_7: Self = Self(codes::KEY_7);
    pub const KEY_8: Self = Self(codes::KEY_8);
    pub const KEY_9: Self = Self(codes::KEY_9);
    pub const KEY_0: Self = Self(codes::KEY_0);

    // TODO: The rest of the key codes...
}

pub mod codes {
    //! # Input Codes

    #![allow(unused)]

    pub const EV_SYN: u16 = 0x00;
    pub const EV_KEY: u16 = 0x01;
    pub const EV_REL: u16 = 0x02;
    pub const EV_ABS: u16 = 0x03;
    pub const EV_MSC: u16 = 0x04;
    pub const EV_SW: u16 = 0x05;
    pub const EV_LED: u16 = 0x11;
    pub const EV_SND: u16 = 0x12;
    pub const EV_REP: u16 = 0x14;
    pub const EV_FF: u16 = 0x15;
    pub const EV_PWR: u16 = 0x16;
    pub const EV_FF_STATUS: u16 = 0x17;

    pub const REL_X: u16 = 0x00;
    pub const REL_Y: u16 = 0x01;
    pub const REL_Z: u16 = 0x02;
    pub const REL_RX: u16 = 0x03;
    pub const REL_RY: u16 = 0x04;
    pub const REL_RZ: u16 = 0x05;
    pub const REL_HWHEEL: u16 = 0x06;
    pub const REL_DIAL: u16 = 0x07;
    pub const REL_WHEEL: u16 = 0x08;
    pub const REL_MISC: u16 = 0x09;
    pub const REL_RESERVED: u16 = 0x0a;
    pub const REL_WHEEL_HI_RES: u16 = 0x0b;
    pub const REL_HWHEEL_HI_RES: u16 = 0x0c;

    pub const KEY_RESERVED: u16 = 0;
    pub const KEY_ESC: u16 = 1;
    pub const KEY_1: u16 = 2;
    pub const KEY_2: u16 = 3;
    pub const KEY_3: u16 = 4;
    pub const KEY_4: u16 = 5;
    pub const KEY_5: u16 = 6;
    pub const KEY_6: u16 = 7;
    pub const KEY_7: u16 = 8;
    pub const KEY_8: u16 = 9;
    pub const KEY_9: u16 = 10;
    pub const KEY_0: u16 = 11;
    pub const KEY_MINUS: u16 = 12;
    pub const KEY_EQUAL: u16 = 13;
    pub const KEY_BACKSPACE: u16 = 14;
    pub const KEY_TAB: u16 = 15;
    pub const KEY_Q: u16 = 16;
    pub const KEY_W: u16 = 17;
    pub const KEY_E: u16 = 18;
    pub const KEY_R: u16 = 19;
    pub const KEY_T: u16 = 20;
    pub const KEY_Y: u16 = 21;
    pub const KEY_U: u16 = 22;
    pub const KEY_I: u16 = 23;
    pub const KEY_O: u16 = 24;
    pub const KEY_P: u16 = 25;
    pub const KEY_LEFTBRACE: u16 = 26;
    pub const KEY_RIGHTBRACE: u16 = 27;
    pub const KEY_ENTER: u16 = 28;
    pub const KEY_LEFTCTRL: u16 = 29;
    pub const KEY_A: u16 = 30;
    pub const KEY_S: u16 = 31;
    pub const KEY_D: u16 = 32;
    pub const KEY_F: u16 = 33;
    pub const KEY_G: u16 = 34;
    pub const KEY_H: u16 = 35;
    pub const KEY_J: u16 = 36;
    pub const KEY_K: u16 = 37;
    pub const KEY_L: u16 = 38;
    pub const KEY_SEMICOLON: u16 = 39;
    pub const KEY_APOSTROPHE: u16 = 40;
    pub const KEY_GRAVE: u16 = 41;
    pub const KEY_LEFTSHIFT: u16 = 42;
    pub const KEY_BACKSLASH: u16 = 43;
    pub const KEY_Z: u16 = 44;
    pub const KEY_X: u16 = 45;
    pub const KEY_C: u16 = 46;
    pub const KEY_V: u16 = 47;
    pub const KEY_B: u16 = 48;
    pub const KEY_N: u16 = 49;
    pub const KEY_M: u16 = 50;
    pub const KEY_COMMA: u16 = 51;
    pub const KEY_DOT: u16 = 52;
    pub const KEY_SLASH: u16 = 53;
    pub const KEY_RIGHTSHIFT: u16 = 54;
    pub const KEY_KPASTERISK: u16 = 55;
    pub const KEY_LEFTALT: u16 = 56;
    pub const KEY_SPACE: u16 = 57;
    pub const KEY_CAPSLOCK: u16 = 58;
    pub const KEY_F1: u16 = 59;
    pub const KEY_F2: u16 = 60;
    pub const KEY_F3: u16 = 61;
    pub const KEY_F4: u16 = 62;
    pub const KEY_F5: u16 = 63;
    pub const KEY_F6: u16 = 64;
    pub const KEY_F7: u16 = 65;
    pub const KEY_F8: u16 = 66;
    pub const KEY_F9: u16 = 67;
    pub const KEY_F10: u16 = 68;
    pub const KEY_NUMLOCK: u16 = 69;
    pub const KEY_SCROLLLOCK: u16 = 70;
    // TODO: 71..=99
    pub const KEY_RIGHTALT: u16 = 100;
    pub const KEY_LINEFEED: u16 = 101;
    pub const KEY_HOME: u16 = 102;
    pub const KEY_UP: u16 = 103;
    pub const KEY_PAGEUP: u16 = 104;
    pub const KEY_LEFT: u16 = 105;
    pub const KEY_RIGHT: u16 = 106;
    pub const KEY_END: u16 = 107;
    pub const KEY_DOWN: u16 = 108;
    pub const KEY_PAGEDOWN: u16 = 109;
    pub const KEY_INSERT: u16 = 110;
    pub const KEY_DELETE: u16 = 111;
    // TODO: 112..=124
    pub const KEY_LEFTMETA: u16 = 125;
    pub const KEY_RIGHTMETA: u16 = 126;
    // TODO: 127..=271
    pub const BTN_LEFT: u16 = 0x110;
    pub const BTN_RIGHT: u16 = 0x111;
    pub const BTN_MIDDLE: u16 = 0x112;
    pub const BTN_SIDE: u16 = 0x113;
    pub const BTN_EXTRA: u16 = 0x114;
    pub const BTN_FORWARD: u16 = 0x115;
    pub const BTN_BACK: u16 = 0x116;
    pub const BTN_TASK: u16 = 0x117;
    // TODO: 279..=335
    pub const BTN_GEAR_DOWN: u16 = 0x150;
    pub const BTN_GEAR_UP: u16 = 0x151;
    // TODO: 338..
}
