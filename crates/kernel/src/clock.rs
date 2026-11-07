//! Boot-owned single-core QEMU clock. No interrupts or scheduler are required.
use byte_ring_hal::port::PortRange;
use byte_ring_kernel_core::clock::Clock;

pub fn initialize() -> Clock {
    // SAFETY: sole boot CPU at CPL0; no other code owns PIT channel 2 or speaker.
    let mut pit = unsafe { PortRange::new(0x42, 2) };
    let mut gate = unsafe { PortRange::new(0x61, 1) };
    let (start, elapsed) = byte_ring_drivers::pit::calibrate(&mut pit, &mut gate, counter)
        .expect("PIT channel-2 calibration timed out");
    Clock::calibrated(start, elapsed).expect("invalid TSC calibration")
}

pub fn counter() -> u64 {
    let low: u32;
    let high: u32;
    // SAFETY: supported QEMU x86_64 CPU has TSC and LFENCE. IF=0, one CPU;
    // this stays on the calibration CPU. Compiler and CPU ordering are required.
    unsafe {
        core::arch::asm!("lfence", "rdtsc", out("eax") low, out("edx") high, options(nostack, preserves_flags));
    }
    (u64::from(high) << 32) | u64::from(low)
}
