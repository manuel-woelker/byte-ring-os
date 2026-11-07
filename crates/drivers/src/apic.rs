//! Local xAPIC one-shot deadlines with explicit virtual-wire and MMIO ownership.
use byte_ring_hal::memory::Memory;

pub const TIMER_VECTOR: u8 = 48;
pub const SPURIOUS_VECTOR: u8 = 255;
const VERSION: usize = 0x30;
const PRIORITY: usize = 0x80;
const EOI: usize = 0xb0;
const SPURIOUS: usize = 0xf0;
const TIMER: usize = 0x320;
const THERMAL: usize = 0x330;
const PERFORMANCE: usize = 0x340;
const LINT0: usize = 0x350;
const LINT1: usize = 0x360;
const ERROR: usize = 0x370;
const INITIAL: usize = 0x380;
const CURRENT: usize = 0x390;
const DIVIDE: usize = 0x3e0;
const MASKED: u32 = 1 << 16;
const PIT_HZ: u128 = 1_193_182;
const SAMPLE_MS_DENOMINATOR: u128 = 65_535 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Configuration,
    Calibration,
    Uncalibrated,
}

/// CPUID leaf 1 EDX must expose both MSR and local APIC before reading its MSR.
pub fn supported(features: u32) -> bool {
    features & ((1 << 5) | (1 << 9)) == (1 << 5) | (1 << 9)
}

/// Validate the enabled BSP xAPIC MSR and the current mapper's physical range.
/// x2APIC, disabled APIC, reserved flags, and high mappings are unsupported.
pub fn base_address(msr: u64) -> Result<u64, Error> {
    let address = msr & !0xfff;
    if msr & 0xfff != 0x900 || !(0x40000000..=0xfffff000).contains(&address) {
        return Err(Error::Configuration);
    }
    Ok(address)
}

struct Rate {
    sample_ticks: u32,
}
impl Rate {
    fn counts(&self, milliseconds: u64) -> u32 {
        let numerator = u128::from(milliseconds) * u128::from(self.sample_ticks) * PIT_HZ;
        numerator
            .div_ceil(SAMPLE_MS_DENOMINATOR)
            .clamp(1, u128::from(u32::MAX)) as u32
    }
}

/// Exclusive local APIC owner. All operations require IF=0 on the boot CPU.
pub struct Apic<'a> {
    registers: Memory<'a>,
    rate: Option<Rate>,
}
impl<'a> Apic<'a> {
    /// The caller has validated and enabled the xAPIC base MSR. Registers must
    /// be uncached. PIC delivery remains on LINT0 ExtINT, with NMI on LINT1.
    pub fn new(mut registers: Memory<'a>) -> Result<Self, Error> {
        if registers.length() < 4096 || !registers.physical(0).is_multiple_of(4096) {
            return Err(Error::Configuration);
        }
        let version = registers.read_u32(VERSION);
        if version & 0xff < 0x10 || (version >> 16) & 0xff < 5 {
            return Err(Error::Configuration);
        }
        registers.write_u32(TIMER, MASKED | u32::from(TIMER_VECTOR));
        registers.write_u32(INITIAL, 0);
        registers.write_u32(DIVIDE, 0x0a); // Divide by 128; one-shot mode.
        for register in [THERMAL, PERFORMANCE, ERROR] {
            registers.write_u32(register, MASKED | 49);
        }
        registers.write_u32(LINT0, 7 << 8); // ExtINT; PIC EOI only.
        registers.write_u32(LINT1, 4 << 8); // NMI.
        registers.write_u32(PRIORITY, 0);
        registers.write_u32(SPURIOUS, (1 << 8) | u32::from(SPURIOUS_VECTOR));
        Ok(Self {
            registers,
            rate: None,
        })
    }
    /// Start a masked countdown before sampling with PIT channel 2.
    pub fn start_calibration(&mut self) {
        self.cancel();
        self.rate = None;
        self.registers.write_u32(INITIAL, u32::MAX);
    }
    /// Increasing counter for PIT's two calibration callbacks. The countdown
    /// must not expire during the sample; finish_calibration verifies this.
    pub fn calibration_counter(&mut self) -> u64 {
        u64::from(u32::MAX - self.registers.read_u32(CURRENT))
    }
    /// Install a nonzero, non-expired sample spanning exactly PIT's countdown.
    /// Both failure and success leave the timer stopped and masked.
    pub fn finish_calibration(&mut self, elapsed_ticks: u64) -> Result<(), Error> {
        let current = self.registers.read_u32(CURRENT);
        self.cancel();
        self.rate = None;
        if current == 0 || elapsed_ticks == 0 || elapsed_ticks >= u64::from(u32::MAX) {
            return Err(Error::Calibration);
        }
        self.rate = Some(Rate {
            sample_ticks: elapsed_ticks as u32,
        });
        Ok(())
    }
    /// Stop before rearming, unmask in one-shot mode, and load rounded-up ticks.
    /// The hardware maximum shortens only exceptionally long waits, never
    /// allowing count multiplication or a zero count to disable the timer.
    pub fn arm(&mut self, milliseconds: u64) -> Result<(), Error> {
        self.cancel();
        let counts = self
            .rate
            .as_ref()
            .ok_or(Error::Uncalibrated)?
            .counts(milliseconds);
        self.registers.write_u32(TIMER, u32::from(TIMER_VECTOR));
        self.registers.write_u32(INITIAL, counts);
        Ok(())
    }
    pub fn cancel(&mut self) {
        self.registers
            .write_u32(TIMER, MASKED | u32::from(TIMER_VECTOR));
        self.registers.write_u32(INITIAL, 0);
    }
    /// Only local timer delivery receives this EOI; ExtINT and spurious do not.
    pub fn finish_timer(&mut self) {
        self.registers.write_u32(EOI, 0);
    }
}

#[cfg(all(test, feature = "test-hal"))]
#[path = "apic_tests.rs"]
mod tests;
