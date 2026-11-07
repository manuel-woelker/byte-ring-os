extern crate std;

use core::fmt::Write;
use core::num::NonZeroU32;
use std::{vec, vec::Vec};

use Operation::{Read, Write as PortWrite};
use byte_ring_hal::backend::{Operation, TestIo};

use crate::serial::{COM1_BASE, REGISTER_COUNT, SerialPort, TransmitTimeout};

const STATUS: u16 = COM1_BASE + 5;

fn poll_limit(reads: u32) -> NonZeroU32 {
    NonZeroU32::new(reads).unwrap()
}

fn initialization() -> Vec<Operation> {
    // Check the hardware programming sequence, including clearing an unknown
    // inherited DLAB value before disabling interrupts.
    vec![
        PortWrite {
            address: COM1_BASE + 3,
            value: 0x03,
        },
        PortWrite {
            address: COM1_BASE + 1,
            value: 0x00,
        },
        PortWrite {
            address: COM1_BASE + 3,
            value: 0x80,
        },
        PortWrite {
            address: COM1_BASE,
            value: 0x01,
        },
        PortWrite {
            address: COM1_BASE + 1,
            value: 0x00,
        },
        PortWrite {
            address: COM1_BASE + 3,
            value: 0x03,
        },
        PortWrite {
            address: COM1_BASE + 2,
            value: 0xc7,
        },
        PortWrite {
            address: COM1_BASE + 4,
            value: 0x0b,
        },
    ]
}

#[test]
fn initializes_uart_without_reading_or_transmitting() {
    let mut io = TestIo::new([]);
    {
        let _serial = SerialPort::new(io.ports(COM1_BASE, REGISTER_COUNT), poll_limit(1));
    }
    assert_eq!(io.operations(), initialization());
    io.assert_reads_consumed();
}

#[test]
fn waits_for_transmitter_capacity_before_writing() {
    let mut io = TestIo::new([(STATUS, 0), (STATUS, 0x01), (STATUS, 0x20)]);
    {
        let mut serial = SerialPort::new(io.ports(COM1_BASE, REGISTER_COUNT), poll_limit(3));
        assert_eq!(serial.write_byte(b'A'), Ok(()));
    }
    let mut expected = initialization();
    expected.extend([
        Read {
            address: STATUS,
            value: 0,
        },
        Read {
            address: STATUS,
            value: 0x01,
        },
        Read {
            address: STATUS,
            value: 0x20,
        },
        PortWrite {
            address: COM1_BASE,
            value: b'A',
        },
    ]);
    assert_eq!(io.operations(), expected);
    io.assert_reads_consumed();
}

#[test]
fn timeout_is_bounded_and_does_not_write_the_byte() {
    let mut io = TestIo::new([(STATUS, 0); 3]);
    {
        let mut serial = SerialPort::new(io.ports(COM1_BASE, REGISTER_COUNT), poll_limit(3));
        assert_eq!(serial.write_byte(b'A'), Err(TransmitTimeout));
    }
    let mut expected = initialization();
    expected.extend(
        [Read {
            address: STATUS,
            value: 0,
        }; 3],
    );
    assert_eq!(io.operations(), expected);
    io.assert_reads_consumed();
}

#[test]
fn preserves_binary_data_and_checks_capacity_for_each_byte() {
    let bytes = [0, b'\r', b'\n', 0xff];
    let mut io = TestIo::new([(STATUS, 0x60); 4]);
    {
        let mut serial = SerialPort::new(io.ports(COM1_BASE, REGISTER_COUNT), poll_limit(1));
        assert_eq!(serial.write_bytes(&bytes), Ok(()));
    }
    let mut expected = initialization();
    for byte in bytes {
        expected.push(Read {
            address: STATUS,
            value: 0x60,
        });
        expected.push(PortWrite {
            address: COM1_BASE,
            value: byte,
        });
    }
    assert_eq!(io.operations(), expected);
    io.assert_reads_consumed();
}

#[test]
fn stops_on_timeout_and_can_retry_without_reinitialization() {
    let mut io = TestIo::new([(STATUS, 0x20), (STATUS, 0), (STATUS, 0x20)]);
    {
        let mut serial = SerialPort::new(io.ports(COM1_BASE, REGISTER_COUNT), poll_limit(1));
        assert_eq!(serial.write_bytes(b"ABC"), Err(TransmitTimeout));
        assert_eq!(serial.write_byte(b'B'), Ok(()));
    }
    let mut expected = initialization();
    expected.extend([
        Read {
            address: STATUS,
            value: 0x20,
        },
        PortWrite {
            address: COM1_BASE,
            value: b'A',
        },
        Read {
            address: STATUS,
            value: 0,
        },
        Read {
            address: STATUS,
            value: 0x20,
        },
        PortWrite {
            address: COM1_BASE,
            value: b'B',
        },
    ]);
    assert_eq!(io.operations(), expected);
    io.assert_reads_consumed();
}

#[test]
fn formatting_uses_the_same_transmitter() {
    let mut io = TestIo::new([(STATUS, 0x20); 4]);
    {
        let mut serial = SerialPort::new(io.ports(COM1_BASE, REGISTER_COUNT), poll_limit(1));
        write!(serial, "n={}", 42).unwrap();
    }
    let output: Vec<u8> = io
        .operations()
        .into_iter()
        .skip(initialization().len())
        .filter_map(|operation| match operation {
            PortWrite {
                address: COM1_BASE,
                value,
            } => Some(value),
            _ => None,
        })
        .collect();
    assert_eq!(output, b"n=42");
    io.assert_reads_consumed();
}

#[test]
fn formatting_propagates_transmit_failure() {
    let mut io = TestIo::new([(STATUS, 0)]);
    {
        let mut serial = SerialPort::new(io.ports(COM1_BASE, REGISTER_COUNT), poll_limit(1));
        assert_eq!(serial.write_str("failure"), Err(core::fmt::Error));
    }
    assert_eq!(io.operations().len(), initialization().len() + 1);
    io.assert_reads_consumed();
}

#[test]
fn empty_output_does_not_poll() {
    let mut io = TestIo::new([]);
    {
        let mut serial = SerialPort::new(io.ports(COM1_BASE, REGISTER_COUNT), poll_limit(1));
        serial.write_bytes(b"").unwrap();
    }
    assert_eq!(io.operations(), initialization());
}

#[test]
#[should_panic(expected = "UART requires eight ports")]
fn rejects_an_incomplete_uart_range() {
    let mut io = TestIo::new([]);
    let _serial = SerialPort::new(io.ports(COM1_BASE, 7), poll_limit(1));
}
