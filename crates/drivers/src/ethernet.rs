//! smoltcp adapter with copied frames and explicit virtqueue backpressure.
use crate::virtio_net::{Error, Net};
use smoltcp::{
    phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken},
    time::Instant,
};

const MTU: usize = 1514;

/// Sole NIC owner. Polling has a bounded receive budget; device failures are
/// sticky until the caller takes them and terminates this device's service.
pub struct Ethernet<'a> {
    nic: Net<'a>,
    input: [u8; MTU],
    output: [u8; MTU],
    failure: Option<Error>,
    budget: usize,
}
impl<'a> Ethernet<'a> {
    pub fn new(nic: Net<'a>) -> Self {
        Self {
            nic,
            input: [0; MTU],
            output: [0; MTU],
            failure: None,
            budget: 0,
        }
    }
    pub fn begin_poll(&mut self) {
        self.budget = 16;
    }
    pub fn take_error(&mut self) -> Option<Error> {
        self.failure.take()
    }
    fn ready(&mut self) -> bool {
        if self.failure.is_some() {
            return false;
        }
        match self.nic.can_transmit() {
            Ok(ready) => ready,
            Err(error) => {
                self.failure = Some(error);
                false
            }
        }
    }
}

pub struct Receive<'a>(&'a [u8]);
impl RxToken for Receive<'_> {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(self.0)
    }
}
pub struct Transmit<'a, 'device> {
    nic: &'a mut Net<'device>,
    output: &'a mut [u8; MTU],
    failure: &'a mut Option<Error>,
}
impl TxToken for Transmit<'_, '_> {
    fn consume<R, F>(self, length: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        // Stack-generated packets obey the MTU advertised by capabilities().
        let result = f(&mut self.output[..length]);
        if let Err(error) = self.nic.transmit(&self.output[..length]) {
            *self.failure = Some(error);
        }
        result
    }
}
impl<'device> Device for Ethernet<'device> {
    type RxToken<'a>
        = Receive<'a>
    where
        Self: 'a;
    type TxToken<'a>
        = Transmit<'a, 'device>
    where
        Self: 'a;
    fn receive(&mut self, _: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        if self.budget == 0 || !self.ready() {
            return None;
        }
        self.budget -= 1;
        let length = match self.nic.receive(&mut self.input) {
            Ok(Some(length)) if length > 0 => length,
            Ok(_) => return None,
            Err(error) => {
                self.failure = Some(error);
                return None;
            }
        };
        Some((
            Receive(&self.input[..length]),
            Transmit {
                nic: &mut self.nic,
                output: &mut self.output,
                failure: &mut self.failure,
            },
        ))
    }
    fn transmit(&mut self, _: Instant) -> Option<Self::TxToken<'_>> {
        if !self.ready() {
            return None;
        }
        Some(Transmit {
            nic: &mut self.nic,
            output: &mut self.output,
            failure: &mut self.failure,
        })
    }
    fn capabilities(&self) -> DeviceCapabilities {
        let mut capabilities = DeviceCapabilities::default();
        capabilities.medium = Medium::Ethernet;
        capabilities.max_transmission_unit = MTU;
        capabilities.max_burst_size = Some(1);
        capabilities
    }
}
