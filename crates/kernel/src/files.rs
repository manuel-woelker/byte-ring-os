//! Initial boot file context. Only stdout is granted, backed by COM1.

use byte_ring_drivers::serial::SerialPort;
use byte_ring_kernel_core::bytecode::builder::ProgramBuilder;
use byte_ring_kernel_core::bytecode::vm::{self, Limits};
use byte_ring_kernel_core::file::{FileWriter, STDOUT, WriteError};

pub struct BootFiles {
    stdout: SerialPort<'static>,
}

impl BootFiles {
    pub fn new(stdout: SerialPort<'static>) -> Self {
        Self { stdout }
    }
}

impl FileWriter for BootFiles {
    fn write(&mut self, handle: u64, bytes: &[u8]) -> Result<usize, WriteError> {
        match handle {
            STDOUT => {
                self.stdout.write_bytes(bytes).map_err(|_| WriteError::Io)?;
                Ok(bytes.len())
            }
            _ => Err(WriteError::BadHandle),
        }
    }
}

pub fn boot_program(files: &mut BootFiles) -> Result<u64, vm::Error> {
    let mut storage = [0; 128];
    let mut program = ProgramBuilder::new(&mut storage);
    program
        .write(STDOUT, b"Byte Ring OS: COM1 ready\r\n")
        .expect("boot WRITE fits its fixed program buffer");
    program.return_imm(123).expect("boot RETURN fits");
    vm::execute(
        program.finish().expect("boot program ends in RETURN"),
        files,
        Limits::default(),
    )
}

#[cfg(any(feature = "network", feature = "userspace"))]
use core::fmt::{self, Write};
#[cfg(any(feature = "network", feature = "userspace"))]
struct Text {
    bytes: [u8; 256],
    length: usize,
}
#[cfg(any(feature = "network", feature = "userspace"))]
impl Write for Text {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.length.checked_add(text.len()).ok_or(fmt::Error)?;
        let destination = self.bytes.get_mut(self.length..end).ok_or(fmt::Error)?;
        destination.copy_from_slice(text.as_bytes());
        self.length = end;
        Ok(())
    }
}
#[cfg(any(feature = "network", feature = "userspace"))]
pub fn log(files: &mut BootFiles, args: fmt::Arguments<'_>) {
    let mut text = Text {
        bytes: [0; 256],
        length: 0,
    };
    if text.write_fmt(args).is_err() {
        return;
    }
    let mut bytes = [0; 300];
    let mut builder = ProgramBuilder::new(&mut bytes);
    if builder.write(STDOUT, &text.bytes[..text.length]).is_err() || builder.return_imm(0).is_err()
    {
        return;
    }
    if let Ok(program) = builder.finish() {
        let _ = vm::execute(program, files, vm::Limits::default());
    }
}
