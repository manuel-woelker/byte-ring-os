//! Bounded PIT channel-2 calibration; runtime deadlines use the local APIC.
use byte_ring_hal::port::PortRange;

/// Measure counter ticks over 65,535 PIT ticks. The caller exclusively owns
/// ports 0x42..=0x43 and 0x61 and supplies an ordered CPU counter read.
/// Restores speaker/gate bits on both success and timeout.
pub fn calibrate(
    pit: &mut PortRange<'_>,
    gate: &mut PortRange<'_>,
    mut counter: impl FnMut() -> u64,
) -> Option<(u64, u64)> {
    calibrate_with_budget(pit, gate, &mut counter, 5_000_000)
}

fn calibrate_with_budget(
    pit: &mut PortRange<'_>,
    gate: &mut PortRange<'_>,
    mut counter: impl FnMut() -> u64,
    budget: usize,
) -> Option<(u64, u64)> {
    let original = gate.read_u8(0);
    gate.write_u8(0, original & !3);
    pit.write_u8(1, 0xb0); // Channel 2, low/high bytes, mode 0, binary.
    pit.write_u8(0, 0xff);
    pit.write_u8(0, 0xff);
    let start = counter();
    gate.write_u8(0, (original & !2) | 1);
    let mut complete = false;
    for _ in 0..budget {
        if gate.read_u8(0) & 0x20 != 0 {
            complete = true;
            break;
        }
        core::hint::spin_loop();
    }
    let end = counter();
    gate.write_u8(0, original);
    if !complete {
        return None;
    }
    Some((end, end.wrapping_sub(start)))
}

#[cfg(all(test, feature = "test-hal"))]
mod tests {
    use super::*;
    use byte_ring_hal::backend::{Operation, TestIo};

    #[test]
    fn calibration_restores_gate_and_preserves_other_pit_channels() {
        for complete in [true, false] {
            let mut pit_io = TestIo::new([]);
            let mut gate_io = TestIo::new([
                (0x61, 3),
                (0x61, 0),
                (0x61, if complete { 0x20 } else { 0 }),
            ]);
            let mut ticks = [100, 500].into_iter();
            let result = calibrate_with_budget(
                &mut pit_io.ports(0x42, 2),
                &mut gate_io.ports(0x61, 1),
                || ticks.next().unwrap(),
                2,
            );
            assert_eq!(result, if complete { Some((500, 400)) } else { None });
            assert_eq!(
                gate_io.operations().last(),
                Some(&Operation::Write {
                    address: 0x61,
                    value: 3
                })
            );
            assert_eq!(
                pit_io.operations(),
                [
                    Operation::Write {
                        address: 0x43,
                        value: 0xb0
                    },
                    Operation::Write {
                        address: 0x42,
                        value: 0xff
                    },
                    Operation::Write {
                        address: 0x42,
                        value: 0xff
                    }
                ]
            );
            pit_io.assert_reads_consumed();
            gate_io.assert_reads_consumed();
        }
    }
}

/// Stop the firmware's periodic channel-0 timer without loading a countdown.
/// The caller owns 0x40..=0x43 and keeps PIC IRQ0 masked.
pub fn stop_channel0(mut ports: PortRange<'_>) {
    assert_eq!((ports.base(), ports.length()), (0x40, 4));
    ports.write_u8(3, 0x30);
}

#[cfg(all(test, feature = "test-hal"))]
mod stop_tests {
    use super::*;
    use byte_ring_hal::backend::{Operation, TestIo};
    #[test]
    fn stopping_firmware_tick_never_loads_a_deadline() {
        let mut io = TestIo::new([]);
        stop_channel0(io.ports(0x40, 4));
        assert_eq!(
            io.operations(),
            [Operation::Write {
                address: 0x43,
                value: 0x30
            }]
        );
    }
}
