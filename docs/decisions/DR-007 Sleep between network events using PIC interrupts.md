# DR-007: Sleep between network events using PIC interrupts

- Status: Accepted
- Date: 2026-11-07

## Decision

Use the supported QEMU PC's 8259 PIC, firmware-assigned PCI INTx line, and PIT
channel 0 one-shot timer to idle both network images with `sti; hlt; cli`.
Keep interrupts disabled while foreground code processes devices or HTTP.
IRQ entries only record the pending IRQ and clear IF in the saved return frame.
They preserve all general/SIMD state and call no Rust. Foreground code masks
interrupt routes and completes PIC acknowledgement through HAL-owned ports.

Suppress virtqueue interrupts while polling; before sleep enable notifications,
issue an ordering barrier, and recheck RX/TX used rings. Recheck failures or
pending completions prevent sleep. The PIC holds any interrupt between this
check and HLT; STI's interrupt shadow closes the enable/sleep race.

For HTTP, select the earliest smoltcp poll time, request deadline, or connection
limit. Arm PIT channel 0 only if a deadline exists, rounding up and capping one
countdown to 65,535 ticks. Re-evaluate long deadlines after each timer wake. No
timer runs as an idle tick. PIT channel 2 still calibrates the single-core TSC.

## Rationale

A periodic tick alone would reduce CPU use but could conceal lost device
wakeups and wake an otherwise idle listener unnecessarily. INTx and a one-shot
PIT meet the initial QEMU PC's needs without APIC discovery, MSI-X tables, or a
scheduler. Deferring all work keeps driver ownership, unsafe DMA contracts, and
existing portable protocol code intact.

## Context

The ping and HTTP images busy-poll virtio queues with IF clear. Their idle loops
consume a host CPU even with no traffic. HTTP additionally needs timed TCP
retransmission and request recovery. Existing exception entries are fatal and
cannot return from hardware interrupts.

## Consequences

A quiet listener halts until network activity. Active deadlines may wake at
approximately 55 ms intervals because of the PIT's finite countdown. There is
no scheduler, concurrent interrupt servicing, or general interrupt API. Device
work still runs in bounded foreground polls. Unsupported PCI IRQ assignments
fail explicitly; host firmware routing is assumed valid on the supported PC.

The PIC is masked before EOI and remains masked while the NIC ISR is cleared
in foreground code. IRQ 7/15 spurious acknowledgement rules remain explicit.
No IRQ handler logs or borrows a live device handle. Other images retain their
original fatal diagnostics and interrupts-disabled behavior.

## Considered alternatives

- **Periodic PIT polling:** simpler, but needless wakeups and periodic recovery
  would hide missing device interrupt delivery.
- **APIC/MSI-X immediately:** useful for SMP and more devices, but requires
  additional discovery and routing outside the current platform scope.
- **Process packets inside IRQ handlers:** would require synchronization and
  reentrant device ownership without providing a benefit for one CPU.
- **HLT without notification recheck or atomic enable/sleep:** can miss an
  arrival and stall indefinitely, so is rejected.
