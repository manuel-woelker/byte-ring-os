#![no_std]
#![forbid(unsafe_code)]

#[cfg(feature = "tcp")]
pub mod ethernet;
pub mod pci;
pub mod pit;
pub mod serial;
#[cfg(all(test, feature = "test-hal"))]
mod serial_tests;
pub mod virtio_net;
pub mod virtqueue;
