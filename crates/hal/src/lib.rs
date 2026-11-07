#![no_std]

#[cfg(all(feature = "test-hal", not(target_os = "none")))]
#[path = "test_backend.rs"]
pub mod backend;
#[cfg(any(not(feature = "test-hal"), target_os = "none"))]
#[path = "x86_64.rs"]
pub mod backend;
mod configuration;
pub mod port;
#[cfg(all(test, feature = "test-hal"))]
mod port_tests;
