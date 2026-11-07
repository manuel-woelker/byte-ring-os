//! File service boundary. Handles are resolved in the caller's own context.

/// The initially granted stdout handle; its destination is chosen by the host.
pub const STDOUT: u64 = 0;

/// File-service failures stop the current bytecode program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteError {
    BadHandle,
    Denied,
    Io,
}

/// A statically dispatched file service, responsible for checking handle
/// ownership, write rights, and bounded device work on every call.
pub trait FileWriter {
    /// Write raw bytes without newline conversion. Return the accepted byte
    /// count, at most `bytes.len()`. A failure may have externally visible
    /// partial effects. The VM does not retry short or failed writes.
    fn write(&mut self, handle: u64, bytes: &[u8]) -> Result<usize, WriteError>;
}
