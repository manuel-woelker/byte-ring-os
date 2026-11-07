extern crate std;
use super::*;
use crate::user_abi::{put_u32, put_u64};
use std::{vec, vec::Vec};
fn executable() -> Vec<u8> {
    let mut bytes = vec![0; 8192];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    bytes[16..20].copy_from_slice(&[2, 0, 62, 0]);
    put_u32(&mut bytes, 20, 1);
    put_u64(&mut bytes, 24, BASE);
    put_u64(&mut bytes, 32, 64);
    bytes[52..58].copy_from_slice(&[64, 0, 56, 0, 1, 0]);
    put_u32(&mut bytes, 64, 1);
    put_u32(&mut bytes, 68, 5);
    put_u64(&mut bytes, 72, 4096);
    put_u64(&mut bytes, 80, BASE);
    put_u64(&mut bytes, 96, 16);
    put_u64(&mut bytes, 104, PAGE as u64);
    put_u64(&mut bytes, 112, PAGE as u64);
    bytes[4096..4112].fill(0x90);
    bytes
}
#[test]
fn initialized_data_and_bss_have_explicit_page_permissions() {
    let mut bytes = executable();
    bytes[56] = 2;
    put_u32(&mut bytes, 120, 1);
    put_u32(&mut bytes, 124, 6);
    put_u64(&mut bytes, 128, 8192);
    put_u64(&mut bytes, 136, BASE + PAGE as u64);
    put_u64(&mut bytes, 160, PAGE as u64);
    put_u64(&mut bytes, 168, PAGE as u64);
    let image = Image::parse(&bytes).unwrap();
    assert!(image.executable(BASE));
    assert!(!image.executable(BASE + 16));
    assert_eq!(image.permission(0), Permission::Execute);
    assert_eq!(image.permission(1), Permission::Write);
    assert_eq!(image.permission(2), Permission::Absent);
    let mut backing = [0xff; IMAGE_PAGES * PAGE];
    image.load(&mut backing);
    assert_eq!(&backing[..16], &[0x90; 16]);
    assert!(backing[16..].iter().all(|byte| *byte == 0));
}
#[test]
fn malformed_headers_and_tables_are_bounded_errors() {
    for bytes in [&[][..], &[0; 63][..]] {
        assert_eq!(Image::parse(bytes).err(), Some(Error::Header));
    }
    for (offset, value) in [
        (4, 1),
        (5, 2),
        (7, 3),
        (16, 3),
        (18, 3),
        (52, 63),
        (54, 55),
        (56, 0),
        (56, 9),
    ] {
        let mut bytes = executable();
        bytes[offset] = value;
        assert_eq!(Image::parse(&bytes).err(), Some(Error::Header));
    }
    let mut bytes = executable();
    put_u64(&mut bytes, 32, u64::MAX);
    assert_eq!(Image::parse(&bytes).err(), Some(Error::Header));
}
#[test]
fn invalid_ranges_wx_and_dynamic_segments_are_rejected() {
    for (offset, value, error) in [
        (72, u64::MAX, Error::Segment),
        (80, BASE - 4096, Error::Segment),
        (80, BASE + 1, Error::Segment),
        (96, 4097, Error::Segment),
        (104, u64::MAX, Error::Segment),
        (112, 3, Error::Segment),
        (24, BASE + 4096, Error::Entry),
    ] {
        let mut bytes = executable();
        put_u64(&mut bytes, offset, value);
        assert_eq!(Image::parse(&bytes).err(), Some(error));
    }
    for flags in [0, 1, 2, 7, 0xff] {
        let mut bytes = executable();
        put_u32(&mut bytes, 68, flags);
        assert_eq!(Image::parse(&bytes).err(), Some(Error::Permissions));
    }
    for kind in [2, 3, 7] {
        let mut bytes = executable();
        put_u32(&mut bytes, 64, kind);
        assert_eq!(Image::parse(&bytes).err(), Some(Error::Unsupported));
    }
    let mut bytes = executable();
    bytes[56] = 2;
    let first: [u8; 56] = bytes[64..120].try_into().unwrap();
    bytes[120..176].copy_from_slice(&first);
    assert_eq!(Image::parse(&bytes).err(), Some(Error::Segment));
}
