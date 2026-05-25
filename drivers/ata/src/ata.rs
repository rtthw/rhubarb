//! # Advanced Technology Attachment (ATA)

#![no_std]

extern crate alloc;

use {
    alloc::{string::String, vec::Vec},
    core::{fmt, time::Duration},
    log::{error, warn},
    spin_mutex::Mutex,
};

const SECTOR_SIZE: usize = 512;

const DATA_REGISTER_OFFSET: u16 = 0;
const _ERROR_REGISTER_OFFSET: u16 = 1;
const SECTOR_COUNT_REGISTER_OFFSET: u16 = 2;
const LBA_LOW_REGISTER_OFFSET: u16 = 3;
const LBA_MID_REGISTER_OFFSET: u16 = 4;
const LBA_HIGH_REGISTER_OFFSET: u16 = 5;
const DRIVE_REGISTER_OFFSET: u16 = 6;
const STATUS_REGISTER_OFFSET: u16 = 7;
const COMMAND_REGISTER_OFFSET: u16 = 7;
const ALT_STATUS_REGISTER_OFFSET: u16 = 0;

const FLOATING_BUS_STATUS: u8 = 0xFF;

const READ_COMMAND: u16 = 0x20;
const _WRITE_COMMAND: u16 = 0x30;
const IDENTIFY_COMMAND: u16 = 0xEC;

pub static BUSES: Mutex<[Bus; 2]> =
    Mutex::new([Bus::new(0, 0x1F0, 0x3F6), Bus::new(1, 0x170, 0x376)]);



pub fn enumerate_drives() -> Vec<Drive> {
    let mut drives = Vec::new();
    for bus in 0..2 {
        for disk in 0..2 {
            if let Some(drive) = Drive::open(bus, disk) {
                drives.push(drive)
            }
        }
    }

    drives
}




#[derive(Clone, Debug)]
pub struct Bus {
    id: u8,
    selected_drive: Option<u8>,

    // https://wiki.osdev.org/ATA_PIO#Registers
    io_port_base: u16,
    control_port_base: u16,
}

impl Bus {
    const fn new(id: u8, io_port_base: u16, control_port_base: u16) -> Self {
        Self {
            id,
            selected_drive: None,
            io_port_base,
            control_port_base,
        }
    }

    fn status(&mut self) -> Status {
        // The ALT_STATUS register is just a copy of the STATUS register that doesn't
        // affect interrupts.
        Status(unsafe { x86_port::read_u8(self.control_port_base + ALT_STATUS_REGISTER_OFFSET) })
    }

    fn read_data(&mut self) -> u16 {
        unsafe { x86_port::read_u16(self.io_port_base + DATA_REGISTER_OFFSET) }
    }

    fn read(&mut self, drive: u8, block: u32, buf: &mut [u8]) -> Result<(), &'static str> {
        debug_assert!(buf.len() == SECTOR_SIZE);

        self.select_drive(drive)?;
        self.write_command(drive, block, READ_COMMAND)?;

        for chunk in buf.chunks_mut(2) {
            let data = self.read_data().to_le_bytes();
            chunk.clone_from_slice(&data);
        }

        if self.status().error() {
            error!("Failed to read ATA bus {}", self.id);

            Err("failed to read bus")
        } else {
            Ok(())
        }
    }

    fn poll(&mut self, status_check: fn(Status) -> bool) -> Result<(), &'static str> {
        let start = time::now();
        while !status_check(self.status()) {
            if time::now().duration_since(start) > Duration::from_secs(1) {
                warn!("ATA bus {} hangup while polling status", self.id);

                return Err("poll hangup");
            }

            core::hint::spin_loop();
        }

        Ok(())
    }

    fn select_drive(&mut self, drive: u8) -> Result<(), &'static str> {
        self.poll(|status| !status.busy())?;
        self.poll(|status| !status.data_request())?;

        if self.selected_drive.is_some_and(|d| d == drive) {
            return Ok(());
        } else {
            self.selected_drive = Some(drive);
        }

        unsafe {
            x86_port::write_u8(
                self.io_port_base + DRIVE_REGISTER_OFFSET,
                0b10100000 | (drive << 4),
            );
        }

        let start = time::now();
        while time::now().duration_since(start) < Duration::from_nanos(400) {
            core::hint::spin_loop();
        }

        self.poll(|status| !status.busy())?;
        self.poll(|status| !status.data_request())?;

        Ok(())
    }

    fn write_command(&mut self, drive: u8, block: u32, command: u16) -> Result<(), &'static str> {
        let block_bytes = block.to_le_bytes();
        unsafe {
            x86_port::write_u8(self.io_port_base + SECTOR_COUNT_REGISTER_OFFSET, 1);
            x86_port::write_u8(self.io_port_base + LBA_LOW_REGISTER_OFFSET, block_bytes[0]);
            x86_port::write_u8(self.io_port_base + LBA_MID_REGISTER_OFFSET, block_bytes[1]);
            x86_port::write_u8(self.io_port_base + LBA_HIGH_REGISTER_OFFSET, block_bytes[2]);
            x86_port::write_u8(
                self.io_port_base + DRIVE_REGISTER_OFFSET,
                block_bytes[3] | (0b11100000 | (drive << 4)),
            );
        }

        unsafe {
            x86_port::write_u8(self.io_port_base + COMMAND_REGISTER_OFFSET, command as u8);
        }

        let start = time::now();
        while time::now().duration_since(start) < Duration::from_nanos(400) {
            core::hint::spin_loop();
        }

        _ = self.status();
        unsafe {
            _ = x86_port::read_u8(self.io_port_base + STATUS_REGISTER_OFFSET);
        }

        // https://wiki.osdev.org/ATA_PIO#IDENTIFY_command
        if self.status().0 == 0 {
            return Err("drive does not exist");
        }

        if self.status().error() {
            return Err("failed to write command");
        }

        self.poll(|status| !status.busy())?;
        self.poll(|status| status.data_request())?;

        Ok(())
    }

    // https://wiki.osdev.org/ATA_PIO#IDENTIFY_command
    fn identify_drive(&mut self, drive: u8) -> Result<IdentifyResponse, &'static str> {
        // https://wiki.osdev.org/ATA_PIO#Floating_Bus
        if self.status().0 == FLOATING_BUS_STATUS {
            return Ok(IdentifyResponse::None);
        }

        self.select_drive(drive)?;

        // https://wiki.osdev.org/ATA_PIO#%22Command_Aborted%22
        if self.write_command(drive, 0, IDENTIFY_COMMAND).is_err() {
            return Ok(IdentifyResponse::None);
        }

        // https://wiki.osdev.org/ATA_PIO#Detecting_device_types
        match unsafe {
            (
                x86_port::read_u8(self.io_port_base + LBA_MID_REGISTER_OFFSET),
                x86_port::read_u8(self.io_port_base + LBA_HIGH_REGISTER_OFFSET),
            )
        } {
            (0x00, 0x00) => Ok(IdentifyResponse::Pata([(); 256].map(|_| self.read_data()))),
            (0x14, 0xEB) => Ok(IdentifyResponse::PataPi),
            (0x3C, 0xC3) => Ok(IdentifyResponse::Sata),

            (_, _) => Err("unknown device type"),
        }
    }
}



