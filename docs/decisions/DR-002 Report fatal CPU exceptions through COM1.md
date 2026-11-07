# DR-002: Report fatal CPU exceptions through COM1

- Status: Accepted
- Date: 2026-11-07

## Decision

Install an x86_64 interrupt descriptor table (IDT), task state segment (TSS),
and extended global descriptor table (GDT) at the start of `kernel_main`, before
initializing device drivers. Treat all exceptions as terminal in this initial
kernel: report the cause over bounded, polling COM1 output, then halt without
resuming the interrupted code. Keep maskable hardware interrupts disabled.

Provide separate assembly entries for vectors 0 through 31, normalizing
CPU-pushed error codes and exceptions without error codes into one frame.
Capture CR2 before entering Rust and align the stack for the ordinary C ABI.
Since these handlers never return, they do not preserve general or SIMD
registers and do not execute `iretq`.

Reports include the vector/name, RIP, CS, RFLAGS, RSP, SS, and error code.
Page faults additionally include CR2 and decoded access/protection bits. Keep
frame representation and formatting in portable kernel-core code for host
tests; keep privileged descriptor-table setup and entry assembly in the kernel,
alongside its existing native boot adapter. Device output still goes through
the selected HAL and UART driver.

Assign independent 32 KiB interrupt stack table (IST) stacks to double faults,
NMIs, and machine checks. In particular, a double fault must not depend on the
stack that caused exception delivery to fail. Avoid a global logger lock; fatal
handlers permanently take over COM1 from interrupted code. A recursion guard
halts immediately if diagnostics themselves fault or panic.

## Rationale

Early CPU faults otherwise look like a silent hang or reset. A fixed, small
report gives enough context to investigate while requiring no allocator,
scheduler, interrupts, unwinding, or working application stack for double
faults. Terminal handlers allow ordinary stable Rust behind a short assembly
ABI adapter instead of introducing a compiler-specific interrupt ABI.

Host tests verify output contracts and error handling. QEMU tests validate
actual CPU stack frames, error codes, fault instruction addresses, CR2, and the
double-fault emergency stack. Test success requires both diagnostic output and
an explicit guest validation result, not merely a matching printed label.

## Context

The initial kernel boots a single x86_64 QEMU CPU and exposes COM1 through the
compile-time HAL selected in DR-001. It has no userspace, scheduler, recoverable
fault contract, or device interrupt handling. Debug output is available before
the planned bytecode VM and interaction queues exist.

Hardware frame and descriptor formats follow the
[AMD64 system programming manual](https://www.amd.com/content/dam/amd/en/documents/processor-tech-docs/programmer-references/24593.pdf).

## Consequences

- Boot and deliberate divide, invalid-opcode, general-protection, page-fault,
  double-fault, and panic cases run in QEMU as part of `./n check`.
- Panic diagnostics include their message and source location.
- Even breakpoints are fatal for now. Returning handlers, register preservation,
  and debugger integration require a separate design before implementation.
- This protects faults after IDT installation; the preceding 32-bit/64-bit
  bootstrap transition is still outside this diagnostic path.
- Reporting is best effort: a faulty UART, recursive fault, corrupted tables,
  or triple fault may prevent complete output. Polling bounds CPU work per
  byte, not elapsed time or remote delivery.
- The GDT/TSS/IDT and emergency stacks belong to the boot CPU. SMP requires
  per-CPU tables, stacks, and diagnostic ownership.
- Maskable interrupts remain unsupported. The remaining IDT entries report an
  unexpected interrupt and halt; no interrupt-controller acknowledgement is
  attempted.

## Considered alternatives

### Continue after every exception

Rejected because the kernel has no recovery policy and faults can indicate
corrupted state. Resuming the same faulting instruction could simply fault
again; general recovery requires a different entry/return contract.

### Use the current stack for every exception

Rejected because stack exhaustion or an invalid stack can make exception
delivery fail. A separate double-fault IST is necessary to diagnose that case.

### Print only an exception label

Rejected because it hides the instruction, address, and error information
needed to distinguish a bad mapping from a selector or instruction failure.

### Switch to a nightly interrupt ABI

Rejected because non-returning assembly entries plus stable Rust meet the
current diagnostic needs without changing the pinned compiler channel.
