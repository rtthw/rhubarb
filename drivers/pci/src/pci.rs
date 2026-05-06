//! # Peripheral Component Interconnect (PCI)

#![no_std]

#[macro_use]
extern crate alloc;

mod pci_capability;
mod pci_class;

use {
    alloc::vec::Vec,
    bit_utils::{bit_field, bit_range},
    core::fmt::Debug,
};

pub use {pci_capability::*, pci_class::*};

pub const CONFIG_ADDRESS: u16 = 0xCF8;
pub const CONFIG_DATA: u16 = 0xCFC;

const CONFIG_SPAGE_REG_0_OFFSET: u8 = 0x0;
// const CONFIG_SPAGE_REG_1_OFFSET: u8 = 0x4;
const CONFIG_SPAGE_REG_2_OFFSET: u8 = 0x8;
const CONFIG_SPAGE_REG_3_OFFSET: u8 = 0xC;

const NONEXISTENT_VENDOR_ID: u16 = 0xFFFF;


pub fn enumerate_devices() -> Vec<Device> {
    let mut devices = vec![];
    for bus in 0..=255 {
        for id in 0..32 {
            if let Some(device) = Device::open(bus, id, 0) {
                if device.header_type.multiple_functions() {
                    for function in 1..8 {
                        if let Some(device) = Device::open(bus, id, function) {
                            devices.push(device);
                        }
                    }
                }
                devices.push(device);
            }
        }
    }

    devices
}

pub unsafe fn read(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let bus = bus as u32;
    let device = device as u32;
    let function = function as u32;
    let offset = offset as u32;

    let address =
        ((bus << 16) | (device << 11) | (function << 8) | (offset & 0xFC) | 0x80000000) as u32;

    unsafe {
        x86_port::write_u32(CONFIG_ADDRESS, address);
        x86_port::read_u32(CONFIG_DATA)
    }
}

pub unsafe fn write(bus: u8, device: u8, function: u8, offset: u8, value: u32) {
    let bus = bus as u32;
    let device = device as u32;
    let function = function as u32;
    let offset = offset as u32;

    let address =
        ((bus << 16) | (device << 11) | (function << 8) | (offset & 0xfc) | 0x80000000) as u32;

    unsafe {
        x86_port::write_u32(CONFIG_ADDRESS, address);
        x86_port::write_u32(CONFIG_DATA, value);
    }
}



#[derive(Clone)]
pub struct Device {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub device_id: u16,
    pub vendor_id: u16,
    pub class: Class,
    pub subclass: u8,
    pub header_type: HeaderType,
    pub interface: u8,
    pub interrupt_line: u8,
    pub interrupt_pin: u8,
}

impl Device {
    pub fn open(bus: u8, device: u8, function: u8) -> Option<Self> {
        let reg_0 =
            ConfigSpaceRegister0(unsafe { read(bus, device, function, CONFIG_SPAGE_REG_0_OFFSET) });

        if reg_0.vendor_id() == NONEXISTENT_VENDOR_ID {
            return None;
        }

        let reg_2 =
            ConfigSpaceRegister2(unsafe { read(bus, device, function, CONFIG_SPAGE_REG_2_OFFSET) });
        let reg_3 =
            ConfigSpaceRegister3(unsafe { read(bus, device, function, CONFIG_SPAGE_REG_3_OFFSET) });

        let class = Class::from_raw(reg_2.class())?;

        let last_row = unsafe { read(bus, device, 0, 0x3C) };
        let interrupt_line = (last_row & 0xFF) as u8;
        let interrupt_pin = ((last_row >> 8) & 0xFF) as u8;

        Some(Self {
            bus,
            device,
            function,
            device_id: reg_0.device_id(),
            vendor_id: reg_0.vendor_id(),
            class,
            subclass: reg_2.subclass(),
            header_type: HeaderType(reg_3.header_type()),
            interface: reg_2.prog_if(),
            interrupt_line,
            interrupt_pin,
        })
    }

