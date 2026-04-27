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
