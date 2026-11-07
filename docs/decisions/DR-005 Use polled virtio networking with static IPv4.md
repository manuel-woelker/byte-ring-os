# DR-005: Use polled virtio networking with static IPv4

- Status: Accepted
- Date: 2026-11-07

## Decision

Implement modern PCI virtio-net with split RX/TX queues, static DMA storage,
and polling on the initial single QEMU CPU. Configure guest `192.0.2.2/24`,
peer `192.0.2.1`, and MAC `52:54:00:12:34:56`. Support a bounded Ethernet,
ARP, IPv4, and ICMP echo responder; no general socket stack is required.

Use typed, bounded HAL port/MMIO/DMA access instead of exposing ordinary Rust
references to device-mutated memory. Kernel-owned static DMA buffers stay alive
for the lifetime of the device. Publish descriptors before queue indices with
release ordering; acquire device completions before consuming their buffers.
The initial platform assumes coherent x86 DMA without an IOMMU.

Map only validated PCI BAR windows above the initial RAM identity mapping, using
supervisor, uncached mappings and statically reserved page tables. Reject
addresses outside the initial below-4-GiB MMIO mapper, overlapping RAM,
malformed PCI capabilities, and unsupported features instead of truncating
addresses.

Use a Unix-stream Ethernet peer for unprivileged automated tests and a separate,
explicit preconfigured TAP workflow for ordinary host ping. Keep container
networking disabled. Log normal ping traffic through bytecode WRITE to stdout;
keep logging bounded and separate from descriptor reclamation.

## Rationale

Virtio provides a documented VM-oriented interface and useful ownership rules
without emulating a physical NIC's legacy register behavior. Polling and static
storage allow useful networking before an allocator, scheduler, or interrupt
controller exists. A small responder keeps protocol behavior independently
unit-testable and limits the initial attack surface.

## Context

The existing kernel has boot, faults, HAL port I/O, a bytecode VM, and COM1, but
no PCI, DMA, MMIO mapping, or networking. The implementation follows the
[networking plan](<../plans/completed/2026-11-07 Virtio networking and static IPv4 ping.md>).
QEMU user-mode NAT does not provide inbound host ICMP forwarding.

## Consequences

- The device owns published DMA buffers until a validated completion returns.
  Descriptor corruption stops processing; storage is never freed underneath DMA.
- Network parsing failures drop packets, not halt the kernel.
- Polling consumes idle CPU. Interrupt-driven sleep and per-CPU state are future
  work, as are application-facing network handles and SQ/CQ integration.
- The initial MMIO/DMA layout is QEMU PC-specific, not physical-hardware
  support.
- TAP setup remains explicit and optional; ordinary checks need no host routes,
  raw sockets, bridge, firewall changes, or network-administration capabilities.

## Considered alternatives

- Legacy virtio PCI: rejected to avoid maintaining a second transport.
- A general allocator and interrupts first: deferred because bounded static
  storage and polling satisfy this milestone with fewer dependencies.
- A full TCP/IP stack: deferred until protocols beyond ARP and ping are needed.
- QEMU NAT and host forwarding for ping: rejected because TCP/UDP forwarding
  does not deliver inbound ICMP echo requests to this guest.