#[derive(Clone, Debug)]
pub struct Drive {
    pub id: u8,
    pub bus: u8,
    model: String,
    serial: String,
    sector_count: u32,
}

impl Drive {
    pub fn open(bus: u8, drive: u8) -> Option<Self> {
        let response = BUSES.lock()[bus as usize].identify_drive(drive);
        match response {
            Ok(IdentifyResponse::Pata(buf)) => {
                let mut serial = String::new();
                let mut model = String::new();

                for word in 10..20 {
                    let value = buf[word];
                    let ch_1 = (value >> 8) as u8 as char;
                    if ch_1 != '\0' {
                        serial.push(ch_1);
                    }
                    let ch_2 = (value as u8) as char;
                    if ch_2 != '\0' {
                        serial.push(ch_2);
                    }
                }
                for word in 27..47 {
                    let value = buf[word];
                    let ch_1 = (value >> 8) as u8 as char;
                    if ch_1 != '\0' {
                        model.push(ch_1);
                    }
                    let ch_2 = (value as u8) as char;
                    if ch_2 != '\0' {
                        model.push(ch_2);
                    }
                }

                let mut sector_count = (buf[100] as u64)
                    | ((buf[101] as u64) << 16)
                    | ((buf[102] as u64) << 32)
                    | ((buf[103] as u64) << 48);

                let _lba_bit_count = if sector_count == 0 {
                    sector_count = (buf[60] as u64) | ((buf[61] as u64) << 16);
                    28
                } else {
                    48
                };

                // TODO: Support different LBA modes.

                Some(Self {
                    id: drive,
                    bus,
                    model: model.trim().into(),
                    serial: serial.trim().into(),
                    sector_count: sector_count as u32,
                })
            }
            Ok(IdentifyResponse::Sata) => {
                warn!("SATA drives are not yet supported");
                None
            }
            Ok(IdentifyResponse::PataPi) => {
                warn!("ATA-PI drives are not yet supported");
                None
            }
            Ok(IdentifyResponse::None) => None,

            Err(error) => {
                warn!("failed to identify ATA {bus:x}:{drive:x}: {error}");
                None
            }
        }
    }

    pub fn sector_count(&self) -> usize {
        self.sector_count as usize
    }
}

impl io::BlockSize for Drive {
    #[inline(always)]
    fn block_size(&self) -> usize {
        SECTOR_SIZE
    }
}

impl io::BlockReader for Drive {
    fn read_blocks(&mut self, offset: usize, buffer: &mut [u8]) -> Result<usize, io::ReadError> {
        let length = buffer.len();
        if length % SECTOR_SIZE != 0 {
            return Err(io::ReadError::InvalidBufferLength { length });
        }
        let block_count = length / SECTOR_SIZE;
        let mut buses = BUSES.lock();
        let bus = &mut buses[self.bus as usize];
        for offset_addend in 0..block_count {
            let block_start = offset_addend * SECTOR_SIZE;
            let block_end = block_start + SECTOR_SIZE;
            bus.read(
                self.id,
                (offset + offset_addend) as u32,
                &mut buffer[block_start..block_end],
            )?;
        }

        Ok(length)
    }
}

impl fmt::Display for Drive {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} {} ({} blocks)",
            self.model, self.serial, self.sector_count,
        )
    }
}



enum IdentifyResponse {
    Pata([u16; 256]),
    PataPi,
    Sata,
    None,
}

bit_utils::bit_field! {
    pub struct Status: u8 {
        pub error: bool = 0,
        pub _index: bool = 1,
        pub _corrected: bool = 2,
        pub data_request: bool = 3,
        pub service_request: bool = 4,
        pub drive_fault: bool = 5,
        pub ready: bool = 6,
        pub busy: bool = 7,
    }
}
