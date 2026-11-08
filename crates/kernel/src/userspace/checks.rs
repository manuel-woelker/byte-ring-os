//! Native process isolation, resumable preemption, and peer recovery tests.
use super::*;
use byte_ring_hal::port::PortRange;
pub(super) fn finish(success: bool) -> ! {
    // SAFETY: this QEMU image exclusively owns isa-debug-exit.
    let mut port = unsafe { PortRange::new(0xf4, 1) };
    port.write_u8(0, if success { 0x10 } else { 0x11 });
    crate::halt()
}
fn expected(argument: u64) -> Outcome {
    match argument {
        1 => Outcome::Fault(14, 5),
        2 | 3 | 10 | 14 | 15 => Outcome::Fault(14, 7),
        4 => Outcome::Fault(14, 4),
        5 => Outcome::Fault(13, 0),
        6 => Outcome::Timeout,
        9 | 16 => Outcome::BadReturn,
        11 => Outcome::Fault(14, 21),
        _ => Outcome::Exited(0),
    }
}
fn single(argument: u64) {
    let result = execute(argument);
    state(|s| {
        crate::files::log(
            &mut s.files,
            format_args!("userspace case={argument} result={result:?}\r\n"),
        )
    });
    if result != expected(argument) {
        finish(false);
    }
}
fn batch(name: &str, arguments: [u64; 2]) {
    state(|s| {
        crate::files::log(
            &mut s.files,
            format_args!("scheduler batch={name} begin\r\n"),
        )
    });
    let results = execute_batch(&arguments);
    for (index, &argument) in arguments.iter().enumerate() {
        if results[index] != Some(expected(argument)) {
            finish(false);
        }
    }
    let valid = state(|s| {
        for (index, argument) in arguments.iter().enumerate() {
            let task = s.processes[index].as_ref().unwrap();
            crate::files::log(
                &mut s.files,
                format_args!(
                    "scheduled pid={} scenario={} result={:?} quanta={} calls={}\r\n",
                    index,
                    argument,
                    task.outcome.unwrap(),
                    task.quanta,
                    task.syscalls
                ),
            );
        }
        crate::files::log(
            &mut s.files,
            format_args!("scheduler batch={} switches={} end\r\n", name, s.switches),
        );
        if arguments == [17, 18] {
            s.switches >= 4
                && s.processes
                    .iter()
                    .flatten()
                    .all(|p| p.quanta >= 3 && p.syscalls >= 3)
        } else {
            true
        }
    });
    if !valid {
        finish(false);
    }
}
pub(super) fn run(selected: Option<u64>, peer: Option<u64>) -> ! {
    if let Some(peer) = peer {
        batch("selected", [selected.unwrap_or(0), peer]);
        finish(true);
    }
    if let Some(selected) = selected {
        single(selected);
        finish(true);
    }
    for argument in 0..=18 {
        single(argument);
    }
    batch("compute", [17, 18]);
    batch("loop-peer", [6, 17]);
    batch("fault-peer", [1, 17]);
    batch("queue-peer", [7, 0]);
    single(0);
    state(|s| {
        crate::files::log(
            &mut s.files,
            format_args!("userspace: isolation and recovery passed\r\n"),
        )
    });
    finish(true)
}

/// Kernel Rust must not inherit a user's IF/DF/AC or FP rounding controls.
pub(super) fn entry_state() {
    let mut mxcsr = 0u32;
    let flags: u64;
    // SAFETY: valid aligned output, instructions only observe trusted CPU state.
    unsafe {
        core::arch::asm!("stmxcsr [{value}]", value = in(reg) &mut mxcsr, options(nostack,preserves_flags));
        core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(preserves_flags));
    }
    assert_eq!(mxcsr & 0x6000, 0);
    assert_eq!(flags & ((1 << 8) | (1 << 9) | (1 << 10) | (1 << 18)), 0);
}
