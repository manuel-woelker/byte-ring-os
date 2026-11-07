//! Single-buffer split virtqueue ownership. DMA RAM is accessed only via HAL.

use byte_ring_hal::memory::Memory;
use core::sync::atomic::{Ordering, fence};

pub const QUEUE_SIZE: usize = 32;
pub const BUFFER_SIZE: usize = 2048;
pub const HEADER_SIZE: usize = 12;
pub const AVAILABLE: usize = 512;
pub const USED: usize = 4096;
pub const BUFFERS: usize = 8192;
pub const DMA_SIZE: usize = BUFFERS + QUEUE_SIZE * BUFFER_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Corrupt,
    Full,
    PacketSize,
}

pub struct Queue<'a> {
    memory: Memory<'a>,
    size: u16,
    available: u16,
    used: u16,
    owned: [bool; QUEUE_SIZE],
    receive: bool,
    poisoned: bool,
}

impl<'a> Queue<'a> {
    pub fn new(mut memory: Memory<'a>, size: u16, receive: bool) -> Result<Self, Error> {
        if size == 0
            || !size.is_power_of_two()
            || size as usize > QUEUE_SIZE
            || memory.length() < DMA_SIZE
            || memory.physical(0) % 4096 != 0
        {
            return Err(Error::Corrupt);
        }
        memory.clear();
        memory.write_u16(AVAILABLE, 1); // Suppress queue interrupts; this driver polls.
        let mut queue = Self {
            memory,
            size,
            available: 0,
            used: 0,
            owned: [false; QUEUE_SIZE],
            receive,
            poisoned: false,
        };
        if receive {
            for id in 0..size {
                queue.publish(id, BUFFER_SIZE as u32)?;
            }
        }
        Ok(queue)
    }
    pub fn addresses(&self) -> (u64, u64, u64) {
        (
            self.memory.physical(0),
            self.memory.physical(AVAILABLE),
            self.memory.physical(USED),
        )
    }
    fn publish(&mut self, id: u16, length: u32) -> Result<(), Error> {
        if self.poisoned || id >= self.size || self.owned[id as usize] {
            return Err(Error::Corrupt);
        }
        let descriptor = id as usize * 16;
        self.memory.write_u64(
            descriptor,
            self.memory.physical(BUFFERS + id as usize * BUFFER_SIZE),
        );
        self.memory.write_u32(descriptor + 8, length);
        self.memory
            .write_u16(descriptor + 12, if self.receive { 2 } else { 0 });
        self.memory.write_u16(descriptor + 14, 0);
        self.memory.write_u16(
            AVAILABLE + 4 + (self.available % self.size) as usize * 2,
            id,
        );
        self.owned[id as usize] = true;
        fence(Ordering::Release);
        self.available = self.available.wrapping_add(1);
        self.memory.write_u16(AVAILABLE + 2, self.available);
        // x86 coherent DMA still needs store ordering before MMIO notification.
        fence(Ordering::SeqCst);
        Ok(())
    }
    fn completion(&mut self) -> Result<Option<(u16, usize)>, Error> {
        if self.poisoned {
            return Err(Error::Corrupt);
        }
        let next = self.memory.read_u16(USED + 2);
        let count = next.wrapping_sub(self.used);
        if count == 0 {
            return Ok(None);
        }
        fence(Ordering::Acquire);
        if count > self.size {
            self.poisoned = true;
            return Err(Error::Corrupt);
        }
        let slot = USED + 4 + (self.used % self.size) as usize * 8;
        let id = self.memory.read_u32(slot);
        let length = self.memory.read_u32(slot + 4) as usize;
        if id >= self.size as u32
            || !self.owned[id as usize]
            || length > BUFFER_SIZE
            || (self.receive && length < HEADER_SIZE)
        {
            self.poisoned = true;
            return Err(Error::Corrupt);
        }
        self.owned[id as usize] = false;
        self.used = self.used.wrapping_add(1);
        Ok(Some((id as u16, length)))
    }
    pub fn reclaim(&mut self) -> Result<(), Error> {
        if self.receive {
            return Err(Error::Corrupt);
        }
        for _ in 0..self.size {
            if self.completion()?.is_none() {
                break;
            }
        }
        Ok(())
    }
    pub fn transmit(&mut self, packet: &[u8]) -> Result<(), Error> {
        if self.receive || self.poisoned {
            return Err(Error::Corrupt);
        }
        if packet.len() > 1514 {
            return Err(Error::PacketSize);
        }
        self.reclaim()?;
        let id = (0..self.size)
            .find(|&id| !self.owned[id as usize])
            .ok_or(Error::Full)?;
        let offset = BUFFERS + id as usize * BUFFER_SIZE;
        self.memory.write_bytes(offset, &[0; HEADER_SIZE]);
        self.memory.write_bytes(offset + HEADER_SIZE, packet);
        self.publish(id, (HEADER_SIZE + packet.len()) as u32)
    }
    pub fn receive(&mut self, output: &mut [u8]) -> Result<Option<usize>, Error> {
        if !self.receive {
            return Err(Error::Corrupt);
        }
        let Some((id, length)) = self.completion()? else {
            return Ok(None);
        };
        let offset = BUFFERS + id as usize * BUFFER_SIZE;
        let flags = self.memory.read_u8(offset);
        let gso = self.memory.read_u8(offset + 1);
        // QEMU 7.2 leaves num_buffers zero when mergeable RX is disabled.
        // Both zero and one still describe our single owned descriptor.
        let buffers = self.memory.read_u16(offset + 10);
        let packet_length = length - HEADER_SIZE;
        let valid = flags == 0 && gso == 0 && buffers <= 1 && packet_length <= output.len();
        if valid {
            self.memory
                .read_bytes(offset + HEADER_SIZE, &mut output[..packet_length]);
        }
        self.publish(id, BUFFER_SIZE as u32)?;
        Ok(Some(if valid { packet_length } else { 0 }))
    }
}

#[cfg(all(test, feature = "test-hal"))]
#[path = "virtqueue_tests.rs"]
mod tests;
