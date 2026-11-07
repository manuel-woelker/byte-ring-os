extern crate std;
use super::*;
use crate::bytecode::builder::ProgramBuilder;
use std::vec::Vec;
struct Shared {
    tail: u64,
    drained: u64,
    head: u64,
    produced: u64,
    sq: [[u8; SUBMISSION_SIZE]; 4],
    cq: [[u8; COMPLETION_SIZE]; 4],
    mutate: bool,
}
impl Default for Shared {
    fn default() -> Self {
        Self {
            tail: 0,
            drained: 0,
            head: 0,
            produced: 0,
            sq: [[0; SUBMISSION_SIZE]; 4],
            cq: [[0; COMPLETION_SIZE]; 4],
            mutate: false,
        }
    }
}
impl Memory for Shared {
    fn submission_tail(&mut self) -> u64 {
        self.tail
    }
    fn completion_tail(&mut self) -> u64 {
        self.drained
    }
    fn copy_submission(&mut self, slot: usize, output: &mut [u8; SUBMISSION_SIZE]) {
        *output = self.sq[slot];
    }
    fn publish_submission_head(&mut self, head: u64) {
        self.head = head;
        if self.mutate {
            self.sq[0].fill(0xff);
            self.tail = 2;
        }
    }
    fn write_completion(&mut self, slot: usize, bytes: &[u8; COMPLETION_SIZE]) {
        self.cq[slot] = *bytes;
    }
    fn publish_completion_head(&mut self, head: u64) {
        self.produced = head;
    }
}
#[derive(Default)]
struct Files(Vec<u8>);
impl FileWriter for Files {
    fn write(&mut self, handle: u64, bytes: &[u8]) -> Result<usize, WriteError> {
        if handle != 0 {
            return Err(WriteError::BadHandle);
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
}
fn request(shared: &mut Shared, sequence: u64, handle: Option<u64>) {
    let mut bytes = [0; MAX_PROGRAM];
    let mut program = ProgramBuilder::new(&mut bytes);
    if let Some(handle) = handle {
        program.write(handle, b"hello").unwrap();
    }
    program.return_imm(123).unwrap();
    assert!(submission(
        77,
        program.finish().unwrap(),
        &mut shared.sq[(sequence & 3) as usize]
    ));
}
#[test]
fn wraparound_order_correlation_and_cq_backpressure() {
    let start = u64::MAX - 1;
    let mut pair = Pair {
        submission_head: start,
        submission_seen: start,
        completion_head: start,
        completion_seen: start,
        disabled: false,
    };
    let mut memory = Shared {
        tail: start,
        drained: start,
        ..Default::default()
    };
    let mut files = Files::default();
    for n in 0..4 {
        request(&mut memory, start.wrapping_add(n), Some(0));
    }
    memory.tail = start.wrapping_add(4);
    assert_eq!(
        pair.enter(&mut memory, &mut files, 2),
        (2, Stop::BatchLimit)
    );
    assert_eq!(pair.enter(&mut memory, &mut files, 4), (2, Stop::Idle));
    assert_eq!(memory.produced, 2);
    for n in 0..4 {
        let sequence = start.wrapping_add(n);
        let cq = &memory.cq[(sequence & 3) as usize];
        assert_eq!(read_u64(cq, 0), sequence);
        assert_eq!(read_u64(cq, 8), 77);
        assert_eq!(read_u64(cq, 24), 123);
    }
    request(&mut memory, 2, Some(0));
    memory.tail = 3;
    assert_eq!(
        pair.enter(&mut memory, &mut files, 4),
        (0, Stop::CompletionFull)
    );
    assert_eq!(memory.head, 2);
    memory.drained = 2;
    assert_eq!(
        pair.enter(&mut memory, &mut files, u64::MAX),
        (1, Stop::Idle)
    );
    assert_eq!(files.0.len(), 25);
}
#[test]
fn hostile_cursor_movement_disables_pair_and_bad_arguments_consume_nothing() {
    let mut pair = Pair::default();
    let mut memory = Shared::default();
    let mut files = Files::default();
    assert_eq!(
        pair.enter(&mut memory, &mut files, 0),
        (0, Stop::BadArgument)
    );
    memory.tail = 5;
    assert_eq!(
        pair.enter(&mut memory, &mut files, 4),
        (0, Stop::QueueFault)
    );
    memory.tail = 0;
    assert_eq!(
        pair.enter(&mut memory, &mut files, 4),
        (0, Stop::QueueFault)
    );
    for bad_tail in [u64::MAX, 1] {
        let mut pair = Pair::default();
        let mut memory = Shared {
            drained: bad_tail,
            ..Default::default()
        };
        assert_eq!(
            pair.enter(&mut memory, &mut files, 1),
            (0, Stop::QueueFault)
        );
    }
    let mut pair = Pair {
        submission_seen: 2,
        ..Default::default()
    };
    let mut memory = Shared {
        tail: 1,
        ..Default::default()
    };
    assert_eq!(
        pair.enter(&mut memory, &mut files, 1),
        (0, Stop::QueueFault)
    );
}
#[test]
fn copied_program_and_entry_snapshot_survive_user_mutation() {
    let mut pair = Pair::default();
    let mut memory = Shared {
        tail: 1,
        mutate: true,
        ..Default::default()
    };
    request(&mut memory, 0, Some(0));
    request(&mut memory, 1, None);
    let mut files = Files::default();
    assert_eq!(pair.enter(&mut memory, &mut files, 4), (1, Stop::Idle));
    assert_eq!(read_u32(&memory.cq[0], 16), Status::Success as u32);
    assert_eq!(read_u64(&memory.cq[0], 24), 123);
    assert_eq!(files.0, b"hello");
    assert_eq!(pair.enter(&mut memory, &mut files, 4), (1, Stop::Idle));
}
#[test]
fn rejected_programs_complete_without_ungranted_or_invalid_effects() {
    let mut pair = Pair::default();
    let mut memory = Shared {
        tail: 4,
        ..Default::default()
    };
    for n in 0..4 {
        request(&mut memory, n, Some(1));
    }
    memory.sq[1][8] = 2;
    memory.sq[2][16] = 0xff;
    put_u32(&mut memory.sq[3], 12, u32::MAX);
    let mut files = Files::default();
    assert_eq!(pair.enter(&mut memory, &mut files, 4), (4, Stop::Idle));
    for n in 0..4 {
        assert_eq!(
            read_u32(&memory.cq[n], 16),
            if n == 0 {
                Status::Denied
            } else {
                Status::InvalidProgram
            } as u32
        );
    }
    assert!(files.0.is_empty());
}
