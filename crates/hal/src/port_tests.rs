use crate::backend::{Operation, TestIo};

#[test]
fn translates_offsets_and_keeps_test_instances_independent() {
    let mut first = TestIo::new([(0x3fd, 0x20)]);
    let mut second = TestIo::new([(0x3fd, 0x00)]);
    {
        let mut first_ports = first.ports(0x3f8, 8);
        let mut second_ports = second.ports(0x3f8, 8);
        assert_eq!(first_ports.base(), 0x3f8);
        assert_eq!(first_ports.read_u8(5), 0x20);
        assert_eq!(second_ports.read_u8(5), 0);
        first_ports.write_u8(0, b'A');
        second_ports.write_u8(0, b'B');
    }
    assert_eq!(
        first.operations(),
        [
            Operation::Read {
                address: 0x3fd,
                value: 0x20
            },
            Operation::Write {
                address: 0x3f8,
                value: b'A'
            },
        ]
    );
    assert_eq!(
        second.operations(),
        [
            Operation::Read {
                address: 0x3fd,
                value: 0
            },
            Operation::Write {
                address: 0x3f8,
                value: b'B'
            },
        ]
    );
    first.assert_reads_consumed();
    second.assert_reads_consumed();
}

#[test]
fn allows_the_last_port_without_overflow() {
    let mut io = TestIo::new([(u16::MAX, 0xff)]);
    {
        let mut ports = io.ports(u16::MAX, 1);
        assert_eq!(ports.read_u8(0), 0xff);
    }
    io.assert_reads_consumed();
}

#[test]
#[should_panic(expected = "port range must not be empty")]
fn rejects_empty_range() {
    let mut io = TestIo::new([]);
    let _ports = io.ports(0x3f8, 0);
}

#[test]
#[should_panic(expected = "port range overflow")]
fn rejects_address_overflow() {
    let mut io = TestIo::new([]);
    let _ports = io.ports(u16::MAX, 2);
}

#[test]
#[should_panic(expected = "port offset outside owned range")]
fn rejects_out_of_range_read() {
    let mut io = TestIo::new([]);
    io.ports(0x3f8, 8).read_u8(8);
}

#[test]
#[should_panic(expected = "port offset outside owned range")]
fn rejects_out_of_range_write() {
    let mut io = TestIo::new([]);
    io.ports(0x3f8, 8).write_u8(8, 0);
}

#[test]
#[should_panic(expected = "unexpected port read: no scripted value remains")]
fn rejects_unscripted_read() {
    let mut io = TestIo::new([]);
    io.ports(0x3f8, 8).read_u8(5);
}

#[test]
#[should_panic(expected = "unexpected port read address")]
fn rejects_wrong_read_address() {
    let mut io = TestIo::new([(0x3fd, 0x20)]);
    io.ports(0x3f8, 8).read_u8(4);
}
