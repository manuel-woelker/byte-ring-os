use super::*;
use byte_ring_hal::memory::{Access, Memory};

#[test]
fn cpu_features_and_base_are_checked_before_mmio() {
    assert!(supported((1 << 5) | (1 << 9)));
    for flags in [0, 1 << 5, 1 << 9] {
        assert!(!supported(flags));
    }
    assert_eq!(base_address(0xfee00900), Ok(0xfee00000));
    for base in [
        0xfee00100,
        0xfee00d00,
        0xfee00800,
        0xfee00901,
        0xfee00b00,
        0x100000900,
        0x900,
    ] {
        assert_eq!(base_address(base), Err(Error::Configuration));
    }
}

fn registers(bytes: &mut [u8]) -> Memory<'_> {
    bytes[VERSION..VERSION + 4].copy_from_slice(&0x00050014u32.to_le_bytes());
    Memory::for_test(bytes, 0xfee00000)
}

#[test]
fn invalid_register_layouts_are_rejected_without_programming() {
    let mut bytes = [0; 4096];
    assert!(matches!(
        Apic::new(Memory::for_test(&mut bytes, 0xfee00001)),
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Apic::new(Memory::for_test(&mut bytes, 0xfee00000)),
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Apic::new(registers(&mut bytes[..1024])),
        Err(Error::Configuration)
    ));
}

#[test]
fn virtual_wire_and_masked_timer_initialization_preserve_pic_delivery() {
    let mut bytes = [0; 4096];
    let mut apic = Apic::new(registers(&mut bytes)).unwrap();
    assert_eq!(apic.registers.read_u32(LINT0), 0x700);
    assert_eq!(apic.registers.read_u32(LINT1), 0x400);
    assert_eq!(apic.registers.read_u32(SPURIOUS), 0x1ff);
    assert_eq!(apic.registers.read_u32(PRIORITY), 0);
    assert_eq!(apic.registers.read_u32(DIVIDE), 0x0a);
    assert_eq!(apic.registers.read_u32(TIMER), MASKED | 48);
    assert_eq!(apic.registers.read_u32(INITIAL), 0);
    for register in [THERMAL, PERFORMANCE, ERROR] {
        assert_ne!(apic.registers.read_u32(register) & MASKED, 0);
    }
    assert_eq!(apic.arm(1000), Err(Error::Uncalibrated));
    assert_eq!(apic.registers.read_u32(INITIAL), 0);
}

#[test]
fn count_conversion_rounds_up_and_handles_long_deadlines() {
    let rate = Rate {
        sample_ticks: 65_535,
    };
    assert_eq!(rate.counts(0), 1);
    assert_eq!(rate.counts(1), 1194);
    assert_eq!(rate.counts(55), 65626);
    assert_eq!(rate.counts(5000), 5_965_910);
    assert_eq!(rate.counts(u64::MAX), u32::MAX);
}

#[test]
fn calibration_failure_always_cancels_and_clears_old_rate() {
    let mut bytes = [0; 4096];
    let mut apic = Apic::new(registers(&mut bytes)).unwrap();
    for (current, elapsed) in [
        (0, 100),
        (100, 0),
        (100, u64::from(u32::MAX)),
        (100, u64::MAX),
    ] {
        apic.rate = Some(Rate { sample_ticks: 10 });
        apic.registers.write_u32(CURRENT, current);
        assert_eq!(apic.finish_calibration(elapsed), Err(Error::Calibration));
        assert_eq!(apic.registers.read_u32(INITIAL), 0);
        assert!(apic.rate.is_none());
    }
}

#[test]
fn rearm_cancel_and_local_eoi_obey_register_contracts() {
    let mut bytes = [0; 4096];
    let mut apic = Apic::new(registers(&mut bytes)).unwrap();
    apic.start_calibration();
    apic.registers.write_u32(CURRENT, u32::MAX - 65_535);
    assert_eq!(apic.calibration_counter(), 65_535);
    apic.finish_calibration(65_535).unwrap();
    let start = apic.registers.accesses().len();
    apic.arm(5000).unwrap();
    let writes = &apic.registers.accesses()[start..];
    assert_eq!(
        writes,
        &[
            Access {
                offset: TIMER,
                width: 4,
                value: MASKED | 48,
                write: true
            },
            Access {
                offset: INITIAL,
                width: 4,
                value: 0,
                write: true
            },
            Access {
                offset: TIMER,
                width: 4,
                value: 48,
                write: true
            },
            Access {
                offset: INITIAL,
                width: 4,
                value: 5_965_910,
                write: true
            },
        ]
    );
    apic.arm(1).unwrap();
    assert_eq!(apic.registers.read_u32(INITIAL), 1194);
    apic.cancel();
    assert_eq!(apic.registers.read_u32(INITIAL), 0);
    apic.finish_timer();
    assert_eq!(
        apic.registers.accesses().last(),
        Some(&Access {
            offset: EOI,
            width: 4,
            value: 0,
            write: true
        })
    );
}
