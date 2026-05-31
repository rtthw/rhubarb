//! # Universal Asynchronous Receiver-Transmitter (UART)

#![no_std]

use core::fmt::Write;


pub struct Device {
    io_port: u16,
}

pub const COM1_PORT: u16 = 0x3F8;
pub const COM2_PORT: u16 = 0x2F8;

const INTERRUPT_ENABLE_REGISTER: u16 = 1;
const FIFO_CONTROL_REGISTER: u16 = 2;
const LINE_CONTROL_REGISTER: u16 = 3;
const MODEM_CONTROL_REGISTER: u16 = 4;
const LINE_STATUS_REGISTER: u16 = 5;
const _MODEM_STATUS_REGISTER: u16 = 6;

const LINE_STATUS_OUTPUT_EMPTY: u8 = 1 << 5;

impl Device {
    pub const COM1: Self = Self::new(COM1_PORT);
    pub const COM2: Self = Self::new(COM2_PORT);

    pub const fn new(io_port: u16) -> Self {
        Self { io_port }
    }

    // https://wiki.osdev.org/Serial_Ports#Programming_the_Serial_Communications_Port
    pub unsafe fn init(&mut self) {
        const DLAB: u8 = 1 << 7;
        const WORD_LEN_8BIT: u8 = 0b_0000_0011;
        const DIVISOR_LSB: u8 = 3;
        const DIVISOR_MSB: u8 = 0;
        const FIFO_ENABLE: u8 = 0b_0000_0001;
        const FIFO_CLEAR_SEND: u8 = 0b_0000_0100;
        const FIFO_CLEAR_RECV: u8 = 0b_0000_0010;
        const FIFO_INT_LEVEL_14: u8 = 0b_1100_0000;
        const DATA_TERMINAL_READY: u8 = 0b_0000_0001;
        const REQUEST_TO_SEND: u8 = 0b_0000_0010;
        const PIN_OUT2: u8 = 0b_0000_1000;

        unsafe {
            let data_port = self.io_port;
            let interrupt_enable = self.io_port + INTERRUPT_ENABLE_REGISTER;
            let fifo_control = self.io_port + FIFO_CONTROL_REGISTER;
            let line_control = self.io_port + LINE_CONTROL_REGISTER;
            let modem_control = self.io_port + MODEM_CONTROL_REGISTER;

            // Disable interrupts.
            x86_port::write_u8(interrupt_enable, 0);

            // Set the divisor latch access bit (DLAB).
            x86_port::write_u8(line_control, DLAB);

            // Set the baud rate. See the OSDev article for more:
            //      https://wiki.osdev.org/Serial_Ports#Baud_Rate
            x86_port::write_u8(data_port, DIVISOR_LSB);
            x86_port::write_u8(interrupt_enable, DIVISOR_MSB);

            // Finish setting the baud rate by clearing the DLAB, and at the same time set
            // the word length to 8 bits. I know `& !DLAB` isn't doing anything, it's easier
            // to read this way.
            x86_port::write_u8(line_control, WORD_LEN_8BIT & !DLAB);

            // Enable FIFO, clear it, and set the interrupt trigger level to 14 bytes.
            x86_port::write_u8(
                fifo_control,
                FIFO_ENABLE | FIFO_CLEAR_SEND | FIFO_CLEAR_RECV | FIFO_INT_LEVEL_14,
            );

            // Set the data terminal ready pin, signal request to send, and enable hardware
            // pin OUT2 (enable IRQ).
            x86_port::write_u8(
                modem_control,
                DATA_TERMINAL_READY | REQUEST_TO_SEND | PIN_OUT2,
            );

            // Enable interrupts.
            x86_port::write_u8(interrupt_enable, 1);
        }
    }

    pub unsafe fn write(&mut self, byte: u8) {
        match byte {
            0x08 /* BS */ | 0x7F /* DEL */ => unsafe {
                self.send(0x08); // Move back 1 character.
                self.send(b' '); // Write a space (also moves forward 1 character).
                self.send(0x08); // Go back to before the space.
            }
            b'\n' => unsafe {
                self.send(b'\r');
                self.send(b'\n');
            }

            other => unsafe {
                self.send(other);
            }
        }
    }

    pub unsafe fn send(&mut self, byte: u8) {
        while !unsafe { self.try_send(byte) } {
            core::hint::spin_loop();
        }
    }

    pub unsafe fn try_send(&mut self, byte: u8) -> bool {
        unsafe {
            if self.line_output_empty() {
                x86_port::write_u8(self.io_port, byte);

                true
            } else {
                false
            }
        }
    }

    pub unsafe fn read_line_status(&self) -> u8 {
        unsafe { x86_port::read_u8(self.io_port + LINE_STATUS_REGISTER) }
    }

    pub unsafe fn line_output_empty(&self) -> bool {
        unsafe { self.read_line_status() & LINE_STATUS_OUTPUT_EMPTY == LINE_STATUS_OUTPUT_EMPTY }
    }
}

impl Write for Device {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.bytes() {
            unsafe {
                self.write(byte);
            }
        }

        Ok(())
    }
}
