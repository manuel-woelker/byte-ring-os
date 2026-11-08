//! Native ring-3 isolation/recovery suite, present only in the test image.
use super::*;
use byte_ring_hal::port::PortRange;
pub(super) fn finish(success: bool) -> ! {
    // SAFETY: the QEMU integration image exclusively owns isa-debug-exit.
    let mut port = unsafe { PortRange::new(0xf4, 1) };
    port.write_u8(0, if success { 0x10 } else { 0x11 });
    crate::halt()
}
pub(super) fn run(selected: Option<u64>) -> ! {
    for (argument, expected) in [
        (0, Outcome::Exited(0)),
        (1, Outcome::Fault(14, 5)),
        (2, Outcome::Fault(14, 7)),
        (3, Outcome::Fault(14, 7)),
        (4, Outcome::Fault(14, 4)),
        (5, Outcome::Fault(13, 0)),
        (6, Outcome::Timeout),
        (7, Outcome::Exited(0)),
        (8, Outcome::Exited(0)),
        (9, Outcome::BadReturn),
        (10, Outcome::Fault(14, 7)),
        (11, Outcome::Fault(14, 21)),
        (12, Outcome::Exited(0)),
        (13, Outcome::Exited(0)),
        (14, Outcome::Fault(14, 7)),
        (15, Outcome::Fault(14, 7)),
        (16, Outcome::BadReturn),
        (0, Outcome::Exited(0)),
    ] {
        if selected.is_some_and(|selected| selected != argument) {
            continue;
        }
        let result = execute(argument);
        state(|s| {
            crate::files::log(
                &mut s.files,
                format_args!("userspace case={argument} result={result:?}\r\n"),
            )
        });
        if result != expected {
            finish(false);
        }
        if selected.is_some() {
            finish(true);
        }
    }
    state(|s| {
        crate::files::log(
            &mut s.files,
            format_args!("userspace: isolation and recovery passed\r\n"),
        )
    });
    finish(true)
}

/// Kernel code must not inherit a user's DF/IF or FP rounding controls.
pub(super) fn entry_state() {
    let mut mxcsr = 0u32;
    let flags: u64;
    // SAFETY: output stack slots are aligned/valid and neither instruction
    // changes CPU state. This runs only on the trusted entry stack.
    unsafe {
        core::arch::asm!("stmxcsr [{value}]", value = in(reg) &mut mxcsr, options(nostack, preserves_flags));
        core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(preserves_flags));
    }
    assert_eq!(mxcsr & 0x6000, 0);
    assert_eq!(flags & ((1 << 8) | (1 << 9) | (1 << 10) | (1 << 18)), 0);
}
