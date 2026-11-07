//! Modern PCI virtio-net: VERSION_1, MAC, one RX/TX pair, no offloads.

use crate::virtqueue::{Error as QueueError, QUEUE_SIZE, Queue};
use byte_ring_hal::memory::Memory;

#[derive(Debug)]
pub enum Error {
    Reset,
    Features,
    Configuration,
    Queue(QueueError),
}

pub struct Net<'a> {
    common: Memory<'a>,
    notify: Memory<'a>,
    isr: Memory<'a>,
    rx: Queue<'a>,
    tx: Queue<'a>,
    offsets: [usize; 2],
    pub mac: [u8; 6],
}
impl<'a> Net<'a> {
    pub fn new(
        mut common: Memory<'a>,
        notify: Memory<'a>,
        isr: Memory<'a>,
        mut config: Memory<'a>,
        multiplier: u32,
        rx_memory: Memory<'a>,
        tx_memory: Memory<'a>,
    ) -> Result<Self, Error> {
        if common.length() < 56 || config.length() < 6 || isr.length() < 1 {
            return Err(Error::Configuration);
        }
        common.write_u8(20, 0);
        let mut reset = false;
        for _ in 0..100000 {
            if common.read_u8(20) == 0 {
                reset = true;
                break;
            }
        }
        if !reset {
            return Err(Error::Reset);
        }
        common.write_u8(20, 1);
        common.write_u8(20, 3);
        common.write_u32(0, 0);
        let low = common.read_u32(4);
        common.write_u32(0, 1);
        let high = common.read_u32(4);
        if low & (1 << 5) == 0 || high & 1 == 0 {
            common.write_u8(20, 0x83);
            return Err(Error::Features);
        }
        common.write_u32(8, 0);
        common.write_u32(12, 1 << 5);
        common.write_u32(8, 1);
        common.write_u32(12, 1);
        common.write_u8(20, 11);
        if common.read_u8(20) & 8 == 0 {
            return Err(Error::Features);
        }
        if common.read_u16(18) < 2 {
            return Err(Error::Configuration);
        }
        let mut mac = [0; 6];
        let mut consistent = false;
        for _ in 0..16 {
            let generation = common.read_u8(21);
            config.read_bytes(0, &mut mac);
            if common.read_u8(21) == generation {
                consistent = true;
                break;
            }
        }
        if !consistent || mac == [0; 6] || mac[0] & 1 != 0 {
            return Err(Error::Configuration);
        }
        let mut offsets = [0; 2];
        let mut sizes = [0; 2];
        for index in 0..2 {
            common.write_u16(22, index as u16);
            let maximum = common.read_u16(24);
            if maximum == 0 || common.read_u16(28) != 0 {
                return Err(Error::Configuration);
            }
            let mut size = QUEUE_SIZE as u16;
            while size > maximum {
                size /= 2;
            }
            sizes[index] = size;
            common.write_u16(24, size);
            common.write_u16(26, u16::MAX);
            let offset = (common.read_u16(30) as usize)
                .checked_mul(multiplier as usize)
                .ok_or(Error::Configuration)?;
            if offset % 2 != 0
                || offset
                    .checked_add(2)
                    .is_none_or(|end| end > notify.length())
            {
                return Err(Error::Configuration);
            }
            offsets[index] = offset;
        }
        let rx = Queue::new(rx_memory, sizes[0], true).map_err(Error::Queue)?;
        let tx = Queue::new(tx_memory, sizes[1], false).map_err(Error::Queue)?;
        for (index, queue) in [&rx, &tx].iter().enumerate() {
            common.write_u16(22, index as u16);
            let (descriptors, available, used) = queue.addresses();
            common.write_u64(32, descriptors);
            common.write_u64(40, available);
            common.write_u64(48, used);
            common.write_u16(28, 1);
        }
        common.write_u8(20, 15);
        let mut net = Self {
            common,
            notify,
            isr,
            rx,
            tx,
            offsets,
            mac,
        };
        net.notify.write_u16(net.offsets[0], 0);
        Ok(net)
    }
    pub fn receive(&mut self, output: &mut [u8]) -> Result<Option<usize>, Error> {
        if self.common.read_u8(20) & 0xc0 != 0 {
            return Err(Error::Configuration);
        }
        let _ = self.isr.read_u8(0);
        self.tx.reclaim().map_err(Error::Queue)?;
        let packet = self.rx.receive(output).map_err(Error::Queue)?;
        if packet.is_some() {
            self.notify.write_u16(self.offsets[0], 0);
        }
        Ok(packet)
    }
    /// Availability remains valid until the next transmit on this exclusive owner.
    pub fn can_transmit(&mut self) -> Result<bool, Error> {
        if self.common.read_u8(20) & 0xc0 != 0 {
            return Err(Error::Configuration);
        }
        self.tx.can_transmit().map_err(Error::Queue)
    }
    pub fn transmit(&mut self, packet: &[u8]) -> Result<(), Error> {
        self.tx.transmit(packet).map_err(Error::Queue)?;
        self.notify.write_u16(self.offsets[1], 1);
        Ok(())
    }
}

#[cfg(all(test, feature = "test-hal"))]
#[path = "virtio_net_tests.rs"]
mod tests;
