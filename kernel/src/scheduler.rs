//! # Scheduler
//!
//! A preemptive, multitasking process scheduler.

use {
    crate::{
        KERNEL_STACK, KERNEL_STACK_SIZE, gdt,
        loader::global_loader,
        memory::{AddressSpace, KernelMapping, kernel_address_space},
    },
    alloc::{
        collections::{btree_map::BTreeMap, vec_deque::VecDeque},
        string::String,
    },
    core::{
        arch::asm,
        fmt,
        sync::atomic::{AtomicU64, Ordering},
    },
    log::{debug, info, warn},
    memory_types::{AddressDomain, PAGE_SIZE, PageRange, PageTableFlags, Address},
    spin_mutex::Mutex,
    x86_64::{
        instructions::interrupts::without_interrupts, registers::rflags::RFlags,
        structures::idt::InterruptStackFrameValue,
    },
};


const IDLE_PROCESS_ID: u64 = 0;

pub const DEFAULT_KERNEL_STACK_SIZE: usize = PAGE_SIZE * 8;
pub const DEFAULT_USER_STACK_SIZE: usize = PAGE_SIZE * 16;

static PROCESS_ID: AtomicU64 = AtomicU64::new(IDLE_PROCESS_ID + 1);

/// Start preemptive multitasking.
pub fn run() -> ! {
    SCHEDULER.lock().init();
    schedule()
}

/// Define an interrupt handler that makes use of the [`ExecutionContext`] at
/// the time of interruption.
#[macro_export]
macro_rules! define_interrupt_handler_with_context {
    (| $name:ident | $body:block) => {
        #[unsafe(naked)]
        pub extern "x86-interrupt" fn $name(
            frame: ::x86_64::structures::idt::InterruptStackFrame,
        ) {
            use $crate::scheduler::{schedule, with_scheduler, ExecutionContext};

            ::core::arch::naked_asm!(
                // Assemble an `ExecutionContext` into `rdi` (first function argument).
                "mov    [rsp - 120], r15",
                "mov    [rsp - 112], r14",
                "mov    [rsp - 104], r13",
                "mov    [rsp - 96], r12",
                "mov    [rsp - 88], r11",
                "mov    [rsp - 80], r10",
                "mov    [rsp - 72], r9",
                "mov    [rsp - 64], r8",
                "mov    [rsp - 56], rsi",
                "mov    [rsp - 48], rdi",
                "mov    [rsp - 40], rbp",
                "mov    [rsp - 32], rdx",
                "mov    [rsp - 24], rcx",
                "mov    [rsp - 16], rbx",
                "mov    [rsp - 8],  rax",

                "lea    rdi, [rsp - 120]",
                "sub    rsp, 120",

                // Now that we've properly assembled the context, we can jump to `__handler`.
                "call   {}",
                sym __handler,
            );

            extern "C" fn __handler(context: ExecutionContext) -> ! {
                // Interrupts should not be enabled at this point, but it can't hurt to make sure.
                assert!(!::x86_64::instructions::interrupts::are_enabled());

                // Make sure the current context is the one that was executing before this
                // interrupt started.
                with_scheduler(|scheduler| scheduler.set_current_context(context));

                // Run whatever the `body` needs to run.
                $body

                // Schedule the next process to run.
                schedule()
            }
        }
    };
}

/// The global [`Scheduler`] instance.
static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

/// The kernel's process scheduler.
pub struct Scheduler {
    current: Option<Process>,
    queue: BTreeMap<Priority, VecDeque<Process>>,
}

impl Scheduler {
    const fn new() -> Self {
        Self {
            current: None,
            queue: BTreeMap::new(),
        }
    }

