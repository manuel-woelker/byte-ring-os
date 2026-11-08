//! One pinned ring-3 process. Kernel and user execution never access state concurrently.
#[cfg(feature = "userspace-test")]
mod checks;
mod memory;

use super::files::BootFiles;
use byte_ring_drivers::{apic::Apic, pic::Pic, pit};
use byte_ring_hal::port::PortRange;
use byte_ring_kernel_core::{
    clock::Clock,
    elf::Image,
    exception::ExceptionFrame,
    queue::{Pair, Stop},
    user_abi::*,
};
use core::{arch::asm, cell::UnsafeCell};

core::arch::global_asm!(include_str!("userspace/entry.S"), options(att_syntax));
const ELF: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/user/x86_64-unknown-none/release/byte-ring-user-demo"
));
#[repr(C, align(16))]
struct Fx([u8; 512]);
#[unsafe(no_mangle)]
static CLEAN_FX_STATE: Fx = {
    let mut bytes = [0; 512];
    bytes[0] = 0x7f;
    bytes[1] = 3;
    bytes[24] = 0x80;
    bytes[25] = 0x1f;
    Fx(bytes)
};
#[unsafe(no_mangle)]
static mut USER_ROOT: u64 = 0;
unsafe extern "C" {
    fn enter_user(entry: u64, stack: u64, boot: u64, argument: u64);
    fn leave_user() -> !;
    fn user_syscall_entry();
    static USER_KERNEL_STACK_TOP: u8;
}
pub fn kernel_stack_top() -> u64 {
    core::ptr::addr_of!(USER_KERNEL_STACK_TOP) as u64
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Outcome {
    Exited(u64),
    Fault(u64, u64),
    BadReturn,
    Timeout,
}
struct State {
    files: BootFiles,
    pair: Pair,
    image: Image<'static>,
    timer: Apic<'static>,
    clock: Clock,
    deadline: i64,
    active: bool,
    outcome: Outcome,
}
struct Slot(UnsafeCell<Option<State>>);
// SAFETY: one CPU, IF=0 for every Rust access. User interrupts enter only on
// the dedicated stack; no state borrow spans enter_user or leave_user.
unsafe impl Sync for Slot {}
static STATE: Slot = Slot(UnsafeCell::new(None));
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    // SAFETY: private callers obey the non-reentrant foreground ownership rule.
    unsafe {
        f((&mut *STATE.0.get())
            .as_mut()
            .expect("userspace initialized"))
    }
}

/// All registers in stack order, followed by the trusted IRET frame.
#[repr(C)]
struct Context {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rbp: u64,
    rsi: u64,
    rdi: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    rax: u64,
    rip: u64,
    cs: u64,
    flags: u64,
    rsp: u64,
    ss: u64,
}
fn validate(context: &mut Context) {
    let valid = state(|s| s.active && s.image.executable(context.rip))
        && context.cs == USER_CS
        && context.ss == USER_SS
        && (STACK..=STACK_END).contains(&context.rsp);
    if !valid {
        terminate(Outcome::BadReturn);
    }
    // Preserve arithmetic/DF state across returning interrupts, while excluding
    // IOPL, NT, VM, debug and alignment controls from a privileged IRET path.
    context.flags = (context.flags
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
    state(|s| {
        s.timer.cancel();
        s.active = false;
        s.outcome = outcome;
    });
    // SAFETY: an active process owns a saved boot stack/CR3/FP context. This
    // abandons the entry stack only after all Rust borrows above have ended.
    unsafe { leave_user() }
}
#[unsafe(no_mangle)]
extern "C" fn user_syscall_dispatch(context: &mut Context) {
    #[cfg(feature = "userspace-test")]
    checks::entry_state();
    if context.rax == 1 {
        terminate(Outcome::Exited(context.rdi));
    }
    validate(context);
    context.flags = 0x202;
    let (completed, stop) = if context.rax == 0 {
        state(|s| s.pair.enter(&mut memory::Shared, &mut s.files, context.rdi))
    } else {
        (0, Stop::BadArgument)
    };
    context.rax = completed;
    context.rdx = stop as u64;
}
#[unsafe(no_mangle)]
extern "C" fn user_timer_dispatch(context: &mut Context) {
    state(|s| s.timer.finish_timer());
    validate(context);
    let expired = state(|s| {
        let remaining = s.deadline - s.clock.milliseconds(super::clock::counter());
        if remaining <= 0 {
            true
        } else {
            // A canceled previous instance's timer can still be pending in IRR.
            s.timer.arm(remaining as u64).expect("calibrated watchdog");
            false
        }
    });
    if expired {
        terminate(Outcome::Timeout);
    }
}
#[unsafe(no_mangle)]
extern "C" fn user_spurious_dispatch(context: &mut Context) {
    validate(context);
}

pub fn fault(frame: &ExceptionFrame) -> ! {
    state(|s| {
        super::files::log(
            &mut s.files,
            format_args!(
                "userspace fault vector={} error={:#x} rip={:#x} cr2={:#x}\r\n",
                frame.vector, frame.error_code, frame.instruction_pointer, frame.fault_address
            ),
        )
    });
    terminate(Outcome::Fault(frame.vector, frame.error_code));
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
fn execute(argument: u64) -> Outcome {
    let (entry, root) = state(|s| {
        assert!(!s.active);
        s.pair = Pair::default();
        s.outcome = Outcome::Exited(u64::MAX);
        let root = memory::prepare(&s.image);
        s.deadline = s.clock.milliseconds(super::clock::counter()) + 1000;
        s.timer.arm(1000).expect("calibrated watchdog");
        s.active = true;
        (s.image.entry(), root)
    });
    // SAFETY: private root has only validated user pages, trusted kernel maps,
    // and static backing. No state/memory borrow spans execution. Entry and
    // stacks/selectors are fixed and the watchdog bounds non-cooperative code.
    unsafe {
        USER_ROOT = root;
        enter_user(entry, STACK_END - 8, BOOT, argument);
    }
    state(|s| s.outcome)
}

pub fn run(files: BootFiles, scenario: Option<u64>) -> ! {
    initialize_cpu();
    let clock = super::clock::initialize();
    // SAFETY: PIC, ELCR and PIT0 are exclusively boot-owned here with IF=0.
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
    // SAFETY: initialize once on boot CPU, IF=0, before any ring-3 entry.
    unsafe {
        *STATE.0.get() = Some(State {
            files,
            pair: Pair::default(),
            image,
            timer,
            clock,
            deadline: 0,
            active: false,
            outcome: Outcome::Exited(0),
        });
    }
    #[cfg(feature = "userspace-test")]
    checks::run(scenario);
    #[cfg(not(feature = "userspace-test"))]
    {
        let argument = scenario.unwrap_or(0);
        let result = execute(argument);
        state(|s| {
            super::files::log(
                &mut s.files,
                format_args!("userspace scenario={argument}: {result:?}\r\n"),
            )
        });
        super::halt()
    }
}
#[cfg(feature = "userspace-test")]
pub fn test_failure() -> ! {
    checks::finish(false)
}