    pub fn name(&self) -> &'static str {
        match (self.vendor_id, self.device_id) {
            (VENDOR_RED_HAT, red_hat_device) => match red_hat_device {
                0x1000 => "Virtio network device",
                0x1001 => "Virtio block device",
                0x1041 => "Virtio 1.0 network device",
                0x1042 => "Virtio 1.0 block device",
                0x1043 => "Virtio 1.0 console",
                0x1044 => "Virtio 1.0 RNG",
                0x1050 => "Virtio 1.0 GPU",
                0x1051 => "Virtio 1.0 clock/timer",
                0x1052 => "Virtio 1.0 input",

                _ => "Unknown Red Hat, Inc. device",
            },
            (VENDOR_INTEL, intel_device) => match intel_device {
                0x100E => "82540EM Gigabit Ethernet Controller",
                0x1237 => "82441FX PMC [Natoma]",
                0x7000 => "82371SB PIIX3 ISA [Natoma/Triton II]",

                _ => "Unknown Intel Corp. device",
            },

            (_, _) => "Unknown device",
        }
    }

    pub fn capabilities(&self) -> impl Iterator<Item = DeviceCapability> {
        CapabilityIter::new(self.bus, self.device, self.function)
    }

    pub fn bar(&self, slot: u8) -> Option<Bar> {
        if slot >= 6 {
            return None;
        }

        let offset = 16 + slot * 4;
        let bar = RawBaseAddressRegister(unsafe { self.read(offset) });

        if !bar.io() {
            let prefetchable = bar.mem_prefetchable();
            let address = bar.mem_address() << 4;

            match bar.mem_type() {
                0 => {
                    let size = unsafe {
                        self.write(offset, 0xFFFFFFFF);
                        let readback = self.read(offset);
                        self.write(offset, address);

                        // BAR is unimplemented.
                        if readback == 0 {
                            return None;
                        }

                        1 << u32_set_range(readback, 0, 4, 0).trailing_zeros()
                    };

                    Some(Bar::Mem32 {
                        address,
                        size,
                        prefetchable,
                    })
                }
                2 => {
                    // If we are looking at the last slot, then we can't read a 64-bit value.
                    if slot >= 5 {
                        return None;
                    }

                    let address_upper = unsafe { self.read(offset + 4) };

                    let size = unsafe {
                        self.write(offset, 0xFFFFFFFF);
                        self.write(offset + 4, 0xFFFFFFFF);
                        let mut readback_low = self.read(offset);
                        let readback_high = self.read(offset + 4);
                        self.write(offset, address);
                        self.write(offset + 4, address_upper);

                        readback_low = u32_set_range(readback_low, 0, 4, 0);
                        if readback_low != 0 {
                            (1 << readback_low.trailing_zeros()) as u64
                        } else {
                            1 << ((readback_high.trailing_zeros() + 32) as u64)
                        }
                    };

                    let address = u64_set_range(address as u64, 32, 64, address_upper as u64);

                    Some(Bar::Mem64 {
                        address,
                        size,
                        prefetchable,
                    })
                }

                _ => panic!("unknown PCI BAR memory type"),
            }
        } else {
            Some(Bar::Io {
                address: bar.io_address(),
            })
        }
    }

    pub unsafe fn read(&self, offset: u8) -> u32 {
        unsafe { read(self.bus, self.device, self.function, offset) }
    }

    pub unsafe fn write(&self, offset: u8, value: u32) {
        unsafe { write(self.bus, self.device, self.function, offset, value) }
    }

    pub unsafe fn read_struct<T: Clone>(&self, offset: u8) -> T {
        let size = size_of::<T>();
        assert_eq!(size % 4, 0);
        let num_words = size / 4;

        let buf: Vec<u32> = (0..num_words)
            .map(|i| {
                let i: u8 = i.try_into().unwrap();
                unsafe { self.read(offset + 4 * i) }
            })
            .collect();

        let ptr = buf.as_ptr() as *const T;

        unsafe { ptr.as_ref().unwrap().clone() }
    }

    pub fn set_msix(&self, enabled: bool) {
        let Some(cap) = self
            .capabilities()
            .into_iter()
            .find(|cap| cap.id == Capability::MsiX)
        else {
            return;
        };

        let mut word = unsafe { self.read(cap.offset) };
        word = *u32_set_bit(&mut word, 31, enabled);

        unsafe { self.write(cap.offset, word) };
    }
}

