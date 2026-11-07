//! Boot-owned 8259 PIC pair for the single-core QEMU PC.
use byte_ring_hal::port::PortRange;

/// ELCR-programmable IRQs; timer, cascade, RTC and coprocessor are excluded.
pub const LEVEL_IRQS: u16 = 0xdef8;
pub const VECTOR_BASE: u8 = 32;

pub fn valid_network_irq(irq: u8) -> bool {
    irq < 16 && LEVEL_IRQS & (1u16 << irq) != 0
}

/// All accesses require IF=0. Routes are enabled only around an idle HLT.
pub struct Pic<'a> {
    master: PortRange<'a>,
    slave: PortRange<'a>,
    elcr: PortRange<'a>,
}
impl<'a> Pic<'a> {
    pub fn new(master: PortRange<'a>, slave: PortRange<'a>, elcr: PortRange<'a>) -> Self {
        assert_eq!((master.base(), master.length()), (0x20, 2));
        assert_eq!((slave.base(), slave.length()), (0xa0, 2));
        assert_eq!((elcr.base(), elcr.length()), (0x4d0, 2));
        let mut pic = Self {
            master,
            slave,
            elcr,
        };
        pic.mask_all();
        // ICW1..4: initialization, vector bases, cascade wiring, 8086 mode.
        // The supported QEMU PIC needs no legacy ISA I/O delay port.
        pic.master.write_u8(0, 0x11);
        pic.slave.write_u8(0, 0x11);
        pic.master.write_u8(1, VECTOR_BASE);
        pic.slave.write_u8(1, VECTOR_BASE + 8);
        pic.master.write_u8(1, 4);
        pic.slave.write_u8(1, 2);
        pic.master.write_u8(1, 1);
        pic.slave.write_u8(1, 1);
        pic.mask_all();
        pic
    }
    pub fn configure_network(&mut self, irq: u8) {
        assert!(valid_network_irq(irq));
        let offset = u16::from(irq / 8);
        let value = self.elcr.read_u8(offset);
        self.elcr.write_u8(offset, value | (1 << (irq % 8)));
    }
    pub fn mask_all(&mut self) {
        self.master.write_u8(1, 0xff);
        self.slave.write_u8(1, 0xff);
    }
    pub fn arm(&mut self, irq: u8, timer: bool) {
        assert!(valid_network_irq(irq));
        let mut enabled = 1u16 << irq;
        if irq >= 8 {
            enabled |= 1 << 2;
        }
        if timer {
            enabled |= 1;
        }
        self.slave.write_u8(1, !(enabled >> 8) as u8);
        self.master.write_u8(1, !enabled as u8);
    }
    /// Complete a recorded IRQ after masking routes. Spurious IRQ 7 has no
    /// EOI; spurious IRQ 15 acknowledges the master's cascade only.
    pub fn finish(&mut self, irq: u8) {
        assert!(irq < 16);
        if irq == 7 {
            self.master.write_u8(0, 0x0b);
            if self.master.read_u8(0) & 0x80 == 0 {
                return;
            }
        }
        if irq == 15 {
            self.slave.write_u8(0, 0x0b);
            if self.slave.read_u8(0) & 0x80 == 0 {
                self.master.write_u8(0, 0x20);
                return;
            }
        }
        if irq >= 8 {
            self.slave.write_u8(0, 0x20);
        }
        self.master.write_u8(0, 0x20);
    }
}

#[cfg(all(test, feature = "test-hal"))]
#[path = "pic_tests.rs"]
mod tests;
