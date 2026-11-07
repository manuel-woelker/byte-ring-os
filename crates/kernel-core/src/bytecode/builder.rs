//! Allocation-free encoding into caller-provided storage.

use super::instruction::{IMMEDIATE, INLINE, RETURN, WRITE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    NoSpace,
    LengthOverflow,
    AfterReturn,
    MissingReturn,
}

/// Builds a straight-line program ending in exactly one RETURN. Failed append
/// operations leave existing encoded bytes unchanged.
pub struct ProgramBuilder<'a> {
    storage: &'a mut [u8],
    length: usize,
    returned: bool,
}

impl<'a> ProgramBuilder<'a> {
    pub fn new(storage: &'a mut [u8]) -> Self {
        Self {
            storage,
            length: 0,
            returned: false,
        }
    }

    /// Append WRITE with an immediate file handle and inline byte payload.
    pub fn write(&mut self, handle: u64, bytes: &[u8]) -> Result<(), EncodeError> {
        let length = u32::try_from(bytes.len()).map_err(|_| EncodeError::LengthOverflow)?;
        self.append(&[
            &[WRITE, IMMEDIATE],
            &handle.to_le_bytes(),
            &[INLINE],
            &length.to_le_bytes(),
            bytes,
        ])
    }

    /// Append the terminal RETURN with its immediate completion value.
    pub fn return_imm(&mut self, value: u64) -> Result<(), EncodeError> {
        self.append(&[&[RETURN, IMMEDIATE], &value.to_le_bytes()])?;
        self.returned = true;
        Ok(())
    }

    /// Finish encoding. Limits are enforced by the verifier, not the encoder.
    pub fn finish(self) -> Result<&'a [u8], EncodeError> {
        if !self.returned {
            return Err(EncodeError::MissingReturn);
        }
        Ok(&self.storage[..self.length])
    }

    fn append(&mut self, parts: &[&[u8]]) -> Result<(), EncodeError> {
        if self.returned {
            return Err(EncodeError::AfterReturn);
        }
        let mut end = self.length;
        for part in parts {
            end = end
                .checked_add(part.len())
                .ok_or(EncodeError::LengthOverflow)?;
        }
        if end > self.storage.len() {
            return Err(EncodeError::NoSpace);
        }
        for part in parts {
            self.storage[self.length..self.length + part.len()].copy_from_slice(part);
            self.length += part.len();
        }
        Ok(())
    }
}
