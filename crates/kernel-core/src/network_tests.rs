extern crate std;
use super::*;
use std::vec::Vec;
const MAC: [u8; 6] = [0x52, 0x54, 0, 0x12, 0x34, 0x56];
const PEER: [u8; 6] = [2, 0, 0, 0, 0, 1];
fn net() -> Network {
    Network {
        ip: [192, 0, 2, 2],
        mac: MAC,
    }
}
fn request(payload: &[u8]) -> Vec<u8> {
    let mut bytes = std::vec![0; 42 + payload.len()];
    bytes[..6].copy_from_slice(&MAC);
    bytes[6..12].copy_from_slice(&PEER);
    put(&mut bytes, 12, 0x800);
    bytes[14] = 0x45;
    put(&mut bytes, 16, 28 + payload.len() as u16);
    bytes[22] = 64;
    bytes[23] = 1;
    bytes[26..30].copy_from_slice(&[192, 0, 2, 1]);
    bytes[30..34].copy_from_slice(&[192, 0, 2, 2]);
    bytes[34] = 8;
    put(&mut bytes, 38, 123);
    put(&mut bytes, 40, 7);
    bytes[42..].copy_from_slice(payload);
    fix(&mut bytes);
    bytes
}
fn fix(bytes: &mut [u8]) {
    put(bytes, 24, 0);
    let value = checksum(&bytes[14..34]);
    put(bytes, 24, value);
    put(bytes, 36, 0);
    let value = checksum(&bytes[34..]);
    put(bytes, 36, value);
}
#[test]
fn echo_preserves_payload_and_produces_valid_checksums_and_padding() {
    for payload in [&b""[..], &b"hello"[..], &[0xff; 1472][..]] {
        let bytes = request(payload);
        let mut output = [0xa5; MAX_FRAME];
        let reply = net().respond(&bytes, &mut output).unwrap();
        assert_eq!(reply.echo.unwrap().identifier, 123);
        assert_eq!(reply.echo.unwrap().sequence, 7);
        assert_eq!(&output[..6], &PEER);
        assert_eq!(&output[6..12], &MAC);
        assert_eq!(checksum(&output[14..34]), 0);
        assert_eq!(checksum(&output[34..bytes.len()]), 0);
        assert_eq!(&output[42..bytes.len()], payload);
        assert_eq!(output[34], 0);
        assert!(output[bytes.len()..reply.length].iter().all(|&b| b == 0));
    }
}
#[test]
fn all_truncated_headers_drop_without_panics() {
    let bytes = request(b"hello");
    for length in 0..bytes.len() {
        assert!(
            net()
                .respond(&bytes[..length], &mut [0; MAX_FRAME])
                .is_err()
        );
    }
}
#[test]
fn rejects_bad_checksums_wrong_destinations_fragments_options_and_lengths() {
    for offset in [14, 16, 20, 22, 23, 24, 26, 30, 34, 35, 36] {
        let mut bytes = request(b"hello");
        bytes[offset] ^= 1;
        assert!(
            net().respond(&bytes, &mut [0; MAX_FRAME]).is_err(),
            "offset {offset}"
        );
    }
    for flags in [0x2000, 0x8000, 1] {
        let mut bytes = request(b"hello");
        put(&mut bytes, 20, flags);
        fix(&mut bytes);
        assert_eq!(
            net().respond(&bytes, &mut [0; MAX_FRAME]),
            Err(DropReason::Unsupported)
        );
    }
    let mut bytes = request(b"hello");
    bytes[14] = 0x46;
    fix(&mut bytes);
    assert!(net().respond(&bytes, &mut [0; MAX_FRAME]).is_err());
}
#[test]
fn ignores_ethernet_padding_and_accepts_df() {
    let mut bytes = request(b"odd");
    put(&mut bytes, 20, 0x4000);
    fix(&mut bytes);
    bytes.extend_from_slice(&[0xa5; 32]);
    let mut output = [0; MAX_FRAME];
    let reply = net().respond(&bytes, &mut output).unwrap();
    assert_eq!(reply.echo.unwrap().payload_bytes, 3);
    assert!(output[45..60].iter().all(|&b| b == 0));
}
#[test]
fn arp_answers_only_our_ip_with_zero_padding() {
    let mut bytes = [0; 42];
    bytes[..6].fill(255);
    bytes[6..12].copy_from_slice(&PEER);
    put(&mut bytes, 12, 0x806);
    bytes[14..22].copy_from_slice(&[0, 1, 8, 0, 6, 4, 0, 1]);
    bytes[22..28].copy_from_slice(&PEER);
    bytes[28..32].copy_from_slice(&[192, 0, 2, 1]);
    bytes[38..42].copy_from_slice(&[192, 0, 2, 2]);
    let mut output = [0xa5; 60];
    assert_eq!(net().respond(&bytes, &mut output).unwrap().length, 60);
    assert_eq!(word(&output, 20), 2);
    assert_eq!(&output[22..28], &MAC);
    assert_eq!(&output[28..32], &[192, 0, 2, 2]);
    assert!(output[42..].iter().all(|&b| b == 0));
    for length in 0..42 {
        assert!(net().respond(&bytes[..length], &mut output).is_err());
    }
    bytes[41] = 3;
    assert_eq!(
        net().respond(&bytes, &mut output),
        Err(DropReason::Destination)
    );
}
#[test]
fn output_exhaustion_and_invalid_source_are_drops() {
    let mut bytes = request(b"hello");
    assert_eq!(
        net().respond(&bytes, &mut [0; 59]),
        Err(DropReason::OutputFull)
    );
    bytes[29] = 255;
    fix(&mut bytes);
    assert_eq!(
        net().respond(&bytes, &mut [0; MAX_FRAME]),
        Err(DropReason::Source)
    );
}
