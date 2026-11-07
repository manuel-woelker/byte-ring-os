//! Bounded PCI configuration discovery for the initial modern virtio-net device.

use byte_ring_hal::port::PortRange;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Missing,
    Capabilities,
    Bar,
    Window,
    Unsupported,
    Interrupt,
}

#[derive(Debug, Clone, Copy)]
pub struct Window {
    pub address: u64,
    pub length: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Transport {
    pub bdf: u32,
    pub irq: u8,
    pub common: Window,
    pub notify: Window,
    pub isr: Window,
    pub device: Window,
    pub notify_multiplier: u32,
}

#[derive(Clone, Copy)]
struct Bar {
    address: u64,
    length: u64,
}

pub struct Pci<'a> {
    ports: PortRange<'a>,
}
impl<'a> Pci<'a> {
    pub fn new(ports: PortRange<'a>) -> Self {
        assert_eq!((ports.base(), ports.length()), (0xcf8, 8));
        Self { ports }
    }
    pub fn read(&mut self, bdf: u32, offset: u16) -> u32 {
        assert!(offset < 256 && offset.is_multiple_of(4));
        self.ports.write_u32(0, 0x80000000 | bdf | offset as u32);
        self.ports.read_u32(4)
    }
    fn write(&mut self, bdf: u32, offset: u16, value: u32) {
        self.ports.write_u32(0, 0x80000000 | bdf | offset as u32);
        self.ports.write_u32(4, value);
    }
    fn byte(&mut self, bdf: u32, offset: u16) -> u8 {
        (self.read(bdf, offset & !3) >> ((offset & 3) * 8)) as u8
    }
    fn command(&mut self, bdf: u32, value: u16) {
        self.ports.write_u32(0, 0x80000004 | bdf);
        // Do not write back the W1C status bits in the upper 16 bits.
        self.ports.write_u16(4, value);
    }
    pub fn enable(&mut self, bdf: u32) {
        let command = self.read(bdf, 4) as u16;
        self.command(bdf, (command | 6) & !(1 << 10));
    }
    pub fn discover(&mut self) -> Result<Transport, Error> {
        for bus in 0..256u32 {
            for slot in 0..32u32 {
                let first = (bus << 16) | (slot << 11);
                let id = self.read(first, 0);
                if id as u16 == 0xffff {
                    continue;
                }
                let functions = if self.byte(first, 14) & 0x80 != 0 {
                    8
                } else {
                    1
                };
                for function in 0..functions {
                    let bdf = first | (function << 8);
                    let id = if function == 0 { id } else { self.read(bdf, 0) };
                    if id == 0x10411af4 {
                        return self.transport(bdf);
                    }
                }
            }
        }
        Err(Error::Missing)
    }
    fn bars(&mut self, bdf: u32) -> Result<[Option<Bar>; 6], Error> {
        let command = self.read(bdf, 4) as u16;
        self.command(bdf, command & !7);
        let result = self.probe_bars(bdf);
        // Leave bus mastering disabled until the caller validates/maps windows.
        self.command(bdf, command & !4);
        result
    }
    fn probe_bars(&mut self, bdf: u32) -> Result<[Option<Bar>; 6], Error> {
        let mut bars = [None; 6];
        let mut index = 0;
        while index < 6 {
            let offset = 0x10 + index as u16 * 4;
            let low = self.read(bdf, offset);
            if low & 1 != 0 {
                index += 1;
                continue;
            }
            let wide = low & 6 == 4;
            if low & 6 != 0 && !wide {
                return Err(Error::Bar);
            }
            if wide && index == 5 {
                return Err(Error::Bar);
            }
            let high = if wide { self.read(bdf, offset + 4) } else { 0 };
            self.write(bdf, offset, u32::MAX);
            if wide {
                self.write(bdf, offset + 4, u32::MAX);
            }
            let mask_low = self.read(bdf, offset) & !15;
            let mask_high = if wide {
                self.read(bdf, offset + 4)
            } else {
                u32::MAX
            };
            self.write(bdf, offset, low);
            if wide {
                self.write(bdf, offset + 4, high);
            }
            if mask_low != 0 || (wide && mask_high != 0) {
                let mask = ((mask_high as u64) << 32) | mask_low as u64;
                let length = (!mask).wrapping_add(1);
                let address = ((high as u64) << 32) | (low & !15) as u64;
                if length == 0
                    || !length.is_power_of_two()
                    || address == 0
                    || !address.is_multiple_of(length)
                {
                    return Err(Error::Bar);
                }
                bars[index] = Some(Bar { address, length });
            }
            index += if wide { 2 } else { 1 };
        }
        Ok(bars)
    }
    fn transport(&mut self, bdf: u32) -> Result<Transport, Error> {
        if self.byte(bdf, 14) & 0x7f != 0 {
            return Err(Error::Unsupported);
        }
        if self.read(bdf, 4) & (1 << 20) == 0 {
            return Err(Error::Capabilities);
        }
        let irq = self.byte(bdf, 0x3c);
        let pin = self.byte(bdf, 0x3d);
        if !(1..=4).contains(&pin) || !crate::pic::valid_network_irq(irq) {
            return Err(Error::Interrupt);
        }
        let bars = self.bars(bdf)?;
        let mut regions = [None; 4];
        let mut multiplier = 0;
        let mut visited = [false; 64];
        let mut cap = self.byte(bdf, 0x34) as u16;
        while cap != 0 {
            if !(0x40..=0xfc).contains(&cap) || !cap.is_multiple_of(4) || visited[cap as usize / 4]
            {
                return Err(Error::Capabilities);
            }
            visited[cap as usize / 4] = true;
            let next = self.byte(bdf, cap + 1) as u16;
            if self.byte(bdf, cap) == 9 {
                let length = self.byte(bdf, cap + 2) as u16;
                let kind = self.byte(bdf, cap + 3);
                if length < 16 || cap + length > 256 {
                    return Err(Error::Capabilities);
                }
                if (1..=4).contains(&kind) {
                    let bar = self.byte(bdf, cap + 4) as usize;
                    let bar = bars.get(bar).copied().flatten().ok_or(Error::Bar)?;
                    let offset = self.read(bdf, cap + 8) as u64;
                    let size = self.read(bdf, cap + 12) as u64;
                    let minimum = [56, 2, 1, 6][kind as usize - 1];
                    let end = offset.checked_add(size).ok_or(Error::Window)?;
                    let address = bar.address.checked_add(offset).ok_or(Error::Window)?;
                    if size < minimum
                        || end > bar.length
                        || address < 0x40000000
                        || address
                            .checked_add(size)
                            .is_none_or(|end| end > 0x100000000)
                    {
                        return Err(Error::Window);
                    }
                    if (kind == 1 && address % 4 != 0) || (kind == 2 && address % 2 != 0) {
                        return Err(Error::Window);
                    }
                    if regions[kind as usize - 1].is_some() {
                        return Err(Error::Capabilities);
                    }
                    regions[kind as usize - 1] = Some(Window {
                        address,
                        length: size as usize,
                    });
                    if kind == 2 {
                        if length < 20 {
                            return Err(Error::Capabilities);
                        }
                        multiplier = self.read(bdf, cap + 16);
                        if multiplier != 0 && (!multiplier.is_power_of_two() || multiplier % 2 != 0)
                        {
                            return Err(Error::Capabilities);
                        }
                    }
                }
            }
            cap = next;
        }
        let [Some(common), Some(notify), Some(isr), Some(device)] = regions else {
            return Err(Error::Capabilities);
        };
        for (index, first) in regions.iter().flatten().enumerate() {
            for second in regions.iter().flatten().skip(index + 1) {
                if first.address < second.address + second.length as u64
                    && second.address < first.address + first.length as u64
                {
                    return Err(Error::Window);
                }
            }
        }
        Ok(Transport {
            bdf,
            irq,
            common,
            notify,
            isr,
            device,
            notify_multiplier: multiplier,
        })
    }
}

#[cfg(all(test, feature = "test-hal"))]
#[path = "pci_tests.rs"]
mod tests;
