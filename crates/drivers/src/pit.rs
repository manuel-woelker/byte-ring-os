//! PIT channel-2 calibration and channel-0 one-shot deadline interrupts.
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

/// Channel-0 interrupt timer. The PIC masks IRQ0 to cancel a pending wakeup.
pub struct Timer<'a> {
    ports: PortRange<'a>,
}
impl<'a> Timer<'a> {
    pub fn new(mut ports: PortRange<'a>) -> Self {
        assert_eq!((ports.base(), ports.length()), (0x40, 4));
        // Stop the firmware's periodic timer. A count is loaded only for a
        // real deadline; masking IRQ0 alone would leave the PIT ticking.
        ports.write_u8(3, 0x30);
        Self { ports }
    }
    /// Round up to hardware ticks, clamping a long deadline to one countdown.
    pub fn arm(&mut self, milliseconds: u64) {
        let ticks = (u128::from(milliseconds) * 1_193_182)
            .div_ceil(1000)
            .clamp(1, 65_535) as u16;
        self.ports.write_u8(3, 0x30); // Channel 0, low/high bytes, mode 0.
        self.ports.write_u8(0, ticks as u8);
        self.ports.write_u8(0, (ticks >> 8) as u8);
    }
}

#[cfg(all(test, feature = "test-hal"))]
mod timer_tests {
    use super::*;
    use byte_ring_hal::backend::{Operation, TestIo};
    #[test]
    fn one_shot_rounds_up_and_caps_without_wrapping() {
        for (ms, ticks) in [
            (0, 1u16),
            (1, 1194),
            (10, 11932),
            (55, 65535),
            (u64::MAX, 65535),
        ] {
            let mut io = TestIo::new([]);
            Timer::new(io.ports(0x40, 4)).arm(ms);
            assert_eq!(
                io.operations(),
                [
                    Operation::Write {
                        address: 0x43,
                        value: 0x30
                    },
                    Operation::Write {
                        address: 0x43,
                        value: 0x30
                    },
                    Operation::Write {
                        address: 0x40,
                        value: ticks as u8
                    },
                    Operation::Write {
                        address: 0x40,
                        value: (ticks >> 8) as u8
                    }
                ]
            );
        }
    }
}
