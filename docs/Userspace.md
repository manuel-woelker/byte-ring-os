# Userspace

The `userspace` image runs up to two isolated instances of an embedded Rust ELF
on one CPU. A local APIC timer preempts them round-robin in 10 ms quanta. Each
instance owns private memory, queue state, CPU state, stdout grants, and a
kernel entry stack. This is an experimental process ABI, not POSIX
compatibility.

```sh
./n run-user                         # Default hello/queue scenario
./n run-user -- 6                     # Numeric scenario selection
./n run-user --scenario registers     # Named scenario selection
./n run-user --list-scenarios
./n run-user --scenario task-a --peer task-b
./n run-user --scenario loop --peer hello
./n userspace-test
```

`task-a` (17) and `task-b` (18) are CPU-bound probes. They wait without syscalls
for timer preemption, verify resumed registers/SIMD/segment/data state, and
print three interleaved progress lines through their own SQ/CQ. `loop` (6) keeps
running until its CPU budget expires while its healthy peer continues. Exit,
fault, or timeout affects only its owner. After all selected instances finish,
the kernel prints their outcomes and timer-switch count, stops the timer, and
halts. Ctrl+C stops QEMU.

Selection accepts a number (0–18) or a catalogue name. Unknown selections are
rejected before building. The launcher passes `user-scenario=N` and optional
`user-peer=N` through a bounded copied Multiboot command line. No selection
changes the compiled ELF; without a selection, scenario 0 runs alone. String
argv and selecting different ELF files remain unimplemented. Source is in
`crates/user-demo`; the ELF is built under `target/user/` before embedding.

## Loading and mappings

The portable loader accepts little-endian ELF64 ET_EXEC, x86_64, with at most
eight program headers and four load segments. Segments must be page aligned,
sorted, non-overlapping even at page granularity, and readable with either
write or execute rights, never both. Entry must name file-backed executable
bytes. Dynamic, interpreter, TLS, executable-stack, overflow, and unsupported
headers are rejected. Only PT_NULL and non-executable GNU_STACK are ignored.
The image is immutable kernel-owned data; initialized bytes are copied and BSS
and all old process backing are zeroed before every instance.

| User virtual mapping | Size | User rights |
| --- | --- | --- |
| ELF image at `0x40000000` | Up to 128 KiB | Per-segment RX, RO+NX, or RW+NX |
| Stack at `0x40030000` | 16 KiB | RW+NX |
| Bootstrap at `0x40040000` | One page | RO+NX |
| SQ at `0x40041000` | One page | RW+NX |
| SQ consumer at `0x40042000` | One page | RO+NX |
| CQ/producer at `0x40043000` | One page | RO+NX |
| CQ consumer at `0x40044000` | One page | RW+NX |

Unlisted user pages are absent; stack guards are unmapped. The separate user
root retains supervisor-only kernel/APIC mappings, including supervisor-only
physical aliases of user backing. NX and CR0.WP are enabled. This enforces user
W^X and queue rights; the existing supervisor identity map itself remains RWX.
There is no heap, demand paging, general mmap, filesystem, or dynamic linker.

## Bootstrap, entry, and services

RDI contains the immutable descriptor address; RSI is an opaque initial u64
argument. The stack begins at its mapped end minus eight bytes, matching the
x86_64 C function-entry alignment. General registers other than those arguments
and the stack pointer start zero; FP/SIMD state starts clean. There is no TLS or
FSGSBASE facility. IF is set and IOPL is zero. TSS I/O permissions deny ports.
The image requires NX, SYSCALL, and enabled BSP xAPIC on the supported QEMU PC.

Native syscall 0 processes the current process's queue pair, using the
[Kernel API](Kernel%20API.md) codec. Native syscall 1 exits with status in RDI;
this is lifecycle control. All file operations are bytecode WRITE, with stdout
handle 0 granted by the kernel. There is no native WRITE syscall or arbitrary
user-buffer pointer argument. Unknown native operations and a zero batch limit
consume no submissions.

