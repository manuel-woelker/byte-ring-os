extern crate std;
use super::*;
use crate::virtqueue::DMA_SIZE;
use byte_ring_hal::memory::Memory;
fn memory(size: usize, address: u64) -> Memory<'static> {
    Memory::for_test(
        std::boxed::Box::leak(std::vec![0;size].into_boxed_slice()),
        address,
    )
}
fn initialize(features: u32, queue_size: u16, notify_offset: u16) -> Result<Net<'static>, Error> {
    let mut common = memory(64, 0x80000000);
    common.write_u32(4, features);
    common.write_u16(18, 2);
    common.write_u16(24, queue_size);
    common.write_u16(30, notify_offset);
    let mut device = memory(8, 0x80003000);
    device.write_bytes(0, &[0x52, 0x54, 0, 0x12, 0x34, 0x56]);
    Net::new(
        common,
        memory(4096, 0x80001000),
        memory(1, 0x80002000),
        device,
        4,
        memory(DMA_SIZE, 0x100000),
        memory(DMA_SIZE, 0x200000),
    )
}
#[test]
fn feature_and_notify_failures_are_bounded_errors() {
    assert!(matches!(initialize(0, 32, 0), Err(Error::Features)));
    assert!(matches!(initialize(0x21, 0, 0), Err(Error::Configuration)));
    assert!(matches!(
        initialize(0x21, 32, 2048),
        Err(Error::Configuration)
    ));
}
#[test]
fn initialization_programs_split_queue_addresses_and_driver_ok() {
    let mut net = initialize(0x21, 32, 1).unwrap();
    assert_eq!(net.mac, [0x52, 0x54, 0, 0x12, 0x34, 0x56]);
    assert_eq!(net.common.read_u8(20), 15);
    assert_eq!(net.common.read_u16(28), 1);
    assert_eq!(net.notify.read_u16(4), 0);
    net.transmit(b"packet").unwrap();
    assert_eq!(net.notify.read_u16(4), 1);
}
