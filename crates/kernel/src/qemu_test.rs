//! Test-image-only termination through QEMU's isa-debug-exit device.

use byte_ring_hal::port::PortRange;

pub fn finish(success: bool) -> ! {
    // SAFETY: the integration runner installs a byte-accessible exit device at
    // port 0xf4. This single-core test image exclusively owns that port.
    let mut exit = unsafe { PortRange::new(0xf4, 1) };
    // QEMU reports (value << 1) | 1: 33 for success, 35 for failure.
    exit.write_u8(0, if success { 0x10 } else { 0x11 });
    super::halt()
}
