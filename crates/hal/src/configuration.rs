#[cfg(all(feature = "test-hal", target_os = "none"))]
compile_error!("test-hal must never be enabled in a bare-metal kernel");

#[cfg(all(
    not(feature = "test-hal"),
    not(all(target_arch = "x86_64", target_os = "none"))
))]
compile_error!("host builds must enable test-hal; production port I/O requires bare-metal x86_64");