    fn init(&mut self) {
        fn __idle_loop() -> ! {
            loop {
                x86_64::instructions::hlt();
            }
        }

        self.add_to_queue(Process {
            id: IDLE_PROCESS_ID,
            name: "idle".into(),
            address_space: AddressSpace::new("idle", None),
            access_policy: AccessPolicy::All,
            priority: Priority::Idle,
            context: Some(ExecutionContext {
                registers: CpuRegisters::EMPTY,
                frame: InterruptStackFrameValue::new(
                    x86_64::VirtAddr::from_ptr(__idle_loop as *const fn() -> !),
                    gdt::selectors().kernel_code,
                    RFlags::INTERRUPT_FLAG,
                    x86_64::VirtAddr::from_ptr(unsafe {
                        KERNEL_STACK.as_ptr().add(KERNEL_STACK_SIZE)
                    }),
                    gdt::selectors().kernel_code,
                ),
            }),
            heap_mapping: None,
            allow_io: true,
        });
    }

    /// Get the currently running [`Process`].
    pub fn current_process(&self) -> Option<&Process> {
        self.current.as_ref()
    }

    /// Get a mutable reference the currently running [`Process`].
    pub fn current_process_mut(&mut self) -> Option<&mut Process> {
        self.current.as_mut()
    }

    fn add_to_queue(&mut self, process: Process) {
        self.queue
            .entry(process.priority)
            .or_default()
            .push_back(process);
    }

    fn next_ready(&mut self) -> Process {
        self.queue
            .values_mut()
            .find(|q| !q.is_empty())
            .expect("should at least have an idle process available")
            .pop_front()
            .unwrap()
    }

    fn schedule_next(&mut self) -> ExecutionContext {
        if self.current.is_none() {
            let process = self.next_ready();
            process.address_space.enter();
            crate::gdt::set_user_io_allowed(process.allow_io);
            self.current = Some(process);
        }

        self.current
            .as_mut()
            .expect("current process should exist")
            .context
            .take()
            .expect("current process should have a context")
    }

    /// Set the current process's [`ExecutionContext`].
    pub fn set_current_context(&mut self, context: ExecutionContext) {
        let prev_context = self
            .current
            .as_mut()
            .expect("a process should be running at this point")
            .context
            .replace(context);

        assert!(prev_context.is_none());
    }

    /// Preempt the currently running process, and place it at the end of the
    /// run queue.
    pub fn preempt_current(&mut self) {
        if self.queue.is_empty() {
            warn!("Attempted preemption with an empty ready queue");
        } else {
            let process = self
                .current
                .take()
                .expect("current process should be available for preemption");
            self.add_to_queue(process);
        }
    }

    /// Add a kernel process with the given parameters to the run queue.
    ///
    /// ## Arguments
    ///
    /// - `name`, the name of the process to be run.
    /// - `entry_point`, a pointer to the entry point of the process. The
    ///   function must be diverging.
    /// - `stack_size`, a size for the new process's stack. If `None` is
    ///   provided, the [`DEFAULT_KERNEL_STACK_SIZE`] will be used.
    #[allow(unused)]
    pub fn run_kernel_process(
        &mut self,
        name: impl Into<String>,
        entry_point: *const fn() -> !,
        stack_size: Option<usize>,
    ) {
        let id = PROCESS_ID.fetch_add(1, Ordering::SeqCst);
        let name = name.into();

        // This is a kernel process running kernel code, so just inherit the kernel's
        // address space.
        let address_space = AddressSpace::new(format!("{name}.{id}"), Some(kernel_address_space()));

        let stack_size = stack_size.unwrap_or(DEFAULT_KERNEL_STACK_SIZE);
        let stack_top_addr = AddressDomain::UserCode.base_addr();
        address_space.map_pages(
            format!("kernel_stack.{id}"),
            PageRange::from_end_size(stack_top_addr.page(), stack_size),
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
        );

        let context = ExecutionContext {
            registers: CpuRegisters::EMPTY,
            frame: InterruptStackFrameValue::new(
                x86_64::VirtAddr::from_ptr(entry_point),
                gdt::selectors().kernel_code,
                RFlags::INTERRUPT_FLAG,
                x86_64::VirtAddr::new(stack_top_addr.to_raw() as u64),
                gdt::selectors().kernel_data,
            ),
        };

        let process = Process {
            id,
            name,
            access_policy: AccessPolicy::All,
            priority: Priority::Normal,
            address_space,
            context: Some(context),
            heap_mapping: None,
            allow_io: true,
        };

        info!("Running {process}");

        self.add_to_queue(process);
    }

