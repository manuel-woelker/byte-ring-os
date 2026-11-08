//! Terminal x86_64 exception entry, descriptor tables, and emergency stacks.

mod tables;

use byte_ring_kernel_core::exception::{ExceptionFrame, write_report};

core::arch::global_asm!(include_str!("exceptions/entry.S"), userspace = const (cfg!(feature = "userspace") as u8), options(att_syntax));

/// Install the initial exception tables before initializing device drivers.
///
/// # Safety
/// Call exactly once at CPL0 with interrupts disabled, on the sole boot CPU.
pub unsafe fn initialize() {
    // SAFETY: forwarded boot-time privilege and exclusive ownership contract.
    unsafe { tables::install() };
}

#[unsafe(no_mangle)]
extern "C" fn exception_dispatch(frame: &ExceptionFrame) -> ! {
    #[cfg(feature = "userspace")]
    if frame.code_segment & 3 == 3 && !matches!(frame.vector, 2 | 8 | 18) {
        super::userspace::fault(frame);
    }
    super::debug::begin_fatal();
    // SAFETY: interrupt gates disable interrupts, this kernel is single-core,
    // and the interrupted owner can never resume after this fatal handler.
    let mut serial = unsafe { super::debug::com1() };
    let reported = write_report(&mut serial, frame).is_ok();
    #[cfg(feature = "qemu-test")]
    super::qemu_test::exception(frame, reported, tables::on_double_fault_stack(frame));
    #[cfg(not(feature = "qemu-test"))]
    {
        let _ = reported;
        super::halt()
    }
}

/// # Safety
/// IF=0 on the sole CPU; top identifies the selected process's pinned stack.
#[cfg(feature = "userspace")]
pub unsafe fn set_user_stack(top: u64) {
    unsafe {
        tables::set_user_stack(top);
    }
}
