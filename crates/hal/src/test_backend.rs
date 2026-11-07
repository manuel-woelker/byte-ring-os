//! Test-owned scripted I/O, selected instead of privileged instructions.

extern crate std;

use self::std::cell::{Cell, RefCell};
use self::std::collections::BTreeMap;
use self::std::collections::VecDeque;
use self::std::vec::Vec;

use crate::port::PortRange;

/// One observable byte operation, including reads to preserve ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Read { address: u16, value: u8 },
    ReadWide { address: u16, width: u8, value: u32 },
    WriteWide { address: u16, width: u8, value: u32 },
    Write { address: u16, value: u8 },
}

/// A test's independent I/O state. Unexpected or misordered reads panic.
type PciFixture = ([u32; 64], [u32; 6]);

pub struct TestIo {
    pci: RefCell<BTreeMap<u32, PciFixture>>,
    selected: Cell<u32>,
    reads: RefCell<VecDeque<(u16, u8)>>,
    operations: RefCell<Vec<Operation>>,
    wide_reads: RefCell<VecDeque<(u16, u8, u32)>>,
}

impl TestIo {
    /// Supply the exact sequence of expected read addresses and return values.
    pub fn new(reads: impl IntoIterator<Item = (u16, u8)>) -> Self {
        Self {
            pci: RefCell::new(BTreeMap::new()),
            selected: Cell::new(0),
            reads: RefCell::new(reads.into_iter().collect()),
            operations: RefCell::new(Vec::new()),
            wide_reads: RefCell::new(VecDeque::new()),
        }
    }

    /// Model one PCI function and its BAR sizing masks, including configuration
    /// address selection. Absent functions read as all ones.
    pub fn pci_function(&mut self, bdf: u32, words: [u32; 64], masks: [u32; 6]) {
        self.pci.get_mut().insert(bdf, (words, masks));
    }

    /// Supply width-aware PCI configuration reads.
    pub fn expect_wide(&mut self, reads: impl IntoIterator<Item = (u16, u8, u32)>) {
        self.wide_reads.get_mut().extend(reads);
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
        assert!(self.wide_reads.borrow().is_empty(), "unconsumed wide reads");
    }
}

pub(crate) struct Io<'a> {
    state: &'a TestIo,
}

impl Io<'_> {
    fn read_wide(&mut self, address: u16, width: u8) -> u32 {
        let value = if let Some((expected, expected_width, value)) =
            self.state.wide_reads.borrow_mut().pop_front()
        {
            assert_eq!((address, width), (expected, expected_width));
            value
        } else {
            assert_eq!(address, 0xcfc, "unexpected wide read");
            let selected = self.state.selected.get();
            let bdf = selected & 0x00ffff00;
            let index = (selected & 0xfc) as usize / 4;
            let pci = self.state.pci.borrow();
            if let Some((words, masks)) = pci.get(&bdf) {
                let value = words[index];
                if (4..10).contains(&index) && value == u32::MAX {
                    masks[index - 4]
                } else {
                    value
                }
            } else {
                u32::MAX
            }
        };
        self.state
            .operations
            .borrow_mut()
            .push(Operation::ReadWide {
                address,
                width,
                value,
            });
        value
    }
    pub(crate) fn read_u16(&mut self, address: u16) -> u16 {
        self.read_wide(address, 2) as u16
    }
    pub(crate) fn read_u32(&mut self, address: u16) -> u32 {
        self.read_wide(address, 4)
    }
    pub(crate) fn write_u16(&mut self, address: u16, value: u16) {
        if address == 0xcfc {
            let selected = self.state.selected.get();
            if let Some((words, _)) = self
                .state
                .pci
                .borrow_mut()
                .get_mut(&(selected & 0x00ffff00))
            {
                let index = (selected & 0xfc) as usize / 4;
                words[index] = (words[index] & 0xffff0000) | value as u32;
            }
        }
        self.state
            .operations
            .borrow_mut()
            .push(Operation::WriteWide {
                address,
                width: 2,
                value: value as u32,
            });
    }
    pub(crate) fn write_u32(&mut self, address: u16, value: u32) {
        if address == 0xcf8 {
            self.state.selected.set(value);
        }
        if address == 0xcfc {
            let selected = self.state.selected.get();
            if let Some((words, _)) = self
                .state
                .pci
                .borrow_mut()
                .get_mut(&(selected & 0x00ffff00))
            {
                words[(selected & 0xfc) as usize / 4] = value;
            }
        }
        self.state
            .operations
            .borrow_mut()
            .push(Operation::WriteWide {
                address,
                width: 4,
                value,
            });
    }

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
