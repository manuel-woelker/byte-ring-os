# DR-008: Use the local APIC for deadline wakeups

- Status: Accepted
- Date: 2026-11-07

## Decision

Use the boot CPU's local xAPIC one-shot count-down timer for network-image
idle deadlines. Discover its presence with CPUID and read IA32_APIC_BASE;
require enabled xAPIC and a supported, page-aligned mapping below 4 GiB.
Configure the mapped registers through the compile-time MMIO HAL. Use divide
128 and calibrate the counter against PIT channel 2 during boot. Convert
milliseconds to counts with rounded-up rational arithmetic, saturating at the
32-bit counter maximum rather than waking every PIT countdown interval.

Reserve vector 48 for timer delivery and vector 255 for APIC spurious delivery.
Keep PIC vectors 32–47 and NIC routing through LINT0 ExtINT; keep LINT1 NMI.
Mask unused LVT entries and clear task priority for the supported single CPU.
Timer entries retain the register-preserving deferred-IRQ pattern. Foreground
code stops the timer on every wake, gives local timers a local APIC EOI, and
completes PIC interrupts separately. Spurious APIC delivery receives no EOI.

Remove PIT channel-0 deadline arming and keep IRQ0 masked. A quiet listener arms
no timer. PIT channel 2 remains the boot calibration reference for TSC and APIC;
unsupported hardware/calibration fails clearly instead of silently busy-polling.

This replaces the timer and virtual-wire portions of
[DR-007](<./DR-007 Sleep between network events using PIC interrupts.md>), while
retaining its foreground ownership, NIC notification recheck, and atomic sleep.

## Rationale

The PIT's 16-bit countdown requires approximately 55 ms wakeups while waiting
for much longer deadlines. A divided 32-bit APIC counter can wait for normal
TCP/HTTP deadlines in one countdown. Calibrating avoids assuming an APIC bus
frequency. xAPIC count-down mode works with the baseline QEMU CPU without
requiring the optional TSC-deadline or x2APIC features.

## Context

Interrupt-driven network idle already works with PIC-routed virtio INTx and
PIT one-shot deadlines. The next improvement is reducing needless deadline
wakeups without adding a scheduler or changing NIC ownership.

## Consequences

Normal deadlines no longer require repeated PIT IRQ0 wakeups. The APIC counter
still has a finite range; an unusually long deadline is re-evaluated after a
maximum-length count. Virtual-wire configuration preserves the existing PIC
NIC path, but IOAPIC/MSI-X and multi-core routing remain future work. Only HLT
on the supported QEMU PC is validated; deep sleep power states are not covered.
The kernel now requires an enabled, compatible local xAPIC for network images.

## Considered alternatives

- **TSC-deadline mode:** elegant absolute deadlines, but not available on the
  baseline QEMU CPU; count-down mode avoids changing the platform requirement.
- **CPUID frequency assumptions:** not all CPUs enumerate a usable APIC timer
  frequency, so a measured reference is retained.
- **Move NICs to IOAPIC/MSI-X:** useful later, but independent of replacing the
  deadline clock and would expand routing and discovery work unnecessarily.
- **Keep PIT fallback:** would obscure unsupported APIC configurations and
  leave two runtime deadline mechanisms to maintain.
