//! Test-owned scripted I/O, selected instead of privileged instructions.

extern crate std;

use self::std::cell::RefCell;
use self::std::collections::VecDeque;
use self::std::vec::Vec;

use crate::port::PortRange;

/// One observable byte operation, including reads to preserve ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Read { address: u16, value: u8 },
    Write { address: u16, value: u8 },
}

/// A test's independent I/O state. Unexpected or misordered reads panic.
pub struct TestIo {
    reads: RefCell<VecDeque<(u16, u8)>>,
    operations: RefCell<Vec<Operation>>,
}

impl TestIo {
    /// Supply the exact sequence of expected read addresses and return values.
    pub fn new(reads: impl IntoIterator<Item = (u16, u8)>) -> Self {
        Self {
            reads: RefCell::new(reads.into_iter().collect()),
            operations: RefCell::new(Vec::new()),
        }
    }

    /// Borrow this test's state exclusively through a bounded port handle.
    ///
    /// # Panics
    /// Panics for an empty range or an address overflow.
    pub fn ports(&mut self, base: u16, length: u16) -> PortRange<'_> {
        PortRange::from_io(base, length, Io { state: self })
    }

    /// Return the ordered read/write log after the driver releases its handle.
    pub fn operations(&self) -> Vec<Operation> {
        self.operations.borrow().clone()
    }

    /// Assert that the driver performed every supplied read.
    pub fn assert_reads_consumed(&self) {
        assert!(self.reads.borrow().is_empty(), "unconsumed scripted reads");
    }
}

pub(crate) struct Io<'a> {
    state: &'a TestIo,
}

impl Io<'_> {
    pub(crate) fn read_u8(&mut self, address: u16) -> u8 {
        let (expected_address, value) = self
            .state
            .reads
            .borrow_mut()
            .pop_front()
            .expect("unexpected port read: no scripted value remains");
        assert_eq!(address, expected_address, "unexpected port read address");
        self.state
            .operations
            .borrow_mut()
            .push(Operation::Read { address, value });
        value
    }

    pub(crate) fn write_u8(&mut self, address: u16, value: u8) {
        self.state
            .operations
            .borrow_mut()
            .push(Operation::Write { address, value });
    }
}
