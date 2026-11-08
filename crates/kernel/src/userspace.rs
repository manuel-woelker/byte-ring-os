//! Fixed process table; switch only at user return boundaries with IF=0.
#[cfg(feature = "userspace-test")]
mod checks;
mod context;
mod memory;

use super::files::BootFiles;
use byte_ring_drivers::{apic::Apic, pic::Pic, pit};
use byte_ring_hal::port::PortRange;
use byte_ring_kernel_core::{
    clock::Clock,
    elf::Image,
    exception::ExceptionFrame,
    file::{FileWriter, WriteError},
    queue::{Pair, Stop},
    scheduler::{CAPACITY, Decision, Scheduler},
    user_abi::*,
};
use context::Saved;
use core::{arch::asm, cell::UnsafeCell};
core::arch::global_asm!(include_str!("userspace/entry.S"), options(att_syntax));
const ELF: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/user/x86_64-unknown-none/release/byte-ring-user-demo"
));
unsafe extern "C" {
    fn enter_user(saved: *const Saved, root: u64, stack: u64);
    fn resume_user(saved: *const Saved, root: u64, stack: u64) -> !;
    fn leave_user() -> !;
    fn user_syscall_entry();
    static mut ACTIVE_KERNEL_STACK: u64;
}
pub fn kernel_stack_top() -> u64 {
    memory::kernel_stack_top(0)
}
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Outcome {
    Exited(u64),
    Fault(u64, u64),
    BadReturn,
    Timeout,
}
struct Process {
    saved: Saved,
    root: u64,
    pair: Pair,
    stdout: bool,
    outcome: Option<Outcome>,
    quanta: u64,
    syscalls: u64,
}
struct State {
    files: BootFiles,
    image: Image<'static>,
    timer: Apic<'static>,
    clock: Clock,
    processes: [Option<Process>; CAPACITY],
    scheduler: Scheduler,
    switches: u64,
}
struct Slot(UnsafeCell<Option<State>>);
// SAFETY: one CPU; every Rust access has IF=0. No state borrow spans assembly
// resume. User code never maps state, saved contexts, or kernel stacks.
unsafe impl Sync for Slot {}
static STATE: Slot = Slot(UnsafeCell::new(None));
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    // SAFETY: private entry/boot callers obey serialized, non-reentrant ownership.
    unsafe {
        f((&mut *STATE.0.get())
            .as_mut()
            .expect("userspace initialized"))
    }
}
struct ProcessFiles<'a> {
    hardware: &'a mut BootFiles,
    stdout: bool,
}
impl FileWriter for ProcessFiles<'_> {
    fn write(&mut self, handle: u64, bytes: &[u8]) -> Result<usize, WriteError> {
        if handle != 0 || !self.stdout {
            return Err(WriteError::Denied);
        }
        self.hardware.write(handle, bytes)
    }
}

