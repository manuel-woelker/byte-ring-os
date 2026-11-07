# Architecture

## Implemented foundation

- `crates/hal` exposes owned width-aware port ranges and bounded MMIO/DMA
  regions. Centralized `cfg` wiring selects native x86_64 instructions or a host
  test implementation with scripted reads and an ordered operation log. Test
  state belongs to each individual test.
- `crates/drivers` contains a safe, allocation-free 16550 serial transmitter,
  PCI discovery, modern virtio-net transport, and split DMA virtqueues.
  It initializes the UART and polls with a bounded per-byte budget. Raw output
  preserves bytes; `core::fmt::Write` uses the same path.
- `crates/kernel` boots through a Multiboot v1 adapter, enters 64-bit Rust,
  executes WRITE to stdout handle 0 to print `Byte Ring OS: COM1 ready`
  through COM1, and halts. Panics use a bounded
  direct diagnostic path without a global logger lock. CPU exceptions produce
  a fatal register/error report; page faults include CR2 and access details.
- `crates/kernel-core` contains portable `no_std` code. It currently defines an
  allocation-free bytecode encoder, verifier, interpreter, file-service
  boundary, and portable exception diagnostics. The VM supports RETURN with an
  immediate result and WRITE with an immediate file handle and inline bytes.

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

## Network image

The separate `network` feature boots a modern PCI virtio-net device and runs a
bounded polling loop with static guest IPv4 `192.0.2.2/24`. It responds to ARP
and valid ICMP echo requests and logs ordinary ping traffic through bytecode
WRITE to stdout. Portable packet code lives in kernel-core; register access
and raw DMA operations use the compile-time HAL. The driver never exposes DMA
or MMIO handles through the VM.

PCI capabilities select validated BAR windows rather than fixed addresses.
The boot-owned mapper adds uncached supervisor mappings above 1 GiB and below
4 GiB. Permanently pinned static queue/buffer storage uses coherent x86 DMA
without an IOMMU. Queue ownership, bounds, and publication ordering remain
explicit. No heap, scheduler, or device interrupt service is needed yet.

See [Networking](Networking.md) for topology, tests, TAP instructions, and
limits, and
[DR-005](<decisions/DR-005 Use polled virtio networking with static IPv4.md>)
for decisions. The rootless Unix-stream test verifies packets independently;
host TAP ping is an optional separate workflow. Ordinary boot/fault images do
not require a NIC. TCP/UDP, general sockets, and application networking are
unimplemented. Polling consumes idle CPU; device-driven sleep remains future
work.

## Intended interaction model

Each user application has one initial submission queue (SQ) and completion
queue (CQ), established at application startup. It writes bounded bytecode
programs into SQ entries and release-publishes them, then uses the x86_64 CPU
`syscall` instruction to request synchronous processing of a bounded batch.
The kernel copies each program into private memory, validates and executes it
against granted capabilities, and publishes one correlated CQ completion.
The application drains its CQ before requesting more work when it fills.

This is the accepted architecture in
[DR-003](<decisions/DR-003 Submit kernel bytecode through application queue pairs.md>).
See [the kernel API design](Kernel%20API.md) for entry fields, ownership,
publication, backpressure, syscall progress, execution limits, and errors.
The bytecode/service layer is implemented and described in
[Bytecode API](Bytecode%20API.md) and
[DR-004](<decisions/DR-004 Write through file handles with typed bytecode operands.md>).
The boot file context grants only stdout handle 0, backed by the UART driver;
the VM can dispatch other handle numbers through a different context. Whole
program validation precedes file effects. Memory-reference mode is reserved
and rejected. Queues, userspace, and CPU syscall entry remain unimplemented.

The initial queues are single-producer/single-consumer and programs complete
in submission order before the syscall returns. There is no background poller,
blocking wait, or asynchronous service execution yet. CPU entry requests queue
progress; individual VM service calls carry the actual kernel operations.
Kernel entry, queue processing, VM execution, capability-controlled services,
and native drivers remain separate layers.

“All kernel interactions” describes the intended application-facing model.
Hardware interrupts, CPU exceptions, bootstrapping, queue establishment, and
native drivers still require privileged code. VM authority never includes raw
HAL handles or port access. User-fault containment and proper user/supervisor
mappings must precede isolated applications.

## Next milestones

1. Establish explicit memory permissions and interrupt-controller/device
   interrupt handling before introducing concurrent device access.
2. Specify the wire codec for the accepted queue API and implement bounded
   rings, ownership, CQ backpressure, and hostile-control-state tests.
3. Connect the existing bytecode interpreter to queue consumption and map
   validation, limit, file, and short-write errors into CQ completions.
4. Add application isolation and syscall entry/return, connect a bounded stdout
   file handle end to end, then measure batching and tail latency.

Record consequential choices in [decision records](decisions/). Keep this page
about current boundaries and intended contracts; do not describe proposed
features as implemented.