    /// Add a user process with the given parameters to the run queue.
    ///
    /// ## Arguments
    ///
    /// - `name`, the name of the process to be run.
    /// - `stack_size`, a size for the new process's stack. If `None` is
    ///   provided, the [`DEFAULT_USER_STACK_SIZE`] will be used.
    /// - `allow_io`, whether the new process will be allowed to perform I/O
    ///   instructions.
    pub fn run_user_process(
        &mut self,
        name: impl Into<String>,
        stack_size: Option<usize>,
        allow_io: bool,
        access_policy: AccessPolicy,
    ) {
        let id = PROCESS_ID.fetch_add(1, Ordering::SeqCst);
        let name = name.into();

        let address_space = AddressSpace::new(format!("{name}.{id}"), None);
        let user_code_addr = AddressDomain::UserCode.base_addr();

        let _object = global_loader()
            .load_object(&name, &address_space, user_code_addr.page())
            .unwrap();

        let entry_point_section = global_loader()
            .get_text_section(&name, "main")
            .unwrap()
            .upgrade()
            .unwrap();
        let entry_point = user_code_addr + entry_point_section.mapping_offset;

        let stack_size = stack_size.unwrap_or(DEFAULT_USER_STACK_SIZE);
        let stack_top_addr = AddressDomain::UserCode.base_addr();
        address_space.map_pages(
            format!("user_stack.{id}"),
            PageRange::from_end_size(stack_top_addr.page(), stack_size),
            PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE
                | PageTableFlags::NO_EXECUTE,
        );

        let context = ExecutionContext {
            registers: CpuRegisters::EMPTY,
            frame: InterruptStackFrameValue::new(
                x86_64::VirtAddr::new(entry_point.to_raw() as u64),
                gdt::selectors().user_code,
                RFlags::INTERRUPT_FLAG,
                x86_64::VirtAddr::new(stack_top_addr.to_raw() as u64),
                gdt::selectors().user_data,
            ),
        };

        let process = Process {
            id,
            name,
            access_policy,
            priority: Priority::Normal,
            address_space,
            context: Some(context),
            heap_mapping: None,
            allow_io,
        };

        info!("Running {process}");

        self.add_to_queue(process);
    }
}

/// Perform some operation on the [`Scheduler`] with interrupts disabled.
pub fn with_scheduler<F, R>(op: F) -> R
where
    F: FnOnce(&mut Scheduler) -> R,
{
    without_interrupts(|| {
        let mut scheduler = SCHEDULER.lock();
        op(&mut *scheduler)
    })
}

/// Schedule the next process to run.
pub fn schedule() -> ! {
    let next_context = with_scheduler(Scheduler::schedule_next);
    unsafe {
        asm!(
            "mov    rsp, {}",
            "add    rsp, 120",
            "mov    r15, [rsp - 120]",
            "mov    r14, [rsp - 112]",
            "mov    r13, [rsp - 104]",
            "mov    r12, [rsp - 96]",
            "mov    r11, [rsp - 88]",
            "mov    r10, [rsp - 80]",
            "mov    r9,  [rsp - 72]",
            "mov    r8,  [rsp - 64]",
            "mov    rsi, [rsp - 56]",
            "mov    rdi, [rsp - 48]",
            "mov    rbp, [rsp - 40]",
            "mov    rdx, [rsp - 32]",
            "mov    rcx, [rsp - 24]",
            "mov    rbx, [rsp - 16]",
            "mov    rax, [rsp - 8]",

            "iretq",

            in(reg) &next_context,
            options(noreturn),
        )
    }
}

