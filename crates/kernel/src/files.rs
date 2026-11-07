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
