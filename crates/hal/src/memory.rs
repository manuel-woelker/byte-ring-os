//! Bounded, non-cloneable access to mapped MMIO or exclusively owned DMA RAM.

use crate::memory_backend::MemoryIo;

/// A width-aware memory access recorded by the test backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Access {
    pub offset: usize,
    pub width: usize,
    pub value: u32,
    pub write: bool,
}

pub struct Memory<'a> {
    io: MemoryIo<'a>,
    length: usize,
    physical: u64,
}

impl Memory<'static> {
    /// Claim a permanently mapped region without constructing references to
    /// device-mutated bytes. Hardware may access DMA RAM under driver ownership.
    ///
    /// # Safety
    /// The mapping must be valid, correctly typed/cached, and exclusively owned
    /// for the lifetime of the device. `physical` must be its actual DMA/bus
    /// address. No Rust references may alias device-written storage. The caller
    /// must stop DMA before any reuse and supply valid device access widths.
    #[cfg(any(not(feature = "test-hal"), target_os = "none"))]
    pub unsafe fn claim(address: *mut u8, length: usize, physical: u64) -> Self {
        assert!(length > 0 && !address.is_null());
        assert!((address as usize).checked_add(length).is_some());
        assert!(physical.checked_add(length as u64).is_some());
        Self {
            io: MemoryIo::new(address),
            length,
            physical,
        }
    }
}

impl<'a> Memory<'a> {
    #[cfg(all(feature = "test-hal", not(target_os = "none")))]
    pub fn for_test(bytes: &'a mut [u8], physical: u64) -> Self {
        Self {
            length: bytes.len(),
            physical,
            io: MemoryIo::new(bytes),
        }
    }

    #[cfg(all(feature = "test-hal", not(target_os = "none")))]
    pub fn accesses(&self) -> &[Access] {
        self.io.accesses()
    }

    pub fn length(&self) -> usize {
        self.length
    }

    pub fn physical(&self, offset: usize) -> u64 {
        self.bounds(offset, 1);
        self.physical
            .checked_add(offset as u64)
            .expect("physical address overflow")
    }

    fn bounds(&self, offset: usize, width: usize) {
        assert!(
            offset
                .checked_add(width)
                .is_some_and(|end| end <= self.length),
            "memory access outside region"
        );
        assert_eq!(offset % width, 0, "unaligned memory offset");
        assert_eq!(self.physical % width as u64, 0, "unaligned memory base");
    }

    pub fn read_u8(&mut self, offset: usize) -> u8 {
        self.bounds(offset, 1);
        self.io.read(offset, 1) as u8
    }
    pub fn read_u16(&mut self, offset: usize) -> u16 {
        self.bounds(offset, 2);
        self.io.read(offset, 2) as u16
    }
    pub fn read_u32(&mut self, offset: usize) -> u32 {
        self.bounds(offset, 4);
        self.io.read(offset, 4)
    }
    pub fn write_u8(&mut self, offset: usize, value: u8) {
        self.bounds(offset, 1);
        self.io.write(offset, 1, value as u32);
    }
    pub fn write_u16(&mut self, offset: usize, value: u16) {
        self.bounds(offset, 2);
        self.io.write(offset, 2, value as u32);
    }
    pub fn write_u32(&mut self, offset: usize, value: u32) {
        self.bounds(offset, 4);
        self.io.write(offset, 4, value);
    }
    pub fn write_u64(&mut self, offset: usize, value: u64) {
        self.bounds(offset, 8);
        self.write_u32(offset, value as u32);
        self.write_u32(offset + 4, (value >> 32) as u32);
    }
    pub fn read_bytes(&mut self, offset: usize, bytes: &mut [u8]) {
        assert!(
            offset
                .checked_add(bytes.len())
                .is_some_and(|end| end <= self.length)
        );
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = self.read_u8(offset + index);
        }
    }
    pub fn write_bytes(&mut self, offset: usize, bytes: &[u8]) {
        assert!(
            offset
                .checked_add(bytes.len())
                .is_some_and(|end| end <= self.length)
        );
        for (index, &byte) in bytes.iter().enumerate() {
            self.write_u8(offset + index, byte);
        }
    }
    pub fn clear(&mut self) {
        for offset in 0..self.length {
            self.write_u8(offset, 0);
        }
    }
}
