//! Conversion from a PIT-calibrated counter to monotonic milliseconds.

pub const PIT_HZ: u64 = 1_193_182;
pub const PIT_COUNT: u64 = 65_535;

/// A single CPU counter epoch. Calibration spans one PIT channel-2 countdown.
#[derive(Debug)]
pub struct Clock {
    start: u64,
    ticks_per_ms: u64,
}

impl Clock {
    /// Rejects a calibration too short to resolve milliseconds.
    pub fn calibrated(start: u64, elapsed_ticks: u64) -> Option<Self> {
        let ticks_per_ms =
            (u128::from(elapsed_ticks) * u128::from(PIT_HZ) / u128::from(PIT_COUNT * 1000)) as u64;
        if ticks_per_ms == 0 {
            return None;
        }
        Some(Self {
            start,
            ticks_per_ms,
        })
    }

    /// The counter must advance on the same CPU. One wrapping epoch is allowed.
    pub fn milliseconds(&self, counter: u64) -> i64 {
        (counter.wrapping_sub(self.start) / self.ticks_per_ms).min(i64::MAX as u64) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibrated_elapsed_time_and_rounding() {
        // A synthetic counter at exactly PIT_HZ ticks/ms.
        let clock = Clock::calibrated(123, PIT_COUNT * 1000).unwrap();
        assert_eq!(clock.milliseconds(123), 0);
        assert_eq!(clock.milliseconds(123 + PIT_HZ - 1), 0);
        assert_eq!(clock.milliseconds(123 + PIT_HZ * 1234), 1234);
    }

    #[test]
    fn invalid_calibration_and_counter_wrap() {
        assert!(Clock::calibrated(0, 0).is_none());
        let clock = Clock::calibrated(u64::MAX - PIT_HZ, PIT_COUNT * 1000).unwrap();
        assert_eq!(clock.milliseconds(PIT_HZ - 1), 2);
    }
}