/// Publish only trusted, pinned stack addresses. Both roots map kernel storage.
fn activate(s: &mut State, process: usize) -> (*const Saved, u64, u64) {
    let task = s.processes[process].as_ref().expect("runnable process");
    let stack = memory::kernel_stack_top(process);
    // SAFETY: IF=0 on boot CPU; stack is permanently pinned supervisor memory.
    unsafe {
        ACTIVE_KERNEL_STACK = stack;
        super::exceptions::set_user_stack(stack);
    }
    (&task.saved, task.root, stack)
}
fn entry_stack(saved: &Saved) {
    let current = state(|s| s.scheduler.current().expect("entry without a task"));
    assert!(
        memory::on_kernel_stack(current, saved as *const Saved as *const u8),
        "wrong process kernel stack"
    );
}
fn validate(saved: &mut Saved) {
    let context = &mut saved.context;
    let valid = state(|s| s.scheduler.current().is_some() && s.image.executable(context.rip))
        && context.cs == USER_CS
        && context.ss == USER_SS
        && (STACK..=STACK_END).contains(&context.rsp);
    if !valid || !saved.safe_segments() {
        terminate(Outcome::BadReturn);
    }
    // Keep arithmetic/DF state across interrupts; exclude privileged controls.
    let flags = &mut saved.context.flags;
    *flags = (*flags
        & ((1 << 0)
            | (1 << 2)
            | (1 << 4)
            | (1 << 6)
            | (1 << 7)
            | (1 << 10)
            | (1 << 11)
            | (1 << 21)))
        | 0x202;
}
fn terminate(outcome: Outcome) -> ! {
    let next = state(|s| {
        s.timer.cancel();
        let current = s.scheduler.current().expect("active process");
        s.processes[current].as_mut().unwrap().outcome = Some(outcome);
        let now = s.clock.milliseconds(super::clock::counter());
        s.scheduler.retire(now).map(|next| {
            let resume = activate(s, next);
            s.timer
                .arm(s.scheduler.delay(now).unwrap().max(1))
                .expect("calibrated slice timer");
            resume
        })
    });
    // SAFETY: all Rust process borrows have ended; resume copies only a pinned
    // kernel-owned frame. No suspended Rust call/device stack is resumed.
    unsafe {
        if let Some((saved, root, stack)) = next {
            resume_user(saved, root, stack);
        }
        leave_user()
    }
}
fn reschedule(saved: &mut Saved, timer_fired: bool) {
    let (expired, root) = state(|s| {
        let now = s.clock.milliseconds(super::clock::counter());
        match s.scheduler.tick(now) {
            Decision::BudgetExpired(_) => (true, None),
            Decision::Rotate { from, to } => {
                let previous = s.processes[from].as_mut().unwrap();
                previous.saved = *saved;
                previous.quanta += 1;
                memory::publish_quanta(from, previous.quanta);
                if from != to {
                    s.switches += 1;
                }
                *saved = s.processes[to].as_ref().unwrap().saved;
                let (_, root, _) = activate(s, to);
                s.timer
                    .arm(s.scheduler.delay(now).unwrap().max(1))
                    .expect("calibrated slice timer");
                (false, Some(root))
            }
            Decision::Continue => {
                if timer_fired {
                    // Canceled timer deliveries can outlive a previous quantum.
                    s.timer
                        .arm(s.scheduler.delay(now).unwrap().max(1))
                        .expect("calibrated slice timer");
                }
                (false, None)
            }
            Decision::Finished => panic!("entry without a running process"),
        }
    });
    if expired {
        terminate(Outcome::Timeout);
    }
    if let Some(root) = root {
        // SAFETY: selected root retains identical supervisor kernel mappings;
        // this entry stack remains pinned/mapped until IRET finishes popping it.
        unsafe {
            asm!("mov cr3, {}", in(reg) root, options(nostack, preserves_flags));
        }
    }
}
#[unsafe(no_mangle)]
extern "C" fn user_syscall_dispatch(saved: &mut Saved) {
    entry_stack(saved);
    #[cfg(feature = "userspace-test")]
    checks::entry_state();
    if saved.context.rax == 1 {
        terminate(Outcome::Exited(saved.context.rdi));
    }
    validate(saved);
    saved.context.flags = 0x202;
    let (count, stop) = if saved.context.rax == 0 {
        state(|s| {
            let index = s.scheduler.current().unwrap();
            let task = s.processes[index].as_mut().unwrap();
            task.syscalls += 1;
            task.pair.enter(
                &mut memory::Shared::new(index),
                &mut ProcessFiles {
                    hardware: &mut s.files,
                    stdout: task.stdout,
                },
                saved.context.rdi,
            )
        })
    } else {
        (0, Stop::BadArgument)
    };
    saved.context.rax = count;
    saved.context.rdx = stop as u64;
    reschedule(saved, false);
}
#[unsafe(no_mangle)]
extern "C" fn user_timer_dispatch(saved: &mut Saved) {
    #[cfg(feature = "userspace-test")]
    checks::entry_state();
    entry_stack(saved);
    state(|s| s.timer.finish_timer());
    validate(saved);
    reschedule(saved, true);
}
#[unsafe(no_mangle)]
extern "C" fn user_spurious_dispatch(saved: &mut Saved) {
    entry_stack(saved);
    validate(saved);
}
pub fn fault(frame: &ExceptionFrame) -> ! {
    state(|s| {
        let index = s.scheduler.current().unwrap();
        super::files::log(
            &mut s.files,
            format_args!(
                "userspace fault pid={} vector={} error={:#x} rip={:#x} cr2={:#x}\r\n",
                index,
                frame.vector,
                frame.error_code,
                frame.instruction_pointer,
                frame.fault_address
            ),
        );
    });
    terminate(Outcome::Fault(frame.vector, frame.error_code))
}
fn write_msr(register: u32, value: u64) {
    // SAFETY: callers select supported entry MSRs on the sole CPL0 boot CPU.
    unsafe {
        asm!("wrmsr", in("ecx") register, in("eax") value as u32, in("edx") (value >> 32) as u32, options(nostack, preserves_flags));
    }
}
fn initialize_cpu() {
    let features = core::arch::x86_64::__cpuid(0x80000001).edx;
    assert!(
        features & ((1 << 11) | (1 << 20)) == (1 << 11) | (1 << 20),
        "userspace requires SYSCALL and NX"
    );
    let low: u32;
    let high: u32;
    // SAFETY: supported x86_64 EFER and CR0 at CPL0, IF=0.
    unsafe {
        asm!("rdmsr", in("ecx") 0xc0000080u32, out("eax") low, out("edx") high, options(nostack, preserves_flags));
    }
    write_msr(
        0xc0000080,
        (u64::from(high) << 32) | u64::from(low) | 1 | (1 << 11),
    );
    write_msr(0xc0000081, (0x20u64 << 48) | (8 << 32));
    write_msr(0xc0000082, user_syscall_entry as *const () as u64);
    write_msr(0xc0000084, u64::from(u32::MAX));
    unsafe {
        asm!("mov rax, cr0", "or rax, 0x10000", "mov cr0, rax", out("rax") _, options(nostack));
    }
}

