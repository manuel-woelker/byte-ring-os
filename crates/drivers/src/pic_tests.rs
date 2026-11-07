use super::*;
use byte_ring_hal::backend::{Operation, TestIo};

#[test]
fn initialization_routes_only_nic_cascade_and_keeps_irq0_masked() {
    let mut master = TestIo::new([]);
    let mut slave = TestIo::new([]);
    let mut elcr = TestIo::new([(0x4d1, 0x10)]);
    {
        let mut pic = Pic::new(
            master.ports(0x20, 2),
            slave.ports(0xa0, 2),
            elcr.ports(0x4d0, 2),
        );
        pic.configure_network(11);
        pic.arm(11);
        pic.arm(3);
        pic.mask_all();
    }
    let writes = master.operations();
    assert_eq!(
        &writes[..6],
        &[
            Operation::Write {
                address: 0x21,
                value: 0xff
            },
            Operation::Write {
                address: 0x20,
                value: 0x11
            },
            Operation::Write {
                address: 0x21,
                value: 32
            },
            Operation::Write {
                address: 0x21,
                value: 4
            },
            Operation::Write {
                address: 0x21,
                value: 1
            },
            Operation::Write {
                address: 0x21,
                value: 0xff
            },
        ]
    );
    assert_eq!(
        &writes[6..],
        &[
            Operation::Write {
                address: 0x21,
                value: 0xfb
            },
            Operation::Write {
                address: 0x21,
                value: 0xf7
            },
            Operation::Write {
                address: 0x21,
                value: 0xff
            },
        ]
    );
    assert_eq!(
        slave.operations()[6],
        Operation::Write {
            address: 0xa1,
            value: 0xf7
        }
    );
    assert_eq!(
        elcr.operations().last(),
        Some(&Operation::Write {
            address: 0x4d1,
            value: 0x18
        })
    );
    elcr.assert_reads_consumed();
}

#[test]
fn invalid_pci_routes_are_not_accepted() {
    for irq in [0, 1, 2, 8, 13, 16, 255] {
        assert!(!valid_network_irq(irq));
    }
    for irq in [3, 7, 9, 11, 15] {
        assert!(valid_network_irq(irq));
    }
}

#[test]
fn eoi_obeys_slave_order_and_spurious_rules() {
    for (irq, real, master_eoi, slave_eoi) in [
        (0, true, true, false),
        (11, true, true, true),
        (7, false, false, false),
        (7, true, true, false),
        (15, false, true, false),
        (15, true, true, true),
    ] {
        let mut master = TestIo::new((irq == 7).then_some((0x20, if real { 0x80 } else { 0 })));
        let mut slave = TestIo::new((irq == 15).then_some((0xa0, if real { 0x80 } else { 0 })));
        let mut elcr = TestIo::new([]);
        {
            let mut pic = Pic::new(
                master.ports(0x20, 2),
                slave.ports(0xa0, 2),
                elcr.ports(0x4d0, 2),
            );
            pic.finish(irq);
        }
        assert_eq!(
            master.operations().contains(&Operation::Write {
                address: 0x20,
                value: 0x20
            }),
            master_eoi
        );
        assert_eq!(
            slave.operations().contains(&Operation::Write {
                address: 0xa0,
                value: 0x20
            }),
            slave_eoi
        );
        master.assert_reads_consumed();
        slave.assert_reads_consumed();
    }
}
