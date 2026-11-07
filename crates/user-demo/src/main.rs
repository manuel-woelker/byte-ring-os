#![no_std]
#![no_main]
mod application;

#[unsafe(no_mangle)]
extern "C" fn _start(boot: u64, argument: u64) -> ! {
    application::run(boot, argument)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    application::exit(99)
}
