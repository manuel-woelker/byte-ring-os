//! Single-CPU polled network host; logs ordinary requests through stdout WRITE.

use super::files::BootFiles;
#[cfg(not(feature = "http"))]
use byte_ring_drivers::virtqueue::Error as QueueError;
use byte_ring_drivers::{pci::Pci, virtio_net::Net, virtqueue::DMA_SIZE};
use byte_ring_hal::{memory::Memory, port::PortRange};
#[cfg(not(feature = "http"))]
use byte_ring_kernel_core::network::{MAX_FRAME, Network};
use byte_ring_kernel_core::{
    bytecode::{
        builder::ProgramBuilder,
        vm::{Limits, execute},
    },
    file::STDOUT,
};
use core::{
    cell::UnsafeCell,
    fmt::{self, Write},
};

pub const IP: [u8; 4] = [192, 0, 2, 2];
#[repr(C, align(4096))]
struct Dma([u8; DMA_SIZE]);
struct DmaCell(UnsafeCell<Dma>);
// SAFETY: exclusively claimed at network boot and permanently pinned. Access
// uses raw HAL operations, never references to device-written packet storage.
unsafe impl Sync for DmaCell {}
static RX: DmaCell = DmaCell(UnsafeCell::new(Dma([0; DMA_SIZE])));
static TX: DmaCell = DmaCell(UnsafeCell::new(Dma([0; DMA_SIZE])));

struct Text {
    bytes: [u8; 256],
    length: usize,
}
impl Write for Text {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.length.checked_add(text.len()).ok_or(fmt::Error)?;
        let destination = self.bytes.get_mut(self.length..end).ok_or(fmt::Error)?;
        destination.copy_from_slice(text.as_bytes());
        self.length = end;
        Ok(())
    }
}
pub(super) fn log(files: &mut BootFiles, args: fmt::Arguments<'_>) {
    let mut text = Text {
        bytes: [0; 256],
        length: 0,
    };
    if text.write_fmt(args).is_err() {
        return;
    }
    let mut bytes = [0; 300];
    let mut builder = ProgramBuilder::new(&mut bytes);
    if builder.write(STDOUT, &text.bytes[..text.length]).is_err() || builder.return_imm(0).is_err()
    {
        return;
    }
    if let Ok(program) = builder.finish() {
        let _ = execute(program, files, Limits::default());
    }
}

pub(super) enum InitError {
    Pci(byte_ring_drivers::pci::Error),
    Mapping(&'static str),
    Device(byte_ring_drivers::virtio_net::Error),
}
impl fmt::Display for InitError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pci(error) => write!(output, "PCI discovery: {error:?}"),
            Self::Mapping(error) => write!(output, "MMIO mapping: {error}"),
            Self::Device(error) => write!(output, "virtio initialization: {error:?}"),
        }
    }
}

pub(super) fn initialize() -> Result<(Net<'static>, u8), InitError> {
    // SAFETY: one CPU at CPL0, interrupts disabled, no other PCI or DMA owner.
    unsafe {
        let mut pci = Pci::new(PortRange::new(0xcf8, 8));
        let device = pci.discover().map_err(InitError::Pci)?;
        let common = super::mmio::map(device.common).map_err(InitError::Mapping)?;
        let notify = super::mmio::map(device.notify).map_err(InitError::Mapping)?;
        let isr = super::mmio::map(device.isr).map_err(InitError::Mapping)?;
        let config = super::mmio::map(device.device).map_err(InitError::Mapping)?;
        let rx = Memory::claim(RX.0.get().cast::<u8>(), DMA_SIZE, RX.0.get() as u64);
        let tx = Memory::claim(TX.0.get().cast::<u8>(), DMA_SIZE, TX.0.get() as u64);
        pci.enable(device.bdf);
        Net::new(
            common,
            notify,
            isr,
            config,
            device.notify_multiplier,
            rx,
            tx,
        )
        .map(|net| (net, device.irq))
        .map_err(InitError::Device)
    }
}
#[cfg(not(feature = "http"))]
pub fn run(files: &mut BootFiles) -> ! {
    let (mut nic, irq) = initialize().unwrap_or_else(|error| panic!("network: {error}"));
    let mut idle = super::idle::Idle::new(irq);
    let network = Network {
        ip: IP,
        mac: nic.mac,
    };
    log(
        files,
        format_args!(
            "virtio-net ready ip=192.0.2.2/24 mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\r\n",
            nic.mac[0], nic.mac[1], nic.mac[2], nic.mac[3], nic.mac[4], nic.mac[5]
        ),
    );
    let mut input = [0; MAX_FRAME];
    let mut output = [0; MAX_FRAME];
    let mut drops = 0u64;
    let mut full = 0u64;
    let mut suppressed = 0u64;
    loop {
        nic.begin_poll();
        let mut log_budget = 1;
        for _ in 0..16 {
            let length = match nic.receive(&mut input) {
                Ok(Some(length)) => length,
                Ok(None) => break,
                Err(error) => panic!("network RX: {error:?}"),
            };
            let reply = match network.respond(&input[..length], &mut output) {
                Ok(reply) => reply,
                Err(_) => {
                    drops += 1;
                    continue;
                }
            };
            let sent = match nic.transmit(&output[..reply.length]) {
                Ok(()) => true,
                Err(byte_ring_drivers::virtio_net::Error::Queue(QueueError::Full)) => {
                    full += 1;
                    false
                }
                Err(error) => panic!("network TX: {error:?}"),
            };
            if let Some(echo) = reply.echo {
                if log_budget > 0 {
                    log_budget -= 1;
                    log(
                        files,
                        format_args!(
                            "ICMP echo src={}.{}.{}.{} dst=192.0.2.2 id={} seq={} bytes={} reply={} drops={} tx_full={} suppressed={}\r\n",
                            echo.source[0],
                            echo.source[1],
                            echo.source[2],
                            echo.source[3],
                            echo.identifier,
                            echo.sequence,
                            echo.payload_bytes,
                            if sent { "queued" } else { "tx-full" },
                            drops,
                            full,
                            suppressed
                        ),
                    );
                } else {
                    suppressed += 1;
                }
            }
        }
        idle.wait(&mut nic, None)
            .unwrap_or_else(|error| panic!("network idle: {error:?}"));
    }
}