impl Debug for Device {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut debug_struct = f.debug_struct(&format!("#{} '{}'", self.device, self.name()));

        debug_struct
            .field("id", &self.device_id)
            .field(
                "vendor",
                match &self.vendor_id {
                    &VENDOR_RED_HAT => &"Red Hat, Inc.",
                    &VENDOR_INTEL => &"Intel Corp." as &dyn Debug,
                    other => &*other as &dyn Debug,
                },
            )
            .field("bus", &self.bus)
            .field("function", &self.function)
            .field("class", &self.class)
            .field("subclass", &self.subclass)
            .field("header_type", &self.header_type);

        for slot in 0..6 {
            let Some(bar) = self.bar(slot) else {
                continue;
            };
            debug_struct.field(&format!("bar_{slot}"), &bar);
        }

        debug_struct
            .field("programming_interface", &self.interface)
            .field("interrupt_line", &self.interrupt_line)
            .field("interrupt_pin", &self.interrupt_pin)
            // .field("capabilities", &self.capabilities())
            .finish()
    }
}



bit_field! {
    struct ConfigSpaceRegister0: u32 {
        vendor_id: u16 = 0..16,
        device_id: u16 = 16..32,
    }
}

bit_field! {
    struct ConfigSpaceRegister1: u32 {
        command: u16 = 0..16,
        status: u16 = 16..32,
    }
}

bit_field! {
    struct ConfigSpaceRegister2: u32 {
        revision_id: u8 = 0..8,
        prog_if: u8 = 8..16,
        subclass: u8 = 16..24,
        class: u8 = 24..32,
    }
}

bit_field! {
    struct ConfigSpaceRegister3: u32 {
        cache_line_size: u8 = 0..8,
        latency_timer: u8 = 8..16,
        header_type: u8 = 16..24,
        bist: u8 = 24..32,
    }
}



#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct HeaderType(u8);

const HEADER_TYPE_MULTIPLE_FUNCTIONS_BIT: u8 = 1 << 7;

const HEADER_TYPE_STANDARD: u8 = 0;
const HEADER_TYPE_PCI_BRIDGE: u8 = 1;
const HEADER_TYPE_CARBUS_BRIDGE: u8 = 2;

impl Debug for HeaderType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!(
            "{}{}",
            if self.multiple_functions() {
                "Multiple-Function "
            } else {
                ""
            },
            match self.get() {
                HEADER_TYPE_STANDARD => "Standard",
                HEADER_TYPE_PCI_BRIDGE => "PCI Bridge",
                HEADER_TYPE_CARBUS_BRIDGE => "CardBus Bridge",

                _ => "INVALID",
            },
        ))
    }
}

impl HeaderType {
    /// Get this device's actual header type by masking away the multiple
    /// functions bit.
    pub const fn get(&self) -> u8 {
        self.0 & !HEADER_TYPE_MULTIPLE_FUNCTIONS_BIT
    }

    /// Whether this device has multiple functions.
    ///
    /// See [https://wiki.osdev.org/PCI#Multi-function_Devices] for more information.
    pub const fn multiple_functions(&self) -> bool {
        self.0 & HEADER_TYPE_MULTIPLE_FUNCTIONS_BIT == HEADER_TYPE_MULTIPLE_FUNCTIONS_BIT
    }

