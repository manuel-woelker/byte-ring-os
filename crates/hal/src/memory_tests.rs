use crate::backend::{Operation, TestIo};
use crate::memory::Memory;

#[test]
fn wide_ports_keep_width_and_transaction_order() {
    let mut io = TestIo::new([]);
    io.expect_wide([(0xcfc, 4, 0x12345678), (0xcfc, 2, 0x9876)]);
    {
        let mut ports = io.ports(0xcf8, 8);
        ports.write_u32(0, 0x80000000);
        assert_eq!(ports.read_u32(4), 0x12345678);
        assert_eq!(ports.read_u16(4), 0x9876);
        ports.write_u16(4, 6);
    }
    assert_eq!(
        io.operations()[0],
        Operation::WriteWide {
            address: 0xcf8,
            width: 4,
            value: 0x80000000
        }
    );
    io.assert_reads_consumed();
}
#[test]
fn memory_accesses_are_little_endian_and_test_local() {
    let mut a = [0; 32];
    let mut b = [0; 32];
    let mut first = Memory::for_test(&mut a, 0x1000);
    let mut second = Memory::for_test(&mut b, 0x2000);
    first.write_u32(4, 0x12345678);
    assert_eq!(first.read_u8(4), 0x78);
    assert_eq!(second.read_u32(4), 0);
    second.write_u64(8, u64::MAX);
    assert_eq!(second.read_u32(12), u32::MAX);
    assert_eq!(first.physical(4), 0x1004);
    assert_eq!(first.accesses()[0].width, 4);
}
#[test]
#[should_panic(expected = "memory access outside region")]
fn memory_width_cannot_cross_end() {
    let mut bytes = [0; 8];
    Memory::for_test(&mut bytes, 0).read_u32(8);
}
#[test]
#[should_panic(expected = "unaligned memory offset")]
fn memory_width_requires_alignment() {
    let mut bytes = [0; 8];
    Memory::for_test(&mut bytes, 0).read_u32(1);
}
#[test]
#[should_panic]
fn port_width_cannot_cross_end() {
    TestIo::new([]).ports(0xcf8, 4).write_u32(2, 0);
}
