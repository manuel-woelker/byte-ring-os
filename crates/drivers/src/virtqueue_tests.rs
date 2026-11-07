extern crate std;
use super::*;
use byte_ring_hal::memory::Memory;
use std::vec;
fn queue(receive: bool) -> Queue<'static> {
    // Host-only leaked test storage has no device access and is test-local.
    let bytes = std::boxed::Box::leak(vec![0; DMA_SIZE].into_boxed_slice());
    Queue::new(Memory::for_test(bytes, 0x100000), 32, receive).unwrap()
}
fn complete(queue: &mut Queue<'_>, id: u32, length: u32) {
    let slot = USED + 4 + (queue.used % queue.size) as usize * 8;
    queue.memory.write_u32(slot, id);
    queue.memory.write_u32(slot + 4, length);
    queue.memory.write_u16(USED + 2, queue.used.wrapping_add(1));
}
#[test]
fn tx_exhaustion_completion_and_reuse() {
    let mut q = queue(false);
    for _ in 0..32 {
        q.transmit(b"packet").unwrap();
    }
    assert_eq!(q.transmit(b"full"), Err(Error::Full));
    complete(&mut q, 3, 0);
    q.transmit(b"reused").unwrap();
    assert!(q.owned[3]);
    assert_eq!(q.used, 1);
}
#[test]
fn rx_owns_buffers_until_completion_and_reposts_after_copy() {
    let mut q = queue(true);
    let offset = BUFFERS;
    q.memory.write_u16(offset + 10, 1);
    q.memory.write_bytes(offset + 12, b"hello");
    complete(&mut q, 0, 17);
    let mut output = [0; 32];
    assert_eq!(q.receive(&mut output), Ok(Some(5)));
    assert_eq!(&output[..5], b"hello");
    assert!(q.owned[0]);
    assert_eq!(q.available, 33);
    assert_eq!(q.receive(&mut output), Ok(None));
}
#[test]
fn corrupt_ids_lengths_and_duplicate_completions_poison_the_queue() {
    for (id, length) in [(32, 0), (0, 2049)] {
        let mut q = queue(false);
        q.transmit(b"hello").unwrap();
        complete(&mut q, id, length);
        assert_eq!(q.reclaim(), Err(Error::Corrupt));
        assert_eq!(q.transmit(b"again"), Err(Error::Corrupt));
    }
    let mut q = queue(false);
    q.transmit(b"hello").unwrap();
    complete(&mut q, 0, 0);
    q.reclaim().unwrap();
    complete(&mut q, 0, 0);
    assert_eq!(q.reclaim(), Err(Error::Corrupt));
}
#[test]
fn indices_wrap_without_reusing_an_owned_buffer() {
    let mut q = queue(false);
    q.available = u16::MAX;
    q.used = u16::MAX;
    q.memory.write_u16(USED + 2, u16::MAX);
    q.transmit(b"x").unwrap();
    assert_eq!(q.available, 0);
    complete(&mut q, 0, 0);
    q.reclaim().unwrap();
    assert_eq!(q.used, 0);
    q.transmit(b"next").unwrap();
}
#[test]
fn invalid_rx_header_is_dropped_and_buffer_recycled() {
    let mut q = queue(true);
    q.memory.write_u8(BUFFERS + 1, 1);
    complete(&mut q, 0, 12);
    assert_eq!(q.receive(&mut [0; 32]), Ok(Some(0)));
    assert!(q.owned[0]);
    complete(&mut q, 1, 11);
    assert_eq!(q.receive(&mut [0; 32]), Err(Error::Corrupt));
}
#[test]
fn invalid_sizes_and_completion_advances_are_rejected() {
    let mut bytes = vec![0; DMA_SIZE];
    assert!(Queue::new(Memory::for_test(&mut bytes, 0x100000), 3, false).is_err());
    let mut q = queue(false);
    q.memory.write_u16(USED + 2, 33);
    assert_eq!(q.reclaim(), Err(Error::Corrupt));
}
