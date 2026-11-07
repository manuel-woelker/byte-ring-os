//! Version-one bytecode encoding and checked sequential decoding.

/// Encoding version carried by the surrounding submission header.
pub const VERSION: u16 = 1;
pub const RETURN: u8 = 0x01;
pub const WRITE: u8 = 0x02;
pub const IMMEDIATE: u8 = 0x01;
pub const INLINE: u8 = 0x02;
/// Reserved operand mode; rejected until buffer ownership is implemented.
pub const MEMORY_REFERENCE: u8 = 0x03;

/// A structural failure and its byte offset when applicable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    Truncated { offset: usize },
    UnknownOpcode { offset: usize, opcode: u8 },
    InvalidOperandMode { offset: usize, mode: u8 },
    UnsupportedMemoryReference { offset: usize },
    MissingReturn,
    TrailingBytes { offset: usize },
}

pub(crate) enum Instruction<'a> {
    Return(u64),
    Write { handle: u64, bytes: &'a [u8] },
}

pub(crate) struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(crate) fn offset(&self) -> usize {
        self.offset
    }

    pub(crate) fn next(&mut self) -> Result<Option<Instruction<'a>>, DecodeError> {
        if self.offset == self.bytes.len() {
            return Ok(None);
        }
        let offset = self.offset;
        match self.take::<1>()?[0] {
            RETURN => Ok(Some(Instruction::Return(self.immediate()?))),
            WRITE => {
                let handle = self.immediate()?;
                self.mode(INLINE)?;
                let length = u32::from_le_bytes(self.take::<4>()?) as usize;
                let bytes = self.slice(length)?;
                Ok(Some(Instruction::Write { handle, bytes }))
            }
            opcode => Err(DecodeError::UnknownOpcode { offset, opcode }),
        }
    }

    fn immediate(&mut self) -> Result<u64, DecodeError> {
        self.mode(IMMEDIATE)?;
        Ok(u64::from_le_bytes(self.take::<8>()?))
    }

    fn mode(&mut self, expected: u8) -> Result<(), DecodeError> {
        let offset = self.offset;
        let mode = self.take::<1>()?[0];
        if mode == MEMORY_REFERENCE {
            return Err(DecodeError::UnsupportedMemoryReference { offset });
        }
        if mode != expected {
            return Err(DecodeError::InvalidOperandMode { offset, mode });
        }
        Ok(())
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let mut value = [0; N];
        value.copy_from_slice(self.slice(N)?);
        Ok(value)
    }

    fn slice(&mut self, length: usize) -> Result<&'a [u8], DecodeError> {
        let error = DecodeError::Truncated {
            offset: self.offset,
        };
        let end = self.offset.checked_add(length).ok_or(error)?;
        let value = self.bytes.get(self.offset..end).ok_or(error)?;
        self.offset = end;
        Ok(value)
    }
}
