#![no_std]

#[cfg(all(feature = "test-hal", not(target_os = "none")))]
#[path = "test_backend.rs"]
pub mod backend;
#[cfg(any(not(feature = "test-hal"), target_os = "none"))]
#[path = "x86_64.rs"]
pub mod backend;
mod configuration;
pub mod memory;
#[cfg(all(feature = "test-hal", not(target_os = "none")))]
#[path = "memory_test.rs"]
mod memory_backend;
#[cfg(any(not(feature = "test-hal"), target_os = "none"))]
#[path = "memory_native.rs"]
mod memory_backend;
#[cfg(all(test, feature = "test-hal"))]
mod memory_tests;
pub mod port;
#[cfg(all(test, feature = "test-hal"))]
mod port_tests;
