#![no_std]
#![no_main]

mod boot;
mod debug;
mod exceptions;
#[cfg(feature = "qemu-test")]
mod qemu_test;

use core::panic::PanicInfo;

/// Called only by the assembly adapter after establishing 64-bit execution,
/// identity mappings, a zeroed BSS, disabled interrupts, and an aligned stack.
#[unsafe(no_mangle)]
extern "C" fn kernel_main() -> ! {
    // SAFETY: first initialization on the sole boot CPU at CPL0 with IF=0.
    unsafe { exceptions::initialize() };
    // SAFETY: this is the sole COM1 owner; interrupts and other CPUs are off.
    let mut serial = unsafe { debug::com1() };
    let result = serial.write_bytes(b"Byte Ring OS: COM1 ready\r\n");
    #[cfg(feature = "qemu-test")]
    {
        if result.is_err() {
            qemu_test::finish(false);
        }
        qemu_test::run()
    }
    #[cfg(not(feature = "qemu-test"))]
    {
        let _ = result;
        halt()
    }
}

#[panic_handler]
fn panic(information: &PanicInfo<'_>) -> ! {
    use core::fmt::Write;

    debug::begin_fatal();
    // SAFETY: the fatal path never resumes the interrupted device owner.
    let mut serial = unsafe { debug::com1() };
    let reported = write!(serial, "Byte Ring OS: PANIC: {information}\r\n").is_ok();
    #[cfg(feature = "qemu-test")]
    qemu_test::panic(reported);
    #[cfg(not(feature = "qemu-test"))]
    {
        let _ = reported;
        halt()
    }
}

fn halt() -> ! {
    loop {
        // SAFETY: the boot adapter establishes CPL0. Interrupts remain disabled
        // because device interrupt handling is not installed yet.
        unsafe { core::arch::asm!("cli", "hlt", options(nomem, nostack)) };
    }
}
