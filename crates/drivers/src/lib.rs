#![no_std]
#![forbid(unsafe_code)]

pub mod serial;
#[cfg(all(test, feature = "test-hal"))]
mod serial_tests;
