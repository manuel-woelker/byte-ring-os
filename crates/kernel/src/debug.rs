//! Single-owner early boot diagnostics, before interrupts or SMP exist.

use core::num::NonZeroU32;

use byte_ring_drivers::serial::{COM1_BASE, REGISTER_COUNT, SerialPort};
use byte_ring_hal::port::PortRange;

/// Claim COM1 for early diagnostics.
///
/// # Safety
/// The caller must run at CPL0 and ensure no other code can access COM1 while
/// this handle is in use. A panic may take over only if the previous driver
/// will never be used again.
pub unsafe fn com1() -> SerialPort<'static> {
    // SAFETY: the PC boot adapter enters CPL0 with interrupts disabled. This
    // early kernel has one CPU and no other COM1 owner. Panic abandons the
    // interrupted driver permanently before constructing a replacement.
    let ports = unsafe { PortRange::new(COM1_BASE, REGISTER_COUNT) };
    SerialPort::new(ports, NonZeroU32::new(100_000).unwrap())
}

/// Claim the terminal diagnostic path without a lock or allocation. Recursive
/// faults stop immediately rather than recursively formatting or taking COM1.
pub fn begin_fatal() {
    use core::sync::atomic::{AtomicBool, Ordering};

    static ACTIVE: AtomicBool = AtomicBool::new(false);
    if ACTIVE.swap(true, Ordering::Relaxed) {
        #[cfg(feature = "qemu-test")]
        super::qemu_test::finish(false);
        #[cfg(not(feature = "qemu-test"))]
        super::halt();
    }
}
