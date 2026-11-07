use core::marker::PhantomData;

pub(crate) struct MemoryIo<'a> {
    address: *mut u8,
    ownership: PhantomData<(&'a mut (), *mut ())>,
}

impl MemoryIo<'static> {
    pub(crate) fn new(address: *mut u8) -> Self {
        Self {
            address,
            ownership: PhantomData,
        }
    }
}
impl MemoryIo<'_> {
    pub(crate) fn read(&mut self, offset: usize, width: usize) -> u32 {
        // SAFETY: Memory::claim establishes the mapping and width contract;
        // its methods check all ranges. Native pointers must also be aligned.
        unsafe {
            let address = self.address.add(offset);
            assert_eq!(address as usize % width, 0);
            match width {
                1 => address.read_volatile() as u32,
                2 => u16::from_le(address.cast::<u16>().read_volatile()) as u32,
                4 => u32::from_le(address.cast::<u32>().read_volatile()),
                _ => unreachable!(),
            }
        }
    }
    pub(crate) fn write(&mut self, offset: usize, width: usize, value: u32) {
        // SAFETY: same exclusively owned mapped-region contract as read.
        unsafe {
            let address = self.address.add(offset);
            assert_eq!(address as usize % width, 0);
            match width {
                1 => address.write_volatile(value as u8),
                2 => address.cast::<u16>().write_volatile((value as u16).to_le()),
                4 => address.cast::<u32>().write_volatile(value.to_le()),
                _ => unreachable!(),
            }
        }
    }
}
