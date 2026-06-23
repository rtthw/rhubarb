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

/// The policy used to determine how resources are granted to a process.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum AccessPolicy {
    /// The process has access to all resources.
    ///
    /// No checks are performed when it requests access to a resource. The
    /// resource is granted without blocking the process.
    All,
    /// The process has normal access to resources.
    ///
    /// When it requests access to some resource, it will be blocked until
    /// access is granted (or stopped if it is denied).
    #[default]
    Normal,
    // /// The process has no access to resources.
    // ///
    // /// If it requests access to a resource, the process will be stopped.
    // None,
}

/// The execution priority of a process.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum Priority {
    None = 0,
    Normal = 32,
    Idle = 255,
}
