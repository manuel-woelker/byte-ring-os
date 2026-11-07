//! Bounded synchronous SQ/CQ processing with private cursor authority.
use crate::{
    bytecode::vm::{self, Limits},
    file::{FileWriter, WriteError},
    user_abi::*,
};

#[repr(u64)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Stop {
    Idle = 0,
    BatchLimit = 1,
    CompletionFull = 2,
    Busy = 3,
    QueueFault = 4,
    BadArgument = 5,
}
#[repr(u32)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Status {
    Success = 0,
    InvalidProgram = 1,
    Denied = 2,
    LimitExceeded = 3,
    ServiceError = 4,
    VmTrap = 5,
}

/// Implementations acquire-load user counters and release-publish kernel ones.
/// They copy only kernel-bound slot addresses, never pointers from submissions.
pub trait Memory {
    fn submission_tail(&mut self) -> u64;
    fn completion_tail(&mut self) -> u64;
    fn copy_submission(&mut self, slot: usize, output: &mut [u8; SUBMISSION_SIZE]);
    fn publish_submission_head(&mut self, head: u64);
    fn write_completion(&mut self, slot: usize, bytes: &[u8; COMPLETION_SIZE]);
    fn publish_completion_head(&mut self, head: u64);
}

#[derive(Default)]
pub struct Pair {
    submission_head: u64,
    submission_seen: u64,
    completion_head: u64,
    completion_seen: u64,
    disabled: bool,
}
impl Pair {
    pub fn enter(
        &mut self,
        memory: &mut impl Memory,
        files: &mut impl FileWriter,
        requested: u64,
    ) -> (u64, Stop) {
        if requested == 0 {
            return (0, Stop::BadArgument);
        }
        if self.disabled {
            return (0, Stop::QueueFault);
        }
        let tail = memory.submission_tail();
        let drained = memory.completion_tail();
        let outstanding = self.submission_seen.wrapping_sub(self.submission_head);
        let completions = self.completion_head.wrapping_sub(self.completion_seen);
        if tail.wrapping_sub(self.submission_seen) > CAPACITY - outstanding
            || drained.wrapping_sub(self.completion_seen) > completions
        {
            self.disabled = true;
            return (0, Stop::QueueFault);
        }
        self.submission_seen = tail;
        self.completion_seen = drained;
        let limit = requested.min(MAX_BATCH);
        let mut count = 0;
        while self.submission_head != tail {
            if self.completion_head.wrapping_sub(drained) == CAPACITY {
                return (count, Stop::CompletionFull);
            }
            if count == limit {
                return (count, Stop::BatchLimit);
            }
            let sequence = self.submission_head;
            let mut copy = [0; SUBMISSION_SIZE];
            memory.copy_submission((sequence & (CAPACITY - 1)) as usize, &mut copy);
            self.submission_head = self.submission_head.wrapping_add(1);
            memory.publish_submission_head(self.submission_head);
            let id = read_u64(&copy, 0);
            let length = read_u32(&copy, 12) as usize;
            let (status, result) = if copy[8..12] != [1, 0, 0, 0] || length > MAX_PROGRAM {
                (Status::InvalidProgram, 0)
            } else {
                match vm::execute(
                    &copy[16..16 + length],
                    files,
                    Limits {
                        program_bytes: MAX_PROGRAM,
                        instructions: MAX_INSTRUCTIONS,
                        output_bytes: MAX_OUTPUT,
                    },
                ) {
                    Ok(value) => (Status::Success, value),
                    Err(vm::Error::InvalidProgram(_)) => (Status::InvalidProgram, 0),
                    Err(vm::Error::LimitExceeded(_)) => (Status::LimitExceeded, 0),
                    Err(vm::Error::File(WriteError::BadHandle | WriteError::Denied)) => {
                        (Status::Denied, 0)
                    }
                    Err(vm::Error::File(WriteError::Io) | vm::Error::ShortWrite { .. }) => {
                        (Status::ServiceError, 0)
                    }
                    Err(vm::Error::InvalidWriteCount) => (Status::VmTrap, 0),
                }
            };
            let mut completion = [0; COMPLETION_SIZE];
            put_u64(&mut completion, 0, sequence);
            put_u64(&mut completion, 8, id);
            put_u32(&mut completion, 16, status as u32);
            put_u64(&mut completion, 24, result);
            memory.write_completion(
                (self.completion_head & (CAPACITY - 1)) as usize,
                &completion,
            );
            self.completion_head = self.completion_head.wrapping_add(1);
            memory.publish_completion_head(self.completion_head);
            count += 1;
        }
        (count, Stop::Idle)
    }
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
