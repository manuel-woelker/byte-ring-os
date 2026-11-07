# Local APIC deadline timers

## Goal and scope

Replace PIT channel-0 deadline wakeups with a calibrated local xAPIC one-shot
timer on the supported single-core QEMU PC. Preserve timer-free idle, NIC INTx
wakeups, deferred device ownership, TCP retransmission, and HTTP deadlines.
No IOAPIC migration, x2APIC, TSC-deadline requirement, scheduler, or SMP.

## Design

- Discover CPUID APIC support and validate IA32_APIC_BASE before accessing
  registers. Reject unsupported or disabled/x2APIC configurations explicitly.
- Map the discovered APIC page uncached through the existing boot MMIO mapper.
  Keep register programming in a safe HAL-backed driver and CPU discovery in
  kernel bootstrap code.
- Enable local APIC virtual-wire operation: PIC via LINT0 ExtINT, LINT1 NMI,
  masked unused LVT entries, and a returning spurious vector. Reserve vector
  48 for the local timer, preserving PIC vectors 32–47.
- Calibrate the divided 32-bit APIC counter against the bounded PIT channel-2
  sample. Round deadline counts up with wide arithmetic and cap only at the
  APIC counter's hardware range. Stop/mask on each wake and EOI local timer
  delivery separately from PIC delivery. No periodic tick or PIT IRQ0 fallback.
- Retain CPU/device ownership and notification recheck across `sti; hlt; cli`.
  Stale pending timer deliveries are acknowledged without becoming work.

## Checklist

- [x] Record the decision and add tested APIC initialization, calibration,
  count conversion, cancellation, and EOI behavior.
- [x] Connect discovered local APIC, returning vectors, and idle deadlines;
  remove PIT channel-0 arming and optional timer routing from the PIC.
- [x] Extend QMP tests to inspect an active one-shot APIC timer and show a
  deadline longer than 55 ms counts down without repeated PIT wakeups.
- [x] Update docs, run `./n check`, commit, and move this plan to completed.

## Verification and risks

Host tests exercise unsupported APIC layouts, calibration failure, count
rounding/overflow, stop-before-rearm, cancellation, and EOI. QEMU tests verify
quiet halted idle, NIC/burst wakeup, APIC configuration and long countdown,
TCP loss recovery, request timeouts, and repeated rearm after network wakeups.
The existing boot/fault and ping tests remain required.

Firmware INTx routing and single-core TSC assumptions remain. A count-down
APIC timer has a finite range and may not continue in deeper CPU power states;
only HLT on the supported QEMU PC is in scope. PIT channel 2 remains a boot
reference, not an interrupt source or runtime deadline fallback.

## Results

Completed with
[DR-008](<../../decisions/DR-008 Use the local APIC for deadline wakeups.md>).
The enabled BSP xAPIC is discovered, mapped through HAL MMIO, and calibrated
against PIT channel 2. PIC IRQ0 remains masked and its runtime deadline driver
has been removed. Timer and spurious entries preserve foreground ownership.

`./n check` passed: 88 host tests, host/native linting, ordinary boot/fault
QEMU cases, ping, and HTTP/TCP integration. QMP verified a multi-second local
APIC one-shot retained its initial count while the current count decreased
across 150 ms, with no PIC timer activity. A NIC interrupt then rearmed the
remaining deadline, and the five-second header timeout completed. Existing
loss/retransmission, repeated connections, burst handling, halted quiet idle,
and client reset recovery remain passing.

The supported platform is still one QEMU PC CPU with enabled xAPIC and
firmware PIC routing. TSC-deadline, x2APIC, IOAPIC/MSI-X, and deeper CPU sleep
states are excluded. The 32-bit APIC count still bounds exceptionally long
individual waits; normal TCP/HTTP deadlines no longer require PIT-sized wakes.
