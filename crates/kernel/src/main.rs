#![no_std]
#![no_main]

mod boot;
mod debug;
#[cfg(feature = "qemu-test")]
mod qemu_test;

use core::panic::PanicInfo;

/// Called only by the assembly adapter after establishing 64-bit execution,
/// identity mappings, a zeroed BSS, disabled interrupts, and an aligned stack.
#[unsafe(no_mangle)]
extern "C" fn kernel_main() -> ! {
    // SAFETY: this is the sole COM1 owner; interrupts and other CPUs are off.
    let mut serial = unsafe { debug::com1() };
    let result = serial.write_bytes(b"Byte Ring OS: COM1 ready\r\n");
    #[cfg(feature = "qemu-test")]
    qemu_test::finish(result.is_ok());
    #[cfg(not(feature = "qemu-test"))]
    {
        let _ = result;
        halt()
    }
}

#[panic_handler]
fn panic(_information: &PanicInfo<'_>) -> ! {
    // Early boot is single-core, interrupts stay disabled, and unwinding never
    // resumes the abandoned driver's access. Reinitialize without a logger lock.
    // SAFETY: no interrupted device access can resume after this panic.
    let _ = unsafe { debug::com1() }.write_bytes(b"Byte Ring OS: PANIC\r\n");
    #[cfg(feature = "qemu-test")]
    qemu_test::finish(false);
    #[cfg(not(feature = "qemu-test"))]
    halt()
}

fn halt() -> ! {
    loop {
        // SAFETY: the boot adapter establishes CPL0. Interrupts remain disabled
        // because the kernel has not installed an interrupt descriptor table.
        unsafe { core::arch::asm!("cli", "hlt", options(nomem, nostack)) };
    }
}
