# Userspace

The `userspace` image executes one embedded freestanding Rust ELF program at a
time at ring 3. It exercises the native SYSCALL boundary and the bytecode SQ/CQ
model. This is an experimental process ABI, not Linux/POSIX compatibility.

```sh
./n run-user        # Build and run the embedded Rust queue demo
./n userspace-test  # Native privilege, queue, FP/register, and recovery checks
```

The demo prints `Byte Ring OS: hello from ring 3 via SQ/CQ`, validates the CQ,
checks rejected handles/programs, then exits. The kernel reports `Exited(0)`
and halts. Ctrl+C stops QEMU. Source is in `crates/user-demo`; its ELF is built
separately under `target/user/` before embedding in the userspace kernel image.
`./n check` includes its native lint and integration tests.

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
64 KiB trusted kernel entry stack, masks dangerous flags, saves all general
registers and FP/SIMD state, and restores trusted kernel FP state before Rust.
The kernel validates the return instruction against executable file bytes and
the stack against its fixed mapping. Return uses IRET with fixed user selectors
and safe flags. RAX/RDX carry count/stop reason, RCX/R11 are clobbered, other
general registers and FP/SIMD state are preserved. Syscall return flags reset
to `0x202`; returning interrupts preserve arithmetic/DF state while excluding
privileged and unsupported flag controls. No SYSRET fast path is present.

## Ownership and lifetime

Only one process executes at a time. Private kernel cursors bind it to its
fixed pair. User scalar indices use aligned acquire/release atomic accesses;
slot bytes use raw bounded copies. The kernel never creates a Rust slice of
application-mutated storage. It copies a full slot before releasing SQ space,
validates and executes that private copy, and release-publishes one completion.
A full CQ leaves the next SQ slot unconsumed. Invalid index movement disables
the pair for the lifetime of the instance.

Kernel Rust runs with IF clear and bounded per-entry work. User code runs with
IF enabled, so an APIC execution budget terminates a native infinite loop after
approximately one second. This is a process budget, not scheduling or a promise
of an exact wall-clock deadline; bounded kernel work can defer delivery. A
pending timer from a previous instance is acknowledged and checked against the
current trusted deadline before terminating anything.

Ordinary user CPU faults log vector, error, RIP, and CR2, then terminate only
the process. Kernel faults, NMI, double fault, and machine check retain fatal
diagnostics. Exit/fault/timeout stops the timer and restores the saved boot
stack, page root, and FP context. A new instance resets all user backing and
queue authority. Unread completions are discarded; previous file effects are
not rolled back. There is no concurrent network service in this image.

## Verification and next work

Host tests cover ELF validation/BSS, queue wraparound, full CQ, hostile cursor
movement, batch snapshots, mutation after copy, malformed bytecode, and denied
handles. QEMU runs 17 scenarios plus a final healthy instance. It verifies
CPL3, native syscalls, fresh BSS, general/SIMD/MXCSR preservation, entry flags,
protected code/queue/descriptor/APIC pages, stack guard/NX, I/O denial, invalid
and noncanonical return stacks, queue disable/backpressure, and watchdog
recovery. Serial artifacts are in `target/userspace-test/integration/`.

Next steps are an explicit process table and scheduler, resumable timer
preemption, general memory allocation/user-copy contracts, then file-backed
loading and application networking. This milestone establishes CPU isolation
and queue ownership before those concurrent lifetimes are introduced. See
[DR-009](<decisions/DR-009 Run isolated native programs through queue syscalls.md>).
