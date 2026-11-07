//! Exclusive access to a bounded range of byte-wide hardware I/O ports.

use crate::backend::Io;

/// A non-cloneable port range. Device drivers own and serialize its accesses.
pub struct PortRange<'a> {
    base: u16,
    length: u16,
    io: Io<'a>,
}

impl PortRange<'static> {
    /// Claim an exclusively owned hardware port range.
    ///
    /// # Safety
    /// The caller must be executing with I/O privilege and own the complete
    /// range for the lifetime of this handle. Reads and writes at every offset
    /// must be valid device operations. Other code, CPUs, and interrupt handlers
    /// must not access this range concurrently or invalidate its configuration.
    ///
    /// # Panics
    /// Panics if the range is empty or extends beyond the last x86 I/O port.
    #[cfg(any(not(feature = "test-hal"), target_os = "none"))]
    pub unsafe fn new(base: u16, length: u16) -> Self {
        Self::from_io(base, length, Io::new())
    }
}

impl<'a> PortRange<'a> {
    pub(crate) fn from_io(base: u16, length: u16, io: Io<'a>) -> Self {
        assert!(length > 0, "port range must not be empty");
        assert!(
            base.checked_add(length - 1).is_some(),
            "port range overflow"
        );
        Self { base, length, io }
    }

    /// First port address in this range.
    pub fn base(&self) -> u16 {
        self.base
    }

    /// Number of byte-wide ports in this range.
    pub fn length(&self) -> u16 {
        self.length
    }

    /// Read a byte from an offset within the owned range.
    ///
    /// # Panics
    /// Panics if `offset` is outside the range.
    #[inline]
    pub fn read_u8(&mut self, offset: u16) -> u8 {
        let address = self.address(offset);
        self.io.read_u8(address)
    }

    /// Write a byte to an offset within the owned range.
    ///
    /// # Panics
    /// Panics if `offset` is outside the range.
    #[inline]
    pub fn write_u8(&mut self, offset: u16, value: u8) {
        let address = self.address(offset);
        self.io.write_u8(address, value);
    }

    #[inline]
    fn address(&self, offset: u16) -> u16 {
        assert!(offset < self.length, "port offset outside owned range");
        self.base + offset
    }
}
