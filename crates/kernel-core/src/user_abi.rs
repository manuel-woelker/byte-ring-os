//! Experimental version-one process bootstrap and queue wire constants.
pub const PAGE: usize = 4096;
pub const BASE: u64 = 0x4000_0000;
pub const IMAGE_PAGES: usize = 32;
pub const STACK: u64 = BASE + 0x30000;
pub const STACK_PAGES: usize = 4;
pub const STACK_END: u64 = STACK + (STACK_PAGES * PAGE) as u64;
pub const BOOT: u64 = BASE + 0x40000;
pub const SQ: u64 = BASE + 0x41000;
pub const SQ_HEAD: u64 = BASE + 0x42000;
pub const CQ: u64 = BASE + 0x43000;
pub const CQ_CONSUMER: u64 = BASE + 0x44000;
pub const CAPACITY: u64 = 4;
pub const MAX_PROGRAM: usize = 256;
pub const MAX_BATCH: u64 = 4;
pub const MAX_INSTRUCTIONS: u32 = 64;
pub const MAX_OUTPUT: usize = 128;
pub const ENTRIES: usize = 64;
pub const SUBMISSION_SIZE: usize = 16 + MAX_PROGRAM;
pub const COMPLETION_SIZE: usize = 32;
pub const USER_CS: u64 = 0x33;
pub const USER_SS: u64 = 0x2b;

pub fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("fixed ABI field"),
    )
}
pub fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed ABI field"),
    )
}
pub fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
pub fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Fixed, pointer-free codec. The owner supplies a buffer of the advertised size.
pub fn submission(id: u64, program: &[u8], out: &mut [u8; SUBMISSION_SIZE]) -> bool {
    if program.len() > MAX_PROGRAM {
        return false;
    }
    out.fill(0);
    put_u64(out, 0, id);
    out[8..10].copy_from_slice(&1u16.to_le_bytes());
    put_u32(out, 12, program.len() as u32);
    out[16..16 + program.len()].copy_from_slice(program);
    true
}

/// Immutable descriptor passed in RDI. RSI is an opaque initial argument.
pub fn bootstrap() -> [u8; 128] {
    let mut bytes = [0; 128];
    bytes[..8].copy_from_slice(b"BRUSR001");
    put_u32(&mut bytes, 8, 1);
    put_u32(&mut bytes, 12, PAGE as u32);
    for (offset, value) in [
        (16, SQ),
        (24, SQ_HEAD),
        (32, CQ),
        (40, CQ_CONSUMER),
        (64, BASE),
        (72, STACK),
        (80, STACK_END),
        (88, 0),
    ] {
        put_u64(&mut bytes, offset, value);
    }
    put_u32(&mut bytes, 48, CAPACITY as u32);
    put_u32(&mut bytes, 52, MAX_PROGRAM as u32);
    put_u32(&mut bytes, 56, MAX_BATCH as u32);
    put_u32(&mut bytes, 96, MAX_INSTRUCTIONS);
    put_u32(&mut bytes, 100, MAX_OUTPUT as u32);
    bytes
}
