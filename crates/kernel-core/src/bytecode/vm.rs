//! Whole-program verification followed by bounded, straight-line execution.

use crate::file::{FileWriter, WriteError};
use crate::limits::DEFAULT_INSTRUCTION_BUDGET;

use super::instruction::{DecodeError, Decoder, Instruction};

/// Trusted kernel-selected bounds, never taken unchecked from a submission.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub program_bytes: usize,
    pub instructions: u32,
    pub output_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            program_bytes: 4096,
            instructions: DEFAULT_INSTRUCTION_BUDGET,
            output_bytes: 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    ProgramBytes,
    Instructions,
    OutputBytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidProgram(DecodeError),
    LimitExceeded(Limit),
    File(WriteError),
    ShortWrite { written: usize, expected: usize },
    InvalidWriteCount,
}

/// Execute version-one bytecode from an immutable, kernel-owned copy.
///
/// Verify the entire stream before any file service call. Every program must
/// end with RETURN, without trailing bytes. Each WRITE must accept its complete
/// payload; short writes and file errors stop execution without retry or rollback.
/// There are no registers, jumps, user pointers, or memory-reference operands yet.
/// The caller must copy untrusted shared memory before constructing this slice.
pub fn execute(program: &[u8], files: &mut impl FileWriter, limits: Limits) -> Result<u64, Error> {
    verify(program, limits)?;
    let mut decoder = Decoder::new(program);
    while let Some(instruction) = decoder.next().map_err(Error::InvalidProgram)? {
        match instruction {
            Instruction::Return(value) => return Ok(value),
            Instruction::Write { handle, bytes } => {
                let written = files.write(handle, bytes).map_err(Error::File)?;
                if written > bytes.len() {
                    return Err(Error::InvalidWriteCount);
                }
                if written != bytes.len() {
                    return Err(Error::ShortWrite {
                        written,
                        expected: bytes.len(),
                    });
                }
            }
        }
    }
    Err(Error::InvalidProgram(DecodeError::MissingReturn))
}

fn verify(program: &[u8], limits: Limits) -> Result<(), Error> {
    if program.len() > limits.program_bytes {
        return Err(Error::LimitExceeded(Limit::ProgramBytes));
    }
    let mut decoder = Decoder::new(program);
    let mut instructions = 0u32;
    let mut output_bytes = 0usize;
    while let Some(instruction) = decoder.next().map_err(Error::InvalidProgram)? {
        instructions = instructions
            .checked_add(1)
            .ok_or(Error::LimitExceeded(Limit::Instructions))?;
        if instructions > limits.instructions {
            return Err(Error::LimitExceeded(Limit::Instructions));
        }
        match instruction {
            Instruction::Return(_) => {
                if decoder.offset() != program.len() {
                    return Err(Error::InvalidProgram(DecodeError::TrailingBytes {
                        offset: decoder.offset(),
                    }));
                }
                return Ok(());
            }
            Instruction::Write { bytes, .. } => {
                output_bytes = output_bytes
                    .checked_add(bytes.len())
                    .ok_or(Error::LimitExceeded(Limit::OutputBytes))?;
                if output_bytes > limits.output_bytes {
                    return Err(Error::LimitExceeded(Limit::OutputBytes));
                }
            }
        }
    }
    Err(Error::InvalidProgram(DecodeError::MissingReturn))
}
