//! Normalized x86_64 exception frames and allocation-free diagnostics.

use core::fmt::{self, Write};

/// Stack layout established by the native exception entry stubs.
///
/// CPU-pushed fields follow the normalized vector and error code. CR2 is
/// captured before entering Rust; it is meaningful only for page faults.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ExceptionFrame {
    pub fault_address: u64,
    pub vector: u64,
    pub error_code: u64,
    pub instruction_pointer: u64,
    pub code_segment: u64,
    pub flags: u64,
    pub stack_pointer: u64,
    pub stack_segment: u64,
}

/// Human-readable architectural exception name, including reserved vectors.
pub fn name(vector: u64) -> &'static str {
    match vector {
        0 => "Divide error",
        1 => "Debug",
        2 => "Non-maskable interrupt",
        3 => "Breakpoint",
        4 => "Overflow",
        5 => "Bound range exceeded",
        6 => "Invalid opcode",
        7 => "Device not available",
        8 => "Double fault",
        10 => "Invalid TSS",
        11 => "Segment not present",
        12 => "Stack-segment fault",
        13 => "General protection fault",
        14 => "Page fault",
        16 => "x87 floating-point exception",
        17 => "Alignment check",
        18 => "Machine check",
        19 => "SIMD floating-point exception",
        20 => "Virtualization exception",
        21 => "Control protection exception",
        28 => "Hypervisor injection exception",
        29 => "VMM communication exception",
        30 => "Security exception",
        9 | 15 | 22..=27 | 31 => "Reserved exception",
        _ => "Unexpected interrupt",
    }
}

/// Print a complete fatal exception report. Output may contain a prefix if the
/// writer fails; the first formatting error stops further output.
pub fn write_report(output: &mut impl Write, frame: &ExceptionFrame) -> fmt::Result {
    write!(
        output,
        "Byte Ring OS: EXCEPTION vector={} name={}\r\n",
        frame.vector,
        name(frame.vector)
    )?;
    write!(
        output,
        "RIP={:#018x} CS={:#06x} RFLAGS={:#018x}\r\n",
        frame.instruction_pointer, frame.code_segment, frame.flags
    )?;
    write!(
        output,
        "RSP={:#018x} SS={:#06x} ERROR={:#018x}\r\n",
        frame.stack_pointer, frame.stack_segment, frame.error_code
    )?;
    if frame.vector == 14 {
        let error = frame.error_code;
        write!(
            output,
            "CR2={:#018x} access={} privilege={} reason={} reserved={} instruction={} protection-key={} shadow-stack={} sgx={}\r\n",
            frame.fault_address,
            if error & 2 != 0 { "write" } else { "read" },
            if error & 4 != 0 { "user" } else { "supervisor" },
            if error & 1 != 0 {
                "protection"
            } else {
                "not-present"
            },
            error & 8 != 0,
            error & 16 != 0,
            error & 32 != 0,
            error & 64 != 0,
            error & (1 << 15) != 0
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "exception_tests.rs"]
mod tests;
