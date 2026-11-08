# DR-010: Preempt two isolated processes at user return boundaries

- Status: Accepted
- Date: 2026-11-08

## Decision

Add a fixed two-slot process table for the single-core userspace image. Each
slot owns pinned page tables/backing, a kernel entry stack, saved general and
FP/SIMD/segment state, queue authority, stdout grants, and terminal outcome.
The same embedded ELF may run twice with different initial arguments.

Schedule runnable slots round-robin in 10 ms local APIC quanta. Switch only
from userspace or after a complete bounded syscall. Kernel Rust runs with IF
clear and is never suspended. The saved entry frame is copied into process
storage, then a selected peer's frame/FP state replaces the return frame.
Update CR3, TSS RSP0, and the syscall stack pointer before returning to ring 3.
There is no suspended kernel call stack or kernel-thread scheduler.

Account an approximately one-second per-process CPU execution budget, including
bounded kernel work on its behalf. A quantum expiry resumes a peer rather than
killing the owner. Exhausting the budget terminates only that process. A trusted
clock deadline, not timer delivery alone, determines whether a quantum expired;
acknowledge canceled/stale local timer deliveries without unfair early rotation.

Exit, user fault, invalid return context, or budget exhaustion retires the slot
and resumes a runnable peer. Restore the saved bootstrap context and stop the
timer when all slots finish. Reuse/reset slots only after the batch has stopped;
all old backing, saved state, queue authority, and grants are reinitialized.

Expose a kernel-owned per-process quantum-expiry count at offset 8 of the
read-only SQ-consumer page. It is diagnostic, not queue authority. Existing
queue codecs and native syscalls remain unchanged. The bootstrap descriptor
stays immutable. A CLI peer option selects the optional second instance.

## Rationale

The existing user entry already saves the state needed for resumption. Switching
at a user return boundary avoids synchronizing live VM, UART, or kernel borrows.
Two fixed slots prove fairness and isolation before adding allocation, blocking
services, or dynamic lifetimes. Keeping a separate CPU budget preserves the
existing infinite-loop containment while proving a healthy peer can progress.

## Context

DR-009 runs one protected process at a time and terminates it on a one-second
APIC budget. CLI scenarios now select that embedded program's initial argument.
The next milestone is actual resumable preemption and independent peer recovery.

## Consequences

The userspace image can run two isolated program instances with the same virtual
layout and different physical backing. CPU-bound loops no longer monopolize the
processor. IRQ state must preserve arithmetic/DF flags and per-process SIMD and
segment state. Kernel-owned UART hardware remains a single serialized owner;
file grants are resolved against the currently selected process.

There is no SMP, arbitrary process count, fork/exec, blocking wait, kernel
preemption, or concurrent networking. A long bounded syscall may defer quantum
handling. CPU-time limits are approximate, calibrated millisecond accounting,
not hard real-time guarantees. All-finished idle is a halted kernel.

## Considered alternatives

- **Cooperative yield only:** does not contain a CPU-bound loop or demonstrate
  timer-driven fairness and is rejected.
- **Save/resume arbitrary kernel stacks:** requires synchronization and
  interrupt-safe lifetimes unnecessary for the current bounded syscall model.
- **General dynamic scheduler first:** introduces allocation and lifecycle
  complexity before two-process isolation has native coverage.
- **Keep one global address space/queue:** cannot establish independent memory,
  capability, or completion authority and is rejected.
