# Interrupt driven network idle

## Goal and scope

Replace busy idle loops in the ping and HTTP images with interrupt-driven HLT on
the supported single-core QEMU PC. Preserve bounded foreground processing, all
existing wire behavior, and TCP/HTTP timeouts. No scheduler, SMP, APIC/MSI-X,
userspace, or concurrent device servicing is introduced.

## Design and ownership

- Remap and mask the 8259 PIC, validate the firmware-assigned PCI INTx route,
  and configure its ELCR level bit. Keep all other device interrupts masked.
- IRQ stubs record pending vectors and return with IF clear. They call no Rust,
  touch no general or SIMD registers, and perform no port/device accesses.
  The foreground owner masks routes and performs EOI through HAL-backed drivers.
- Poll with IF clear and queue notifications suppressed. Before sleep, enable
  RX/TX notifications, order publication, and recheck used rings. Pending work
  forces another poll. Pending interrupts bridge the final check to `sti; hlt`.
- Arm PIT channel 0 only for the earliest TCP/application deadline, in bounded
  one-shot intervals (the PIT's maximum is approximately 55 ms). With no
  deadline, sleep indefinitely until the NIC wakes the CPU. Channel 2 remains
  the calibration source. All port operations use the compile-time HAL.
- Keep the existing fatal exception path and ordinary boot images unchanged.

## Checklist

- [x] Record the decision; implement safe PIC/timer drivers and notification
  arming/recheck with host tests.
- [x] Add returning IRQ entries and network/HTTP idle integration with explicit
  ownership, pending-work handling, and deadline selection.
- [x] Extend QEMU verification to observe a halted CPU, wake on network traffic,
  exercise a burst past one poll budget, and retain retransmission/timeouts.
- [x] Update documentation, run `./n check`, commit, and move this plan to
      completed.

## Verification and risks

Host tests cover PIC programming/masks/EOI and spurious IRQs, PIT rounding and
bounds, deadline choice, queue notification flags, and completions observed
while arming. QEMU HTTP tests use QMP register inspection to verify HLT during
quiet idle, then independent Ethernet/TCP traffic to verify wakeup and recovery.
Existing boot/fault/ping tests remain required.

Firmware PCI routing, coherent x86 DMA, and a single CPU are explicit platform
assumptions. Returning interrupts must preserve interrupted state. No Rust
borrow of a driver may be accessed by an IRQ handler. Timer-free idle must not
rely on a periodic tick to hide a lost NIC wakeup. Device removal/reset recovery
and power management beyond HLT remain out of scope.

## Results

Implemented in both network images; see
[DR-007](<../../decisions/DR-007 Sleep between network events using PIC interrupts.md>).
`./n check` passed with 82 host tests and all boot/fault/ping/HTTP QEMU checks.
QMP observed HLT and unchanged IRQ statistics during quiet idle, then successful
NIC wakeup and a 24-packet burst exceeding the 16-frame HTTP receive budget.
Existing independent TCP loss/retransmission and five-second header timeout
checks pass with one-shot timer wakeups. The ping image passes without a timer
fallback. No test guest remains running.

The host Clippy task was also restored to lint portable code and tests; native
network lint now uses the same absolute-sysroot build wrapper as HTTP. The
implementation remains limited to the supported single-core QEMU PC and its
firmware PCI interrupt route; there is no APIC, scheduler, or SMP support.
