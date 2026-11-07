extern crate std;

use std::{vec, vec::Vec};

use crate::file::{FileWriter, WriteError};

use super::builder::{EncodeError, ProgramBuilder};
use super::instruction::{DecodeError, IMMEDIATE, INLINE, MEMORY_REFERENCE, RETURN, WRITE};
use super::vm::{Error, Limit, Limits, execute};

#[derive(Default)]
struct Files {
    writes: Vec<(u64, Vec<u8>)>,
    fail_handle: Option<u64>,
    response_count: Option<usize>,
}

impl FileWriter for Files {
    fn write(&mut self, handle: u64, bytes: &[u8]) -> Result<usize, WriteError> {
        if self.fail_handle == Some(handle) {
            return Err(WriteError::BadHandle);
        }
        self.writes.push((handle, bytes.to_vec()));
        Ok(self.response_count.unwrap_or(bytes.len()))
    }
}

fn program(writes: &[(u64, &[u8])], value: u64) -> Vec<u8> {
    let mut storage = [0; 4096];
    let mut builder = ProgramBuilder::new(&mut storage);
    for &(handle, bytes) in writes {
        builder.write(handle, bytes).unwrap();
    }
    builder.return_imm(value).unwrap();
    builder.finish().unwrap().to_vec()
}

#[test]
fn return_immediate_has_an_explicit_little_endian_encoding() {
    let bytes = program(&[], 0x0807_0605_0403_0201);
    assert_eq!(bytes, [RETURN, IMMEDIATE, 1, 2, 3, 4, 5, 6, 7, 8]);
    let mut files = Files::default();
    assert_eq!(
        execute(&bytes, &mut files, Limits::default()),
        Ok(0x0807_0605_0403_0201)
    );
    assert!(files.writes.is_empty());
}

#[test]
fn write_encoding_includes_handle_mode_and_inline_length() {
    let bytes = program(&[(0, b"hi")], 0);
    assert_eq!(
        &bytes[..17],
        &[
            WRITE, IMMEDIATE, 0, 0, 0, 0, 0, 0, 0, 0, INLINE, 2, 0, 0, 0, b'h', b'i'
        ]
    );
}

#[test]
fn writes_stdout_and_other_file_handles_verbatim_in_order() {
    let payload = [0, WRITE, RETURN, IMMEDIATE, INLINE, MEMORY_REFERENCE, 0xff];
    let bytes = program(&[(0, &payload), (42, b"hello\n"), (u64::MAX, b"")], 123);
    let mut files = Files::default();
    assert_eq!(execute(&bytes, &mut files, Limits::default()), Ok(123));
    assert_eq!(
        files.writes,
        [
            (0, payload.to_vec()),
            (42, b"hello\n".to_vec()),
            (u64::MAX, vec![])
        ]
    );
}

#[test]
fn rejects_every_truncated_prefix_before_any_file_effect() {
    let bytes = program(&[(0, b"hello")], 123);
    for length in 0..bytes.len() {
        let mut files = Files::default();
        assert!(
            execute(&bytes[..length], &mut files, Limits::default()).is_err(),
            "prefix length {length}"
        );
        assert!(files.writes.is_empty());
    }
}

#[test]
fn verifies_trailing_instructions_before_writing() {
    let mut bytes = program(&[(0, b"must not be printed")], 0);
    bytes.push(0xff);
    let mut files = Files::default();
    assert!(matches!(
        execute(&bytes, &mut files, Limits::default()),
        Err(Error::InvalidProgram(DecodeError::TrailingBytes { .. }))
    ));
    assert!(files.writes.is_empty());
}

#[test]
fn unknown_opcode_after_a_write_has_no_side_effects() {
    let mut bytes = program(&[(0, b"must not be printed")], 0);
    let return_offset = bytes.len() - 10;
    bytes[return_offset] = 0xff;
    let mut files = Files::default();
    assert_eq!(
        execute(&bytes, &mut files, Limits::default()),
        Err(Error::InvalidProgram(DecodeError::UnknownOpcode {
            offset: return_offset,
            opcode: 0xff
        }))
    );
    assert!(files.writes.is_empty());
}

#[test]
fn missing_return_is_not_an_implicit_success() {
    let mut bytes = program(&[(0, b"x")], 0);
    bytes.truncate(bytes.len() - 10);
    let mut files = Files::default();
    assert_eq!(
        execute(&bytes, &mut files, Limits::default()),
        Err(Error::InvalidProgram(DecodeError::MissingReturn))
    );
    assert!(files.writes.is_empty());
}

#[test]
fn rejects_wrong_operand_types_and_reserved_memory_references() {
    for (offset, mode) in [
        (1, INLINE),
        (10, IMMEDIATE),
        (1, MEMORY_REFERENCE),
        (10, MEMORY_REFERENCE),
    ] {
        let mut bytes = program(&[(0, b"x")], 0);
        bytes[offset] = mode;
        let mut files = Files::default();
        let expected = if mode == MEMORY_REFERENCE {
            DecodeError::UnsupportedMemoryReference { offset }
        } else {
            DecodeError::InvalidOperandMode { offset, mode }
        };
        assert_eq!(
            execute(&bytes, &mut files, Limits::default()),
            Err(Error::InvalidProgram(expected))
        );
        assert!(files.writes.is_empty());
    }
}

