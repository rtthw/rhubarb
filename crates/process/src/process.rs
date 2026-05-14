//! # Process Management

#![no_std]



/// Defer execution to the next process in the scheduler's run queue.
///
/// Execution will resume when this process is next scheduled.
pub fn defer() {
    unsafe {
        core::arch::asm!("int 0x40");
    }
}

/// Exit the current process.
pub fn exit(code: i64) -> ! {
    unsafe {
        core::arch::asm!(
            "int 0x41",
            in("rdi") code,
            options(noreturn),
        );
    }
}

/// Translate the given virtual address to its physical counterpart.
pub fn translate_address(addr: usize) -> Result<usize, TranslateAddressError> {
    let mut num: isize = 0;
    unsafe {
        core::arch::asm!(
            "int 0x42",
            inout("rax") num,
            in("rdi") addr,
            options(nostack),
        );
    }

    if num < 0 {
        Err(match num {
            -1 => TranslateAddressError::PermissionDenied,
            -2 => TranslateAddressError::AddressNotMapped,
            _ => unreachable!(),
        })
    } else {
        Ok(num.cast_unsigned())
    }
}

#[derive(Debug)]
pub enum TranslateAddressError {
    PermissionDenied,
    AddressNotMapped,
}

impl core::fmt::Display for TranslateAddressError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                TranslateAddressError::PermissionDenied => "permission denied",
                TranslateAddressError::AddressNotMapped => "address is not mapped",
            },
        )
    }
}
