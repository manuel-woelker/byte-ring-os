extern crate std;

use core::fmt;
use std::string::String;

use super::{ExceptionFrame, name, write_report};

fn frame(vector: u64, error_code: u64) -> ExceptionFrame {
    ExceptionFrame {
        fault_address: 0x4000_0000,
        vector,
        error_code,
        instruction_pointer: 0x101234,
        code_segment: 8,
        flags: 2,
        stack_pointer: 0x118000,
        stack_segment: 16,
    }
}

#[test]
fn frame_matches_the_native_stack_layout() {
    assert_eq!(core::mem::size_of::<ExceptionFrame>(), 64);
    assert_eq!(core::mem::offset_of!(ExceptionFrame, vector), 8);
    assert_eq!(core::mem::offset_of!(ExceptionFrame, error_code), 16);
    assert_eq!(
        core::mem::offset_of!(ExceptionFrame, instruction_pointer),
        24
    );
    assert_eq!(core::mem::offset_of!(ExceptionFrame, stack_pointer), 48);
}

#[test]
fn reports_a_fault_without_a_cpu_error_code() {
    let mut output = String::new();
    write_report(&mut output, &frame(6, 0)).unwrap();
    assert_eq!(
        output,
        "Byte Ring OS: EXCEPTION vector=6 name=Invalid opcode\r\n\
        RIP=0x0000000000101234 CS=0x0008 RFLAGS=0x0000000000000002\r\n\
        RSP=0x0000000000118000 SS=0x0010 ERROR=0x0000000000000000\r\n"
    );
}

#[test]
fn preserves_selector_error_codes() {
    let mut output = String::new();
    write_report(&mut output, &frame(13, 0x28)).unwrap();
    assert!(output.contains("name=General protection fault\r\n"));
    assert!(output.contains("ERROR=0x0000000000000028\r\n"));
    assert!(!output.contains("CR2="));
}

#[test]
fn explains_a_missing_supervisor_page() {
    let mut output = String::new();
    write_report(&mut output, &frame(14, 0)).unwrap();
    assert!(
        output
            .contains("CR2=0x0000000040000000 access=read privilege=supervisor reason=not-present")
    );
    assert!(output.contains(
        "reserved=false instruction=false protection-key=false shadow-stack=false sgx=false"
    ));
}

#[test]
fn decodes_page_protection_and_extended_error_bits() {
    let mut output = String::new();
    write_report(&mut output, &frame(14, 0x807f)).unwrap();
    assert!(output.contains("access=write privilege=user reason=protection"));
    assert!(
        output.contains(
            "reserved=true instruction=true protection-key=true shadow-stack=true sgx=true"
        )
    );
}

#[test]
fn names_reserved_and_unknown_vectors_without_indexing_panics() {
    for vector in 0..32 {
        assert!(!name(vector).is_empty());
    }
    assert_eq!(name(15), "Reserved exception");
    assert_eq!(name(u64::MAX), "Unexpected interrupt");
}

#[test]
fn stops_when_diagnostic_output_fails() {
    struct BrokenOutput;
    impl fmt::Write for BrokenOutput {
        fn write_str(&mut self, _text: &str) -> fmt::Result {
            Err(fmt::Error)
        }
    }
    assert_eq!(
        write_report(&mut BrokenOutput, &frame(14, 0)),
        Err(fmt::Error)
    );
}
