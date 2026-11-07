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
guarantee. There is no allocator, scheduler, SMP, or memory manager. Ordinary
boot/fault images keep maskable interrupts disabled; network images enable them
only while sleeping, as described below.

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

PCI capabilities select validated BAR windows rather than fixed addresses. The
boot-owned mapper adds uncached supervisor mappings above 1 GiB and below 4 GiB.
Permanently pinned static queue/buffer storage uses coherent x86 DMA without an
IOMMU. Queue ownership, bounds, and publication ordering remain explicit. No
heap, scheduler, or concurrent device service is needed. Returning IRQ entries
wake the foreground owner when it sleeps.

See [Networking](Networking.md) for topology, tests, TAP instructions, and
limits, and
[DR-005](<decisions/DR-005 Use polled virtio networking with static IPv4.md>)
for decisions. The rootless Unix-stream test verifies packets independently;
host TAP ping is an optional separate workflow. Ordinary boot/fault images do
not require a NIC. UDP, general sockets, and application networking are
unimplemented. A quiet network image sleeps until a NIC interrupt.

## HTTP image

The separate `http` feature uses the same native virtio device with pinned
[smoltcp](https://github.com/smoltcp-rs/smoltcp) 0.14.0 for Ethernet, IPv4, TCP,
and ICMP echo replies. Default features, heap allocation, offloads, and fragment
reassembly are disabled. A safe driver adapter copies frames into private
buffers, checks TX capacity before offering tokens, and reports device failures
to the kernel. Each interface poll accepts at most 16 frames; pending
completions prevent sleep between batches.

A single boot-owned socket listens at `192.0.2.2:8080` with 2048-byte RX and
1024-byte TX storage. Portable kernel-core code parses at most 1024 header
bytes and selects an embedded page for GET or HEAD `/`. HTTP/1.1 requires Host;
HTTP/1.0 is supported. Responses carry exact Content-Length and close the
connection. Unsupported bodies and ambiguous framing produce errors. Partial
reads/writes, FIN, disconnect, and socket reuse are explicit in the kernel loop.

Request headers have a five-second deadline from TCP activity, and the whole
connection has a fifteen-second limit. The boot CPU calibrates its TSC against
PIT channel 2 through HAL port access; conversion to milliseconds is portable.
This clock assumes the supported single-core QEMU PC. Ordinary response logs use
stdout WRITE and stop after 64 messages; device faults use fatal diagnostics.

The service has no application socket ABI or isolation from the kernel. It is
an experimental development endpoint, without TLS, persistent connections,
request bodies, routing, or production entropy. See
[DR-006](<decisions/DR-006 Serve a bounded HTTP endpoint using smoltcp.md>) for
the dependency and ownership decision.

## Network interrupts and idle

Both network images remap the legacy 8259 PIC to vectors 32–47 and use the
validated firmware-assigned PCI INTx line, configured as level-triggered in
ELCR. The PIC unmasks only the NIC and its cascade route; IRQ0 stays masked.
The local xAPIC keeps PIC delivery on LINT0 ExtINT and NMI on LINT1. A separate
one-shot local timer uses vector 48, and vector 255 handles APIC spurious
interrupts without EOI. Ordinary boot/fault images keep their existing behavior.

Foreground code runs with IF clear. Small assembly entries record pending IRQ
bits and clear IF in the saved return frame; they use no general or SIMD
registers, call no Rust, and access no devices. After HLT returns, foreground
code masks PIC routes, stops/masks the local timer, and completes EOI at the
controller that delivered the interrupt. PIC spurious IRQ 7/15 rules remain
explicit. Queue notifications and the virtio ISR are handled in foreground
code. There is no shared driver borrow or concurrent device servicing.

Before HLT, the NIC enables RX/TX completion notifications, orders publication,
and rechecks both used rings. Pending completions force another poll. Pending
interrupts survive the final check, and adjacent `sti; hlt; cli` closes the
interrupt-enable/sleep race. All PIC, ELCR, PIT, APIC MMIO, and device accesses
use the HAL; CPU discovery/MSR reads stay in kernel bootstrap code.

HTTP waits for the earliest smoltcp or application deadline. CPUID and
IA32_APIC_BASE select a validated enabled BSP xAPIC below 4 GiB; the boot mapper
maps its register page uncached. Unsupported modes fail explicitly. PIT channel
2 calibrates the divide-128 APIC counter at boot. Deadline counts round up with
wide rational arithmetic and saturate at the 32-bit hardware limit. Ordinary
TCP/HTTP deadlines fit one countdown; a NIC wake cancels and rearms the
remaining time. There is no PIT channel-0 deadline or runtime fallback.

Channel 2 also supplies the existing TSC calibration. A listener with no
deadline arms no timer, and the firmware's periodic PIT tick is stopped. The
ping image has no timer fallback. Immediate application work and already-due
deadlines continue foreground processing. Only HLT on the supported single-core
QEMU PC is validated; x2APIC, IOAPIC/MSI-X routing, deep sleep, and scheduling
remain unimplemented.

QMP integration verifies halted quiet idle, NIC/burst wakeup, one-shot APIC
configuration, and a multi-second countdown that remains armed beyond 55 ms
without PIC timer activity. It also checks rearm after a NIC wake, TCP loss
recovery, and request timeouts. See
[DR-007](<decisions/DR-007 Sleep between network events using PIC interrupts.md>)
and its timer successor
[DR-008](<decisions/DR-008 Use the local APIC for deadline wakeups.md>).

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

1. Establish explicit memory permissions and synchronization before introducing
   concurrent device access; extend interrupt routing when SMP is introduced.
2. Specify the wire codec for the accepted queue API and implement bounded
   rings, ownership, CQ backpressure, and hostile-control-state tests.
3. Connect the existing bytecode interpreter to queue consumption and map
   validation, limit, file, and short-write errors into CQ completions.
4. Add application isolation and syscall entry/return, connect a bounded stdout
   file handle end to end, then measure batching and tail latency.

Record consequential choices in [decision records](decisions/). Keep this page
about current boundaries and intended contracts; do not describe proposed
features as implemented.