pub const DEFER_INTERRUPT_NUMBER: u8 = 0x40; // TODO: Choose a less arbitrary number.
pub const EXIT_INTERRUPT_NUMBER: u8 = 0x41;
pub const TRANSLATE_ADDR_INTERRUPT_NUMBER: u8 = 0x42;

define_interrupt_handler_with_context!(|defer_interrupt_handler| {
    with_scheduler(|scheduler| scheduler.preempt_current());
});

define_interrupt_handler_with_context!(|exit_interrupt_handler| {
    // Exiting the current process is as simple as dropping it. The process
    // will no longer exist within the run queue, and its allocated frames will
    // be deallocated when the address space is dropped.

    let Some(mut process) = with_scheduler(|scheduler| scheduler.current.take()) else {
        unreachable!()
    };
    let context = process
        .context
        .take()
        .expect("process should have a context on exit");
    let exit_code = context.registers.rdi as i64;
    kernel_address_space().enter();
    info!("Exiting {process} with code: {}", exit_code);
    debug!("CONTEXT: {context:x?}");

    // Immediately after this block is finished, `Scheduler::schedule_next`
    // is called to set the next execution context.
});

define_interrupt_handler_with_context!(|translate_addr_interrupt_handler| {
    with_scheduler(|scheduler| {
        let Some(current) = &mut scheduler.current else {
            unreachable!();
        };
        let Some(context) = &mut current.context else {
            unreachable!();
        };

        // TODO: Check permissions here.

        let virt_addr = Address::new(context.registers.rdi as usize);
        if let Some(phys_addr) = current.address_space.translate_address(virt_addr) {
            // log::trace!(
            //     "Translating {virt_addr:x} >> {phys_addr:x} for `{}`",
            //     current.name,
            // );
            context.registers.rax = phys_addr.to_raw() as u64;
        } else {
            context.registers.rax = (-2_i64).cast_unsigned();
        }
    });
});

/// Exit the current process.
pub fn exit(code: i64) {
    unsafe {
        core::arch::asm!("int 0x41", in("rdi") code);
    }
}

/// The state of a running process.
#[derive(Debug)]
pub struct Process {
    /// The ID of the process.
    id: u64,
    /// The name of the process. For user processes, this is the basename of the
    /// process's object file (e.g. "example" for "/example.o").
    pub name: String,
    /// The [`AccessPolicy`] of the process.
    pub access_policy: AccessPolicy,
    /// The run [`Priority`] of the process.
    priority: Priority,
    /// The [`AddressSpace`] of the process. That is, everything the process can
    /// "see" in virtual memory.
    pub address_space: AddressSpace,
    /// The [`ExecutionContext`] of the process. If this is `None`, the process
    /// is currently running.
    context: Option<ExecutionContext>,
    /// The [`KernelMapping`] corresponding to the process's heap.
    pub heap_mapping: Option<KernelMapping>,
    /// Whether the process is allowed to perform I/O instructions.
    allow_io: bool,
}

impl fmt::Display for Process {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("Process #{} '{}'", self.id, self.name))
    }
}

/// The policy used to determine how resources are granted to a process.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
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

/// The execution priority of a [`Process`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum Priority {
    Normal = 32,
    Idle = 255,
}

/// All information necessary for preempting/resuming a [`Process`].
#[derive(Debug)]
#[repr(C)]
pub struct ExecutionContext {
    registers: CpuRegisters,
    frame: InterruptStackFrameValue,
}

#[derive(Clone, Debug)]
#[repr(C)]
pub struct CpuRegisters {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    rax: u64,
}

impl CpuRegisters {
    const EMPTY: Self = Self {
        r15: 0,
        r14: 0,
        r13: 0,
        r12: 0,
        r11: 0,
        r10: 0,
        r9: 0,
        r8: 0,
        rsi: 0,
        rdi: 0,
        rbp: 0,
        rdx: 0,
        rcx: 0,
        rbx: 0,
        rax: 0,
    };
}
