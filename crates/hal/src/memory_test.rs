extern crate std;
use self::std::vec::Vec;
use crate::memory::Access;

pub(crate) struct MemoryIo<'a> {
    bytes: &'a mut [u8],
    accesses: Vec<Access>,
}
impl<'a> MemoryIo<'a> {
    pub(crate) fn new(bytes: &'a mut [u8]) -> Self {
        Self {
            bytes,
            accesses: Vec::new(),
        }
    }
    pub(crate) fn accesses(&self) -> &[Access] {
        &self.accesses
    }
    pub(crate) fn read(&mut self, offset: usize, width: usize) -> u32 {
        let mut raw = [0; 4];
        raw[..width].copy_from_slice(&self.bytes[offset..offset + width]);
        let value = u32::from_le_bytes(raw);
        self.accesses.push(Access {
            offset,
            width,
            value,
            write: false,
        });
        value
    }
    pub(crate) fn write(&mut self, offset: usize, width: usize, value: u32) {
        self.bytes[offset..offset + width].copy_from_slice(&value.to_le_bytes()[..width]);
        self.accesses.push(Access {
            offset,
            width,
            value,
            write: true,
        });
    }
}
