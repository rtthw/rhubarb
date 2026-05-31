//! # Serial Port

use {
    core::fmt::Write, log::LogLevel, spin_mutex::Mutex,
    x86_64::instructions::interrupts::without_interrupts,
};


pub static SERIAL1: Mutex<uart_16550::Device> = Mutex::new(uart_16550::Device::COM1);

pub fn init() {
    unsafe { SERIAL1.lock().init() };
    log::set_logger(&SerialLogger).unwrap();
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    without_interrupts(|| {
        SERIAL1
            .lock()
            .write_fmt(args)
            .expect("failed to write to serial port");
    });
}

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! serial_println {
    () => {
        $crate::serial_print!("\n")
    };
    ($fmt:expr) => {
        $crate::serial_print!(concat!($fmt, "\n"))
    };
    ($fmt:expr, $($arg:tt)*) => {
        $crate::serial_print!(concat!($fmt, "\n"), $($arg)*)
    };
}



const ANSI_SGR_RESET: u8 = 0;
const ANSI_SGR_BOLD: u8 = 0;
const ANSI_SGR_DIM: u8 = 2;

const ANSI_SGR_FG_RED: u8 = 31;
const ANSI_SGR_FG_GREEN: u8 = 32;
const ANSI_SGR_FG_YELLOW: u8 = 33;
const ANSI_SGR_FG_BLUE: u8 = 34;

pub struct SerialLogger;

impl log::Log for SerialLogger {
    fn log(
        &self,
        level: LogLevel,
        target: &str,
        _module_path: &'static str,
        _location: &'static core::panic::Location,
        args: core::fmt::Arguments,
    ) {
        let level_color_code = match level {
            LogLevel::Error => ANSI_SGR_FG_RED,
            LogLevel::Warn => ANSI_SGR_FG_YELLOW,
            LogLevel::Info => ANSI_SGR_FG_GREEN,
            LogLevel::Debug => ANSI_SGR_FG_BLUE,
            LogLevel::Trace => ANSI_SGR_DIM,
        };
        let use_bold = level == LogLevel::Error;

        serial_println!(
            "\x1b[{}m{:<6}\x1b[0m\x1b[2m[{}]\x1b[0m \x1b[{}m{}\x1b[0m",
            level_color_code,
            level.as_str(),
            target,
            if use_bold {
                ANSI_SGR_BOLD
            } else {
                ANSI_SGR_RESET
            },
            args,
        );
    }
}
