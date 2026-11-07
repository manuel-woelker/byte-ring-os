use super::*;
use byte_ring_hal::backend::TestIo;
fn configuration() -> [u32; 64] {
    let mut words = [0; 64];
    words[0] = 0x10411af4;
    words[1] = 1 << 20;
    words[4] = 0x80000004;
    words[13] = 0x40;
    for (index, kind) in [1u32, 2, 3, 4].into_iter().enumerate() {
        let position = 16 + index * 5;
        let next = if index == 3 {
            0
        } else {
            0x40 + (index as u32 + 1) * 20
        };
        words[position] = 9 | (next << 8) | (20 << 16) | (kind << 24);
        words[position + 1] = 0;
        words[position + 2] = (index * 0x1000) as u32;
        words[position + 3] = 0x1000;
        if kind == 2 {
            words[position + 4] = 4;
        }
    }
    words
}
fn discover(words: [u32; 64]) -> Result<Transport, Error> {
    let mut io = TestIo::new([]);
    io.pci_function(0x1800, words, [0xffffc004, u32::MAX, 0, 0, 0, 0]);
    Pci::new(io.ports(0xcf8, 8)).discover()
}
#[test]
fn modern_capabilities_and_64_bit_bar_are_discovered() {
    let device = discover(configuration()).unwrap();
    assert_eq!(device.bdf, 0x1800);
    assert_eq!(device.common.address, 0x80000000);
    assert_eq!(device.device.address, 0x80003000);
    assert_eq!(device.notify_multiplier, 4);
}
#[test]
fn missing_device_is_an_explicit_error() {
    let mut io = TestIo::new([]);
    assert!(matches!(
        Pci::new(io.ports(0xcf8, 8)).discover(),
        Err(Error::Missing)
    ));
}
#[test]
fn cyclic_and_out_of_range_capability_lists_are_rejected() {
    let mut words = configuration();
    words[16] = (words[16] & !0xff00) | 0x4000;
    assert!(matches!(discover(words), Err(Error::Capabilities)));
    let mut words = configuration();
    words[13] = 0x3c;
    assert!(matches!(discover(words), Err(Error::Capabilities)));
    let mut words = configuration();
    words[16] = (words[16] & !0xff0000) | (12 << 16);
    assert!(matches!(discover(words), Err(Error::Capabilities)));
}
#[test]
fn bar_bounds_overlaps_and_high_addresses_are_rejected() {
    let mut words = configuration();
    words[18] = 0x4000;
    assert!(matches!(discover(words), Err(Error::Window)));
    let mut words = configuration();
    words[23] = 0;
    assert!(matches!(discover(words), Err(Error::Window)));
    let mut words = configuration();
    words[5] = 1;
    assert!(matches!(discover(words), Err(Error::Window)));
}
#[test]
fn multifunction_discovery_uses_the_correct_function() {
    let mut io = TestIo::new([]);
    let mut first = [0; 64];
    first[0] = 0x12341234;
    first[3] = 0x00800000;
    io.pci_function(0x1800, first, [0; 6]);
    io.pci_function(0x1900, configuration(), [0xffffc004, u32::MAX, 0, 0, 0, 0]);
    assert_eq!(Pci::new(io.ports(0xcf8, 8)).discover().unwrap().bdf, 0x1900);
}
