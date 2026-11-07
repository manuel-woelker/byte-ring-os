# First isolated userspace programs

## Goal and scope

Boot an embedded freestanding Rust ELF at ring 3, submit WRITE/RETURN programs
through a protected SQ/CQ pair, and return to the kernel on exit, fault, or a
one-second execution budget. Deliver a runnable demo and adversarial QEMU tests.
One process at a time, fixed storage, no allocator, scheduler, filesystem,
dynamic linker, TLS, general user pointers, or networking in this image.

## Design

- Use bounded ELF64 ET_EXEC validation and fixed user virtual addresses.
  Reject dynamic/TLS images, overlapping pages, bad entry/ranges, and writable
  executable segments. Copy initialized bytes and zero BSS before execution.
- Build separate page tables retaining supervisor kernel mappings. Map user
  text RX, rodata/descriptor/kernel queue state RO+NX, data/stack/SQ/application
  queue state RW+NX, and leave guards absent. Enable NX and CR0.WP. This is
  userspace isolation; the existing kernel identity map is still permissive.
- Add ring-3 GDT selectors, a TSS RSP0, trusted syscall stack, SYSCALL MSRs,
  register/FP state preservation, strict return-context checks, and IRET return.
  Enter with clean registers/FP state and IF enabled, IOPL zero.
- Specify a fixed little-endian four-slot queue codec, protected scalar indices,
  private authoritative kernel counters, bounded batches, CQ backpressure, and
  copy-before-verify. Native syscall 0 progresses queues; syscall 1 terminates
  the native process as a lifecycle operation. File operations remain bytecode.
- Only mapped, kernel-bound queue addresses are copied; no syscall accepts a
  user buffer pointer. Keep portable ELF/queue logic host-testable. User faults
  terminate only the process. APIC expiry bounds non-cooperative native loops.
- Build a real Rust demo separately and embed its ELF. Tests run fresh instances
  for syscall/queue rejection, mapping and I/O violations, bad stacks, and
  timeout recovery, then run a healthy instance to verify kernel survival.

## Checklist

- [x] Record DR-009 and implement tested ELF validation and queue ABI/engine.
- [ ] Implement Rust userspace demo, page mappings, privilege transitions,
  trusted entry/return, fault containment, and APIC budget.
- [ ] Add demo/integration/lint tasks, adversarial QEMU checks, and current
      docs.
- [ ] Run `./n check`, commit completed units, and move this plan to completed.

## Verification and assumptions

Host tests cover malformed ELF/ranges/permissions/BSS, queue wraparound and
hostile cursor movement, full CQ, snapshot batching, invalid bytecode, denied
handles, and private-copy execution. Native tests check actual CPL3, queue
page permissions, stack guards, NX/write protection, supervisor/I/O denial,
invalid syscall/return context, FP/register preservation, and loop termination.
Existing network and boot/fault checks remain required.

The supported platform is a single-core QEMU x86_64 PC with NX, SYSCALL, and
local xAPIC. Network and userspace images are separate; this milestone does not
make the HTTP server a userspace process. Fixed ELF/address limits are an
experimental ABI, not POSIX compatibility or a production security claim.