#[test]
fn refuses_lengths_that_run_past_the_program() {
    let mut bytes = program(&[(0, b"x")], 0);
    bytes[11..15].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut files = Files::default();
    assert_eq!(
        execute(&bytes, &mut files, Limits::default()),
        Err(Error::InvalidProgram(DecodeError::Truncated { offset: 15 }))
    );
    assert!(files.writes.is_empty());
}

#[test]
fn enforces_all_resource_limits_before_effects() {
    let bytes = program(&[(0, b"abc"), (0, b"def")], 1);
    let defaults = Limits::default();
    for (limits, expected) in [
        (
            Limits {
                program_bytes: bytes.len() - 1,
                ..defaults
            },
            Limit::ProgramBytes,
        ),
        (
            Limits {
                instructions: 2,
                ..defaults
            },
            Limit::Instructions,
        ),
        (
            Limits {
                instructions: 0,
                ..defaults
            },
            Limit::Instructions,
        ),
        (
            Limits {
                output_bytes: 5,
                ..defaults
            },
            Limit::OutputBytes,
        ),
    ] {
        let mut files = Files::default();
        assert_eq!(
            execute(&bytes, &mut files, limits),
            Err(Error::LimitExceeded(expected))
        );
        assert!(files.writes.is_empty());
    }
    let mut files = Files::default();
    assert_eq!(
        execute(
            &bytes,
            &mut files,
            Limits {
                program_bytes: bytes.len(),
                instructions: 3,
                output_bytes: 6
            }
        ),
        Ok(1)
    );
}

#[test]
fn no_write_budget_is_needed_for_an_immediate_return() {
    let mut files = Files::default();
    assert_eq!(
        execute(
            &program(&[], 7),
            &mut files,
            Limits {
                output_bytes: 0,
                ..Limits::default()
            }
        ),
        Ok(7)
    );
}

#[test]
fn handle_errors_stop_execution_but_do_not_undo_previous_writes() {
    let bytes = program(&[(0, b"first"), (99, b"denied"), (0, b"never")], 0);
    let mut files = Files {
        fail_handle: Some(99),
        ..Files::default()
    };
    assert_eq!(
        execute(&bytes, &mut files, Limits::default()),
        Err(Error::File(WriteError::BadHandle))
    );
    assert_eq!(files.writes, [(0, b"first".to_vec())]);
}

#[test]
fn short_writes_are_not_retried() {
    let mut files = Files {
        response_count: Some(2),
        ..Files::default()
    };
    assert_eq!(
        execute(
            &program(&[(0, b"hello"), (0, b"never")], 0),
            &mut files,
            Limits::default()
        ),
        Err(Error::ShortWrite {
            written: 2,
            expected: 5
        })
    );
    assert_eq!(files.writes.len(), 1);
}

#[test]
fn an_impossible_service_byte_count_is_an_error() {
    let mut files = Files {
        response_count: Some(6),
        ..Files::default()
    };
    assert_eq!(
        execute(&program(&[(0, b"hello")], 0), &mut files, Limits::default()),
        Err(Error::InvalidWriteCount)
    );
}

#[test]
fn file_denial_and_io_failures_propagate_without_panics() {
    struct FailingFile(WriteError);
    impl FileWriter for FailingFile {
        fn write(&mut self, _handle: u64, _bytes: &[u8]) -> Result<usize, WriteError> {
            Err(self.0)
        }
    }
    for failure in [WriteError::Denied, WriteError::Io] {
        assert_eq!(
            execute(
                &program(&[(0, b"x")], 0),
                &mut FailingFile(failure),
                Limits::default()
            ),
            Err(Error::File(failure))
        );
    }
}

#[test]
fn builder_failures_leave_room_for_a_valid_retry() {
    let mut storage = [0; 10];
    let mut builder = ProgramBuilder::new(&mut storage);
    assert_eq!(builder.write(0, b"too big"), Err(EncodeError::NoSpace));
    builder.return_imm(123).unwrap();
    assert_eq!(builder.write(0, b""), Err(EncodeError::AfterReturn));
    assert_eq!(builder.return_imm(0), Err(EncodeError::AfterReturn));
    let mut files = Files::default();
    assert_eq!(
        execute(builder.finish().unwrap(), &mut files, Limits::default()),
        Ok(123)
    );
}

#[test]
fn builder_requires_a_return_and_does_not_truncate_it() {
    let mut empty = [];
    assert_eq!(
        ProgramBuilder::new(&mut empty).finish(),
        Err(EncodeError::MissingReturn)
    );
    let mut small = [0; 9];
    let mut builder = ProgramBuilder::new(&mut small);
    assert_eq!(builder.return_imm(1), Err(EncodeError::NoSpace));
    assert_eq!(builder.finish(), Err(EncodeError::MissingReturn));
}
