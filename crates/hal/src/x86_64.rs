//! Native x86_64 byte port I/O. Access is mediated by an owned PortRange.

use core::arch::asm;
use core::marker::PhantomData;

// The marker carries the selected backend lifetime and prevents Send/Sync.
pub(crate) struct Io<'a>(PhantomData<(&'a mut (), *mut ())>);

impl Io<'static> {
    pub(crate) fn new() -> Self {
        Self(PhantomData)
    }
}

impl Io<'_> {
    pub(crate) fn read_u16(&mut self, address: u16) -> u16 {
        let value;
        // SAFETY: the owning PortRange checks privilege, range, and width.
        unsafe {
            asm!("in ax, dx", in("dx") address, out("ax") value, options(nostack, preserves_flags));
        }
        value
    }
    pub(crate) fn read_u32(&mut self, address: u16) -> u32 {
        let value;
        // SAFETY: the owning PortRange checks privilege, range, and width.
        unsafe {
            asm!("in eax, dx", in("dx") address, out("eax") value, options(nostack, preserves_flags));
        }
        value
    }
    pub(crate) fn write_u16(&mut self, address: u16, value: u16) {
        // SAFETY: the owning PortRange checks privilege, range, and width.
        unsafe {
            asm!("out dx, ax", in("dx") address, in("ax") value, options(nostack, preserves_flags));
        }
    }
    pub(crate) fn write_u32(&mut self, address: u16, value: u32) {
        // SAFETY: the owning PortRange checks privilege, range, and width.
        unsafe {
            asm!("out dx, eax", in("dx") address, in("eax") value, options(nostack, preserves_flags));
        }
    }

    #[inline]
    pub(crate) fn read_u8(&mut self, address: u16) -> u8 {
        let value: u8;
        // SAFETY: PortRange's unsafe constructor establishes privilege and
        // exclusive ownership; its methods check every offset before access.
        unsafe {
            asm!("in al, dx", in("dx") address, out("al") value,
                options(nostack, preserves_flags));
        }
        value
    }

    #[inline]
    pub(crate) fn write_u8(&mut self, address: u16, value: u8) {
        // SAFETY: the same owned-range contract applies to writes. Do not use
        // nomem: device I/O must not allow memory operations to move across it.
        unsafe {
            asm!("out dx, al", in("dx") address, in("al") value,
                options(nostack, preserves_flags));
        }
    }
}
