//! Test-image-only fault injection and termination through isa-debug-exit.

use byte_ring_hal::port::PortRange;
use byte_ring_kernel_core::exception::ExceptionFrame;

core::arch::global_asm!(include_str!("qemu_test.S"), options(att_syntax));

unsafe extern "C" {
    fn trigger_divide() -> !;
    fn trigger_invalid_opcode() -> !;
    fn trigger_general_protection() -> !;
    fn trigger_page_fault() -> !;
    fn trigger_double_fault() -> !;
    static divide_instruction: u8;
    static invalid_opcode_instruction: u8;
    static general_protection_instruction: u8;
    static page_fault_instruction: u8;
}

const CASES: u32 = cfg!(feature = "qemu-divide") as u32
    + cfg!(feature = "qemu-invalid-opcode") as u32
    + cfg!(feature = "qemu-general-protection") as u32
    + cfg!(feature = "qemu-page-fault") as u32
    + cfg!(feature = "qemu-double-fault") as u32
    + cfg!(feature = "qemu-panic") as u32
    + cfg!(feature = "qemu-vm-errors") as u32;
const _: () = assert!(CASES <= 1, "select at most one QEMU fault scenario");

pub fn run(files: &mut impl byte_ring_kernel_core::file::FileWriter) -> ! {
    // SAFETY: only these test-image functions intentionally violate CPU rules.
    // Faults must enter the installed fatal handlers and must never return.
    unsafe {
        if cfg!(feature = "qemu-divide") {
            trigger_divide()
        } else if cfg!(feature = "qemu-invalid-opcode") {
            trigger_invalid_opcode()
        } else if cfg!(feature = "qemu-general-protection") {
            trigger_general_protection()
        } else if cfg!(feature = "qemu-page-fault") {
            trigger_page_fault()
        } else if cfg!(feature = "qemu-double-fault") {
            trigger_double_fault()
        } else if cfg!(feature = "qemu-panic") {
            panic!("intentional QEMU panic")
        } else if cfg!(feature = "qemu-vm-errors") {
            super::vm_checks::run(files)
        } else {
            finish(true)
        }
    }
}

pub fn exception(frame: &ExceptionFrame, reported: bool, emergency_stack: bool) -> ! {
    let (vector, error_code, instruction) = if cfg!(feature = "qemu-divide") {
        (0, 0, core::ptr::addr_of!(divide_instruction) as u64)
    } else if cfg!(feature = "qemu-invalid-opcode") {
        (6, 0, core::ptr::addr_of!(invalid_opcode_instruction) as u64)
    } else if cfg!(feature = "qemu-general-protection") {
        (
            13,
            0x28,
            core::ptr::addr_of!(general_protection_instruction) as u64,
        )
    } else if cfg!(feature = "qemu-page-fault") {
        (14, 0, core::ptr::addr_of!(page_fault_instruction) as u64)
    } else if cfg!(feature = "qemu-double-fault") {
        (8, 0, 0)
    } else {
        finish(false)
    };
    let valid = reported
        && frame.vector == vector
        && frame.error_code == error_code
        && (vector == 8 || frame.instruction_pointer == instruction)
        && frame.code_segment == 8
        && frame.flags & 2 != 0
        && (vector != 14 || frame.fault_address == 0x4000_0000)
        && (vector != 8 || (emergency_stack && frame.stack_pointer == 0x4000_1000));
    finish(valid)
}

pub fn panic(reported: bool) -> ! {
    finish(cfg!(feature = "qemu-panic") && reported)
}

pub fn finish(success: bool) -> ! {
    // SAFETY: the integration runner installs a byte-accessible exit device at
    // port 0xf4. This single-core test image exclusively owns that port.
    let mut exit = unsafe { PortRange::new(0xf4, 1) };
    // QEMU reports (value << 1) | 1: 33 for success, 35 for failure.
    exit.write_u8(0, if success { 0x10 } else { 0x11 });
    super::halt()
}
