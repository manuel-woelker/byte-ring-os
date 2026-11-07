#![no_std]
#![no_main]

#[cfg(any(feature = "network", feature = "userspace"))]
mod apic;
mod boot;
#[cfg(any(feature = "http", feature = "userspace"))]
mod clock;
mod debug;
mod exceptions;
mod files;
#[cfg(feature = "userspace")]
mod userspace;
#[cfg(all(feature = "userspace", feature = "network"))]
compile_error!("userspace and network images are separate");
#[cfg(feature = "http")]
mod http;
#[cfg(feature = "network")]
mod idle;
#[cfg(any(feature = "network", feature = "userspace"))]
mod mmio;
#[cfg(feature = "network")]
mod net;
#[cfg(feature = "qemu-test")]
mod qemu_test;
#[cfg(feature = "qemu-test")]
mod vm_checks;

use core::panic::PanicInfo;

/// Called only by the assembly adapter after establishing 64-bit execution,
/// identity mappings, a zeroed BSS, disabled interrupts, and an aligned stack.
#[unsafe(no_mangle)]
extern "C" fn kernel_main() -> ! {
    // SAFETY: first initialization on the sole boot CPU at CPL0 with IF=0.
    unsafe { exceptions::initialize() };
    // SAFETY: this is the sole COM1 owner; interrupts and other CPUs are off.
    let serial = unsafe { debug::com1() };
    let mut files = files::BootFiles::new(serial);
    let result = files::boot_program(&mut files);
    #[cfg(feature = "userspace")]
    {
        if let Err(error) = result {
            panic!("boot bytecode failed: {error:?}");
        }
        userspace::run(files)
    }
    #[cfg(feature = "network")]
    {
        if let Err(error) = result {
            panic!("boot bytecode failed: {error:?}");
        }
        #[cfg(feature = "http")]
        {
            http::run(&mut files)
        }
        #[cfg(not(feature = "http"))]
        {
            net::run(&mut files)
        }
    }
    #[cfg(all(not(feature = "network"), feature = "qemu-test"))]
    {
        if result != Ok(123) {
            qemu_test::finish(false);
        }
        qemu_test::run(&mut files)
    }
    #[cfg(all(
        not(feature = "network"),
        not(feature = "userspace"),
        not(feature = "qemu-test")
    ))]
    {
        if let Err(error) = result {
            panic!("boot bytecode failed: {error:?}");
        }
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
    #[cfg(feature = "userspace-test")]
    {
        let _ = reported;
        userspace::test_failure();
    }
    #[cfg(all(not(feature = "qemu-test"), not(feature = "userspace-test")))]
    {
        let _ = reported;
        halt()
    }
}

fn halt() -> ! {
    loop {
        // SAFETY: the boot adapter establishes CPL0. Interrupts remain disabled
        // because fatal and ordinary boot paths never resume device work.
        unsafe { core::arch::asm!("cli", "hlt", options(nomem, nostack)) };
    }
}
