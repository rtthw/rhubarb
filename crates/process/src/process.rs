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

pub fn mmap(page_count: u64) -> Result<u64, MemoryMapError> {
    let mut num: i64 = 3;
    unsafe {
        core::arch::asm!(
            "int 0x42",
            inout("rax") num,
            in("rdi") page_count,
            options(nostack),
        );
    }

    if num < 0 {
        Err(MemoryMapError::InvalidPageCount)
    } else {
        Ok(num as u64)
    }
}

#[derive(Debug)]
pub enum MemoryMapError {
    InvalidPageCount,
}
