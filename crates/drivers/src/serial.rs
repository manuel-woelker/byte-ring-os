//! A bounded polling transmitter for a PC-compatible 16550 UART.

use core::fmt;
use core::num::NonZeroU32;

use byte_ring_hal::port::PortRange;

/// Conventional PC COM1 base address. Device presence is a platform assumption.
pub const COM1_BASE: u16 = 0x3f8;
/// Number of consecutive registers owned by a 16550 UART.
pub const REGISTER_COUNT: u16 = 8;

const DATA: u16 = 0;
const INTERRUPT_ENABLE: u16 = 1;
const FIFO_CONTROL: u16 = 2;
const LINE_CONTROL: u16 = 3;
const MODEM_CONTROL: u16 = 4;
const LINE_STATUS: u16 = 5;
const TRANSMITTER_EMPTY: u8 = 1 << 5;

/// The UART did not become writable within the configured number of reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransmitTimeout;

/// Owns its UART registers; requires no allocation, global lock, or interrupts.
pub struct SerialPort<'a> {
    ports: PortRange<'a>,
    poll_limit: NonZeroU32,
}

impl<'a> SerialPort<'a> {
    /// Configure 115200 baud, eight data bits, no parity, and one stop bit.
    ///
    /// Disable UART interrupts, enable/clear FIFOs, and assert DTR/RTS/OUT2.
    /// `poll_limit` bounds status reads per byte, not elapsed wall-clock time.
    ///
    /// # Panics
    /// Panics if the handle does not cover all eight UART registers.
    pub fn new(mut ports: PortRange<'a>, poll_limit: NonZeroU32) -> Self {
        assert_eq!(ports.length(), REGISTER_COUNT, "UART requires eight ports");
        ports.write_u8(LINE_CONTROL, 0x03); // Clear DLAB before accessing IER.
        ports.write_u8(INTERRUPT_ENABLE, 0x00);
        ports.write_u8(LINE_CONTROL, 0x80); // DLAB exposes the baud divisor.
        ports.write_u8(DATA, 0x01);
        ports.write_u8(INTERRUPT_ENABLE, 0x00);
        ports.write_u8(LINE_CONTROL, 0x03);
        ports.write_u8(FIFO_CONTROL, 0xc7);
        ports.write_u8(MODEM_CONTROL, 0x0b);
        Self { ports, poll_limit }
    }

    /// Wait for transmitter capacity, then write one byte verbatim.
    ///
    /// On timeout the byte is not written. This does not detect a missing UART
    /// reliably, or guarantee delivery to a remote receiver.
    pub fn write_byte(&mut self, byte: u8) -> Result<(), TransmitTimeout> {
        for _ in 0..self.poll_limit.get() {
            if self.ports.read_u8(LINE_STATUS) & TRANSMITTER_EMPTY != 0 {
                self.ports.write_u8(DATA, byte);
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(TransmitTimeout)
    }

    /// Write raw bytes without newline conversion. Stops on the first timeout;
    /// an already transmitted prefix is not rolled back.
    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), TransmitTimeout> {
        for &byte in bytes {
            self.write_byte(byte)?;
        }
        Ok(())
    }
}

impl fmt::Write for SerialPort<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.write_bytes(text.as_bytes()).map_err(|_| fmt::Error)
    }
}