SYSCALL saves the user stack pointer without touching that stack, switches to a
selected process's 64 KiB kernel entry stack, masks dangerous flags, saves all
general registers and FP/SIMD state, and restores trusted kernel FP state before
Rust. The kernel validates the return instruction against executable file bytes
and the stack against its fixed mapping. Return uses IRET with fixed user
selectors and safe flags. RAX/RDX carry count/stop reason, RCX/R11 are
clobbered, other general registers and FP/SIMD state are preserved. Syscall
return flags reset to `0x202`; returning interrupts preserve arithmetic/DF state
while excluding privileged and unsupported flag controls. No SYSRET fast path is
present.

## Ownership and lifetime

Two fixed slots have distinct physical backing and page roots with the same
user virtual layout. Each slot owns its saved general/FP/SIMD/segment state,
kernel stack, queue authority, grant state, and outcome. Hardware UART access
has one serialized kernel owner; every file call resolves grants for the current
process. No kernel borrow or hardware operation is suspended across a switch.

Private kernel cursors bind each caller to its own pair. User scalar indices
use aligned acquire/release atomics; slot bytes use bounded raw copies. The
kernel never creates a Rust slice of application-mutated storage. A full slot
is copied before SQ release, then that private copy is validated/executed and
one completion is published. Full CQ leaves the next SQ slot unconsumed; bad
index movement disables only that pair. A read-only diagnostic u64 at offset 8
of the SQ-consumer page counts expired quanta. It is not queue authority and
does not alter the immutable bootstrap descriptor.

Kernel Rust runs with IF clear and bounded per-entry work. APIC quanta preempt
user execution; an expired quantum at syscall return defers the switch until
the entire call finishes. A saved return frame/FP state is copied into the
process slot, and a peer's state replaces it. CR3, TSS RSP0, and the syscall
stack pointer follow the selected process before IRET. No kernel stack with a
suspended Rust call is resumed. With one runnable slot, it continues after its
quantum; retired slots are skipped.

Each process has an approximately one-second CPU execution budget, including
bounded kernel work on its behalf. Waiting for a peer consumes none of that
budget. A native infinite loop is terminated on exhaustion; this is not a
hard wall-clock or real-time guarantee. Canceled timer delivery is acknowledged
and checked against the current trusted deadline before rotating a fresh task.

Ordinary user faults log PID, vector, error, RIP, and CR2, then retire only that
process. Kernel faults, NMI, double fault, and machine check remain fatal. A
runnable peer resumes immediately after exit/fault/budget exhaustion. When all
slots finish, the saved bootstrap stack, page root, and FP state are restored.
Backing stays pinned through the batch; a later batch resets backing, queues,
CPU state, and grants. Unread completions are discarded and previous file
effects are not rolled back. Networking remains a separate kernel image.

## Verification and next work

Host tests cover ELF/BSS, queue wraparound/backpressure/hostile cursors,
copy-before-verify, boot-option validation, round-robin timing, retired slots,
and per-process budget accounting. QEMU retains the privilege suite and adds two
CPU-bound peers with independent data, stack, GPR, SIMD, MXCSR, segment, DF and
queue state. The probe waits for real timer quanta without yielding. Tests
require interleaved progress and multiple genuine peer switches. Additional
batches pair an infinite loop, a user fault, and a bad queue with a healthy
peer; a fresh healthy instance runs afterward. Native entry asserts the selected
kernel-stack bounds. CLI tests select scenarios from the same built image and
reject invalid boot options. Logs are in `target/userspace-test/integration/`.

Next steps are blocking/yield/wakeup semantics, safe resource reclamation and
general memory allocation, then dynamic loading and application networking.
There is no SMP, dynamic process creation, kernel preemption, or network/user
co-scheduling yet. See
[DR-009](<decisions/DR-009 Run isolated native programs through queue syscalls.md>)
and
[DR-010](<decisions/DR-010 Preempt two isolated processes at user return boundaries.md>).