fn execute_batch(arguments: &[u64]) -> [Option<Outcome>; CAPACITY] {
    assert!(!arguments.is_empty() && arguments.len() <= CAPACITY);
    let resume = state(|s| {
        assert!(s.scheduler.current().is_none());
        s.processes = [None, None];
        s.switches = 0;
        for (index, &argument) in arguments.iter().enumerate() {
            let root = memory::prepare(index, &s.image);
            s.processes[index] = Some(Process {
                saved: Saved::new(s.image.entry(), argument),
                root,
                pair: Pair::default(),
                stdout: true,
                outcome: None,
                quanta: 0,
                syscalls: 0,
            });
        }
        let now = s.clock.milliseconds(super::clock::counter());
        s.scheduler = Scheduler::new(arguments.len(), now);
        let resume = activate(s, 0);
        s.timer
            .arm(s.scheduler.delay(now).unwrap().max(1))
            .expect("calibrated slice timer");
        resume
    });
    // SAFETY: contexts, roots and kernel stacks are initialized and private;
    // no Rust process/backing borrow spans execution. All-finished restores
    // this sole saved boot context; retired slots cannot resume themselves.
    unsafe {
        enter_user(resume.0, resume.1, resume.2);
    }
    state(|s| {
        core::array::from_fn(|index| s.processes[index].as_ref().and_then(|task| task.outcome))
    })
}
#[cfg(feature = "userspace-test")]
fn execute(argument: u64) -> Outcome {
    execute_batch(&[argument])[0].unwrap()
}

pub fn run(files: BootFiles, scenario: Option<u64>, peer: Option<u64>) -> ! {
    initialize_cpu();
    let clock = super::clock::initialize();
    // SAFETY: PIC/ELCR/PIT0 are boot-owned at CPL0, IF=0. NIC is absent here.
    unsafe {
        let _pic = Pic::new(
            PortRange::new(0x20, 2),
            PortRange::new(0xa0, 2),
            PortRange::new(0x4d0, 2),
        );
        pit::stop_channel0(PortRange::new(0x40, 4));
    }
    let timer = super::apic::initialize();
    let image = Image::parse(ELF).unwrap_or_else(|error| panic!("user ELF: {error:?}"));
    // SAFETY: first initialization, before ring 3, with IF=0 on the sole CPU.
    unsafe {
        *STATE.0.get() = Some(State {
            files,
            image,
            timer,
            clock,
            processes: [None, None],
            scheduler: Scheduler::new(0, 0),
            switches: 0,
        });
    }
    #[cfg(feature = "userspace-test")]
    checks::run(scenario, peer);
    #[cfg(not(feature = "userspace-test"))]
    {
        let arguments = [scenario.unwrap_or(0), peer.unwrap_or(0)];
        let count = if peer.is_some() { 2 } else { 1 };
        let results = execute_batch(&arguments[..count]);
        state(|s| {
            for (index, result) in results[..count].iter().enumerate() {
                super::files::log(
                    &mut s.files,
                    format_args!(
                        "userspace pid={} scenario={}: {:?}\r\n",
                        index,
                        arguments[index],
                        result.unwrap()
                    ),
                );
            }
            super::files::log(
                &mut s.files,
                format_args!("scheduler: timer switches={}\r\n", s.switches),
            );
        });
        super::halt()
    }
}
#[cfg(feature = "userspace-test")]
pub fn test_failure() -> ! {
    checks::finish(false)
}
