# Scenario selection and two process scheduling

## Goal and scope

First expose runtime scenario selection through `./n run-user`, then add a
fixed two-process preemptive scheduler for the userspace image. Commit the CLI
work before scheduler implementation. Preserve queue/VM semantics, existing
isolation tests, and separate network images. No allocator, SMP, dynamic process
creation, blocking services, or kernel preemption.

## Design

- The pinned Nao version has no task argument forwarding. Extend the `n`
  wrapper narrowly for `run-user` arguments, preserving ordinary task graphs.
  Validate selection before launching Nao and pass a bounded Multiboot command
  line to the kernel. Copy/parse loader data before entering userspace.
- Introduce two fixed process slots, each owning page root/backing, saved
  registers/FP state, kernel stack, queue authority, stdout grants, and outcome.
- Use local APIC quanta with round-robin selection. Switch only from userspace
  or a completed bounded syscall; never suspend kernel Rust/device operations.
  Account a per-process CPU budget and acknowledge stale timer delivery.
- Retain a saved bootstrap context until all processes finish. Fault/exit
  retires only its owner and resumes a runnable peer. Stop the timer when none
  remain.
- Verify actual resumption and memory/queue/FP isolation with two CPU-bound demo
  scenarios, an infinite loop paired with a healthy peer, and a faulting peer.

## Checklist

- [x] Implement validated CLI/catalogue, bounded Multiboot parsing, native
  scenario selection tests, usage docs; run `./n check` and commit.
- [ ] Record scheduling decision and implement tested portable round-robin
  timing/lifecycle policy and per-process native resources/contexts.
- [ ] Add real preemption/resumption and adversarial peer QEMU tests; update
  architecture/API/demo documentation and run `./n check`.
- [ ] Commit completed scheduling work and move this plan to completed.

## Verification and risks

Host tests cover command-line validation, malformed/oversized selection,
round-robin order, retired slots, budget accounting, and deadline behavior.
Native tests retain all userspace checks and add actual multi-process register,
SIMD, data, queue and CR3 resumption, peer fault/exit, and watchdog containment.
The CLI is exercised end to end through QEMU's Multiboot `-append` path.

All kernel Rust remains IF=0 on one CPU. No mutable process/state borrow may
span assembly resume. IRQ returns must preserve arithmetic/DF state and user
FP registers. Reusing a slot requires resetting all backing and private queue
state. Hardware timer delivery alone is not authority that a fresh quantum has
expired; trusted clock deadlines decide scheduling.

## CLI results

`./n check` passes with bounded command parsing host tests and native runtime
selection of port-I/O, infinite-loop, and invalid scenarios from the same ELF.
The actual `./n run-user --scenario port-io` command reported scenario 5 and
Fault(13, 0), then its QEMU instance was stopped. The pinned Nao task graph is
preserved by passing the validated selection through an explicit tool env var.
