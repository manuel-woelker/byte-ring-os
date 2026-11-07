//! Boot CPU discovery and calibration; register operations use the safe driver.
use byte_ring_drivers::{
    apic::{self, Apic},
    pci::Window,
    pit,
};
use byte_ring_hal::port::PortRange;

pub fn initialize() -> Apic<'static> {
    // x86_64 supports CPUID leaf 1. No CPU state is changed.
    let features = core::arch::x86_64::__cpuid(1).edx;
    assert!(
        apic::supported(features),
        "CPU has no local APIC/MSR support"
    );
    let low: u32;
    let high: u32;
    // SAFETY: CPUID verified MSR/APIC support, CPL0, IF=0 on the sole CPU.
    unsafe {
        core::arch::asm!("rdmsr", in("ecx") 0x1bu32, out("eax") low, out("edx") high, options(nostack, preserves_flags));
    }
    let address = apic::base_address((u64::from(high) << 32) | u64::from(low))
        .expect("unsupported APIC base or mode");
    // SAFETY: validated enabled local xAPIC base, exclusively owned here;
    // mapper supplies an uncached, permanent supervisor mapping with IF=0.
    let memory = unsafe {
        super::mmio::map(Window {
            address,
            length: 4096,
        })
    }
    .expect("APIC MMIO mapping");
    let mut apic = Apic::new(memory).expect("unsupported local APIC registers");
    // SAFETY: exclusively boot-owned PIT ports, IF=0. Channel-2 TSC calibration
    // has finished and released its handles before this initialization.
    let mut pit_ports = unsafe { PortRange::new(0x42, 2) };
    let mut gate = unsafe { PortRange::new(0x61, 1) };
    apic.start_calibration();
    let sample = pit::calibrate(&mut pit_ports, &mut gate, || apic.calibration_counter());
    let Some((_, elapsed)) = sample else {
        apic.cancel();
        panic!("APIC PIT calibration timed out");
    };
    apic.finish_calibration(elapsed)
        .expect("invalid APIC timer calibration");
    apic
}
