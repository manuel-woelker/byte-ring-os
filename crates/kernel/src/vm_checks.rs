//! Real-backend VM rejection checks; rejected input must not produce output.

use byte_ring_kernel_core::bytecode::builder::ProgramBuilder;
use byte_ring_kernel_core::bytecode::instruction::DecodeError;
use byte_ring_kernel_core::bytecode::vm::{Error, Limits, execute};
use byte_ring_kernel_core::file::{FileWriter, STDOUT, WriteError};

pub fn run(files: &mut impl FileWriter) -> ! {
    let mut storage = [0; 128];
    let length = {
        let mut builder = ProgramBuilder::new(&mut storage);
        builder.write(STDOUT, b"SHOULD NOT APPEAR").unwrap();
        builder.return_imm(0).unwrap();
        builder.finish().unwrap().len()
    };
    storage[length] = 0xff;
    let malformed = execute(&storage[..length + 1], files, Limits::default());
    if !matches!(
        malformed,
        Err(Error::InvalidProgram(DecodeError::TrailingBytes { .. }))
    ) {
        super::qemu_test::finish(false);
    }
    let mut builder = ProgramBuilder::new(&mut storage);
    builder.write(1, b"SHOULD NOT APPEAR").unwrap();
    builder.return_imm(0).unwrap();
    if execute(builder.finish().unwrap(), files, Limits::default())
        != Err(Error::File(WriteError::BadHandle))
    {
        super::qemu_test::finish(false);
    }
    let mut builder = ProgramBuilder::new(&mut storage);
    builder
        .write(STDOUT, b"Byte Ring OS: VM rejection checks passed\r\n")
        .unwrap();
    builder.return_imm(123).unwrap();
    super::qemu_test::finish(
        execute(builder.finish().unwrap(), files, Limits::default()) == Ok(123),
    )
}
