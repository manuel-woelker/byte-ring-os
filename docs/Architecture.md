# Architecture

## Implemented foundation

- `crates/hal` exposes owned byte port ranges. Centralized `cfg` wiring selects
  native x86_64 instructions or a host test implementation with scripted reads
  and an ordered operation log. Test state belongs to each individual test.
- `crates/drivers` contains a safe, allocation-free 16550 serial transmitter.
  It initializes the UART and polls with a bounded per-byte budget. Raw output
  preserves bytes; `core::fmt::Write` uses the same path.
- `crates/kernel` boots through a Multiboot v1 adapter, enters 64-bit Rust,
  writes `Byte Ring OS: COM1 ready` to COM1, and halts. Panics use a bounded
  direct diagnostic path without a global logger lock. CPU exceptions produce
  a fatal register/error report; page faults include CR2 and access details.
- `crates/kernel-core` contains portable `no_std` code. It currently defines an
  initial instruction budget plus portable exception frames and diagnostic
  formatting; ring buffers and VM behavior are future work.

See
[DR-001](<decisions/DR-001 Select the hardware abstraction layer at compile time.md>)
for HAL selection, ownership, and testing contracts. Drivers own concrete HAL
handles; they do not contain assembly or backend conditionals. Unsafe hardware
port access stays in the native HAL. The kernel owns the bootstrap assembly,
CPU halt loop, and explicit device ownership claims.

## Initial boot contract

The boot format follows the
[Multiboot v1 specification](https://www.gnu.org/software/grub/manual/multiboot/multiboot.html).
The linker places a Multiboot header at physical address 1 MiB. A flat `.bin`
image carries explicit load, BSS, and 32-bit entry addresses. The adapter clears
BSS, creates a 64 KiB stack and page tables, enables long mode and SSE support,
and enters Rust with interrupts disabled. It identity maps the first GiB using
supervisor-writable 2 MiB pages. This bootstrap map provides no W^X separation.

The supported initial platform is QEMU's single-core x86_64 PC with COM1 at
`0x3f8`. The adapter does not consume Multiboot discovery information. At the
start of Rust entry, the kernel installs an IDT, GDT, and TSS for fatal
exception diagnostics, with separate emergency stacks for double faults, NMIs,
and machine checks. This is a development boot path, not a general hardware boot
guarantee. There is no allocator, scheduler, SMP, device interrupt handling, or
memory manager. Maskable interrupts remain disabled.

Host tests enable the explicit `test-hal` feature. Production builds reject it;
host builds reject the native I/O backend. Separate Cargo invocations avoid
unifying the test backend into the real kernel. The QEMU integration image uses
the native HAL, checks its COM1 output byte for byte, and exits through a
separate emulated test device only after transmission succeeds. The normal
kernel image does not contain that test exit path.

See [DR-002](<decisions/DR-002 Report fatal CPU exceptions through COM1.md>) for
exception handling. All exception handlers halt, including breakpoints; there
is no recovery or return ABI yet. Assembly normalizes CPU error-code frames and
captures CR2 before calling Rust. Diagnostic code has a recursion guard and
bounded UART polling. Faults before IDT installation and failures in the
reporting path itself may still prevent output.

QEMU fault images execute real divide, invalid-opcode, selector, unmapped-page,
and invalid-stack faults, plus a Rust panic. The guest checks vector/error
codes, instruction addresses where defined, and CR2 before signaling success.
The double-fault case also verifies that its frame is on the emergency stack
and preserves the invalid interrupted stack pointer.

## Intended interaction model

Clients submit versioned requests and bounded bytecode through submission ring
buffers. The kernel validates the request, resolves capabilities, executes a
bounded amount of VM work, and publishes a correlated completion. Notifications
wake consumers when needed; queues alone do not solve sleeping and wakeup races.

“All kernel interactions” describes the intended client-facing programming
model. Hardware interrupts, CPU exceptions, bootstrapping, and the mechanism
that initially establishes shared queues still require privileged native code.
The exact notification and privilege-transition mechanisms remain undecided.

## Contracts to establish before implementation

- Specify request encoding, version negotiation, completion IDs, and error
  semantics without embedding native pointers or Rust layout in the ABI.
- Specify buffer ownership and lifetimes, queue capacity, backpressure,
  overflow, cancellation, and teardown while requests remain outstanding.
- Begin with a single-producer/single-consumer queue; document memory ordering
  and address-space trust boundaries before adding concurrent producers.
- Treat shared memory as untrusted: prevent validation/execution races by
  snapshotting inputs or enforcing ownership. Safe Rust alone cannot prevent
  another address space from mutating a validated program.
- Define verifier rules, instruction semantics, capability rights, memory and
  instruction budgets, yielding, and fault isolation before privileged effects.
- Keep VM authority separate from execution: bytecode must never grant rights
  merely by naming an operation or handle.

## Next milestones

1. Establish explicit memory permissions and interrupt-controller/device
   interrupt handling before introducing concurrent device access.
2. Define a minimal versioned request/completion format and test bounded queue
   wraparound, full/empty behavior, and ownership on the host.
3. Implement a small interpreter and verifier with malformed-program tests and
   deterministic resource exhaustion behavior.
4. Connect one capability-controlled kernel operation end to end, then measure
   batching benefits and tail latency against a simple baseline.

Record consequential choices in [decision records](decisions/). Keep this page
about current boundaries and intended contracts; do not describe proposed
features as implemented.
