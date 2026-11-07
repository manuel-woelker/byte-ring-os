#![no_std]
#![forbid(unsafe_code)]

pub mod pci;
pub mod serial;
#[cfg(all(test, feature = "test-hal"))]
mod serial_tests;
pub mod virtio_net;
pub mod virtqueue;
