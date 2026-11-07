//! Single-owner IRQ wakeups. Maskable interrupts are enabled only inside HLT.
use byte_ring_drivers::{
    apic::Apic,
    pic::Pic,
    pit,
    virtio_net::{Error, Net},
};
use byte_ring_hal::port::PortRange;
use core::sync::atomic::{AtomicU64, Ordering};

core::arch::global_asm!(include_str!("interrupts.S"), options(att_syntax));

// Written only by register-preserving IRQ stubs, consumed with IF clear.
#[unsafe(no_mangle)]
static IRQ_PENDING: AtomicU64 = AtomicU64::new(0);

pub struct Idle {
    pic: Pic<'static>,
    timer: Apic<'static>,
    irq: u8,
}
impl Idle {
    pub fn new(irq: u8) -> Self {
        // SAFETY: sole boot CPU, IF=0; no other PIC/ELCR/channel-0 owner.
        // HTTP's channel-2 calibration has already released its port handles.
        let (master, slave, elcr, pit) = unsafe {
            (
                PortRange::new(0x20, 2),
                PortRange::new(0xa0, 2),
                PortRange::new(0x4d0, 2),
                PortRange::new(0x40, 4),
            )
        };
        let mut pic = Pic::new(master, slave, elcr);
        pic.configure_network(irq);
        pit::stop_channel0(pit);
        Self {
            pic,
            timer: super::apic::initialize(),
            irq,
        }
    }

    /// None means NIC-only sleep; zero means immediate foreground progress.
    /// The caller holds no IRQ-shared driver state: stubs only set IRQ_PENDING.
    pub fn wait(&mut self, nic: &mut Net<'_>, delay_ms: Option<u64>) -> Result<(), Error> {
        if delay_ms == Some(0) || !nic.prepare_sleep()? {
            return Ok(());
        }
        if let Some(delay) = delay_ms {
            self.timer.arm(delay).expect("calibrated APIC timer");
        }
        self.pic.arm(self.irq);
        // SAFETY: CPL0, IF=0, returning IDT entries installed, routes configured.
        // Queue notifications are armed and used rings rechecked. Pending PIC
        // interrupts survive this gap; STI's shadow makes enable+HLT atomic.
        // Stubs touch no borrowed Rust state and return with IF=0. No nomem:
        // device/publication checks must remain ordered before sleeping.
        unsafe {
            core::arch::asm!("sti", "hlt", "cli", options(nostack));
        }
        self.pic.mask_all();
        self.timer.cancel();
        let pending = IRQ_PENDING.swap(0, Ordering::Acquire);
        if pending & (1 << 16) != 0 {
            self.timer.finish_timer();
        }
        for irq in 0..16 {
            if pending & (1 << irq) != 0 {
                self.pic.finish(irq);
            }
        }
        nic.begin_poll();
        Ok(())
    }
}