    /// Whether this is a standard PCI device.
    // https://wiki.osdev.org/PCI#Header_Type_0x0
    pub const fn is_standard(&self) -> bool {
        self.get() == HEADER_TYPE_STANDARD
    }

    /// Whether this is a PCI-to-PCI bridge device.
    // https://wiki.osdev.org/PCI#Header_Type_0x1_(PCI-to-PCI_bridge)
    pub const fn is_pci_bridge(&self) -> bool {
        self.get() == HEADER_TYPE_PCI_BRIDGE
    }

    /// Whether this is a PCI-to-CardBus bridge device.
    // https://wiki.osdev.org/PCI#Header_Type_0x2_(PCI-to-CardBus_bridge)
    pub const fn is_cardbus_bridge(&self) -> bool {
        self.get() == HEADER_TYPE_CARBUS_BRIDGE
    }
}



#[derive(Clone)]
pub struct DeviceCapability {
    pub id: Capability,
    pub offset: u8,
}

impl Debug for DeviceCapability {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("{:?} @ {:#x}", self.id, self.offset))
    }
}



#[derive(Clone, Debug)]
pub enum Bar {
    Mem32 {
        address: u32,
        size: u32,
        prefetchable: bool,
    },
    Mem64 {
        address: u64,
        size: u64,
        prefetchable: bool,
    },
    Io {
        address: u32,
    },
}

bit_field! {
    struct RawBaseAddressRegister: u32 {
        io: bool = 0,
        io_address: u32 = 2..32,
        mem_type: u8 = 1..3,
        mem_prefetchable: bool = 3,
        mem_address: u32 = 4..32,
    }
}



struct CapabilityIter {
    bus: u8,
    device: u8,
    function: u8,
    offset: u8,
}

impl CapabilityIter {
    fn new(bus: u8, device: u8, function: u8) -> Self {
        let offset = {
            let mut word = unsafe { read(bus, device, function, 0x34) };
            word = *u32_set_bit(u32_set_bit(&mut word, 0, false), 1, false);
            bit_range!(word[0..8]) as u8
        };

        Self {
            bus,
            device,
            function,
            offset,
        }
    }
}

impl Iterator for CapabilityIter {
    type Item = DeviceCapability;

    fn next(&mut self) -> Option<Self::Item> {
        let offset = self.offset;
        let word = unsafe { read(self.bus, self.device, self.function, offset) };
        // FIXME: This should let the user know when it finds an invalid capability.
        self.offset = bit_range!(word[8..16]) as u8;
        Capability::from_raw(bit_range!(word[0..8]) as u8).map(|id| DeviceCapability { id, offset })
    }
}

const fn u32_set_bit(word: &mut u32, bit: usize, value: bool) -> &mut u32 {
    assert!(bit < 32);

    if value {
        *word |= 1 << bit;
    } else {
        *word &= !(1 << bit);
    }

    word
}

const fn u32_set_range(num: u32, start: usize, end: usize, value: u32) -> u32 {
    if start != end {
        let bitmask: u32 = !(!0 << (32 - end) >> (32 - end) >> start << start);
        (num & bitmask) | (value << start)
    } else {
        num
    }
}

const fn u64_set_range(num: u64, start: usize, end: usize, value: u64) -> u64 {
    if start != end {
        let bitmask: u64 = !(!0 << (64 - end) >> (64 - end) >> start << start);
        (num & bitmask) | (value << start)
    } else {
        num
    }
}

const VENDOR_RED_HAT: u16 = 0x1AF4;
const VENDOR_INTEL: u16 = 0x8086;



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_type_works() {
        let a = HeaderType(0b1000_0001);
        assert!(a.multiple_functions());
        assert_eq!(a.get(), 1);

        let b = HeaderType(0b0000_0010);
        assert!(!b.multiple_functions());
        assert_eq!(b.get(), 2);
    }
}
