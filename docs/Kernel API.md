# Kernel API

Status: the version-one bytecode interpreter and boot stdout adapter are
implemented. Portable queue processing and the fixed experimental queue codec
are implemented with native ring-3 programs and SYSCALL entry in the userspace
image. See [Userspace](Userspace.md) for fixed mappings and lifecycle.
See [Bytecode API](Bytecode%20API.md) for the executable program format.
[DR-003](<decisions/DR-003 Submit kernel bytecode through application queue pairs.md>)
records the chosen boundary. Details below describe the first implementation
target, not a stable public binary interface.

## Application boundary

Each application starts with one kernel-established SQ/CQ pair and a descriptor
containing the protocol version, mapping locations, capacities, maximum program
size, execution limits, and its initial capability handles. Queue creation is
part of application bootstrap; it does not require an already working queue.

The initial pair has one application producer and one application consumer.
Multiple application threads must serialize access. The kernel binds the pair
to the calling application's identity and independently serializes execution;
it never trusts a user-supplied pointer to identify another application's queue.

A normal interaction is:

1. Encode a bounded bytecode program into a free SQ slot.
2. Publish the SQ producer index with release ordering.
3. Execute CPU `syscall` to request processing of a bounded batch.
4. The kernel copies, validates, and executes each accepted program.
5. Read correlated results from the CQ using acquire ordering, then release
   those slots by advancing the application-owned CQ consumer index.

Publishing alone does not execute anything. There is no poller, asynchronous
worker, blocking wait, or completion notification in the initial version.

## Queue entries

One SQ slot holds one complete program. Use inline storage for now so a request
does not depend on a separately registered mutable program buffer.

| Submission field | Meaning |
| --- | --- |
| `request_id: u64` | Application-provided opaque correlation cookie |
| `bytecode_version: u16` | Instruction encoding version |
| `flags: u16` | Initially zero; reject unsupported bits |
| `program_length: u32` | Bytes used in the bounded inline program area |
| `program` | Encoded instructions and bounded inline constants |

| Completion field | Meaning |
| --- | --- |
| `submission_sequence: u64` | SQ position assigned by the kernel |
| `request_id: u64` | Cookie copied from the accepted submission |
| `status` | Success or explicit validation/execution/service failure |
| `result: u64` | Program's return value, or status-defined error detail |

Duplicate cookies are allowed; the submission sequence distinguishes requests.
Every consumed submission gets exactly one completion during normal operation,
even when its program is rejected. Completion order initially matches SQ order.
This is not a persistence guarantee across kernel crashes or application death.

Candidate statuses are `Success`, `InvalidProgram`, `Denied`, `LimitExceeded`,
`ServiceError`, and `VmTrap`. A successful syscall does not imply successful
programs: the CQ is the authoritative source of per-program results.

Use explicit fixed-width encoding, initially little-endian. Do not expose Rust
struct layout, native pointers, or `usize` in shared entries. Exact offsets,
alignment, status numbers, capacities, and opcodes remain to be specified before
implementing a public codec; bootstrap must advertise the supported limits.

## Ownership and publication

Both queues have bounded power-of-two capacities. Use monotonically advancing
64-bit sequence counters with wrapping arithmetic; mask a sequence to select a
slot. Capacity must be below half the counter range, and valid outstanding
entries must never exceed capacity.

| Region | Application access | Kernel access |
| --- | --- | --- |
| SQ entries and producer index | Read/write | Read |
| SQ consumer index | Read | Read/write |
| CQ entries and producer index | Read | Read/write |
| CQ consumer index | Read/write | Read |

Place regions with different access rights in separately protectable mappings.
Kernel-owned indices have private authoritative state; published copies are
for observation, not authority. Validate every user-owned index advance against
its last accepted value, available slots, and the current queue state. Invalid
movement is a queue protocol fault, not permission to walk arbitrary memory.

Producers write a complete entry before release-publishing their new producer
index. Consumers acquire that index before reading slots. Consumers release
slots through their consumer index; producers acquire it before reusing slots.
Counters must be properly aligned and accessed through the defined atomic ABI,
not ordinary Rust references to concurrently modified shared fields.

An SQ slot becomes reusable once the kernel has copied it and publishes the
SQ consumer advance. That does not mean the program has finished. Its result
becomes available only when the kernel publishes its CQ completion.

The application must not modify published SQ slots until released. Since an
untrusted application can violate this rule, the kernel copies a bounded slot
into private storage and validates and executes that same copy. Concurrent
mutation can yield a rejected or different submission; it must never invalidate
verification of the bytes actually executed.

## Syscall entry and progress

The initial syscall is conceptually `enter(max_submissions)`. It refers to the
current application's default pair. The kernel snapshots the published SQ tail
at entry, clamps positive requested work to its per-entry maximum, and processes
at most that snapshot. Submissions published later wait for another entry.

Implemented x86_64 register convention:

| Register | Meaning |
| --- | --- |
| `RAX` on entry | `0`: process the default queue pair |
| `RDI` on entry | Positive maximum number of submissions to process |
| `RAX` on return | Number of completions published by this entry |
| `RDX` on return | Stop reason |
| `RCX`, `R11` | Clobbered by the CPU syscall/return convention |

Preserve other general-purpose registers and application FP/SIMD state across
the entry. Stop reasons are `Idle` (snapshot exhausted), `BatchLimit` (work
limit reached), `CompletionFull`, `Busy`, `QueueFault`, or `BadArgument`. Assign
their wire values with the codec. Unknown operations and a zero batch limit
return `BadArgument` without consuming submissions. `Busy` consumes nothing.

For each submission, the kernel:

1. Checks CQ space and reserves one completion slot before consuming the SQ.
2. Copies bounded submission data and validates its header and program.
3. Advances its SQ consumer state after the copy, allowing slot reuse.
4. Rejects invalid programs without executing services, or interprets the
   verified private copy under resource and capability limits.
5. Writes the CQ entry and release-publishes its producer index.

A full CQ leaves the next SQ entry unconsumed. The application drains its CQ
and calls `enter` again. Never overwrite unread completions or accept work that
has no reserved result slot. Ordinary malformed bytecode gets a completion;
invalid queue control state disables the pair and returns `QueueFault`, without
promising completions for entries that were never accepted.

Entry assembly must switch from the user stack to a trusted kernel stack before
calling Rust, save the return context, and validate it before returning to
userspace. CPU `syscall` does not provide that stack switch automatically; see
the
[Intel instruction reference](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2b-manual.pdf).
The entry, VM, service dispatcher, and native drivers are separate layers.

## VM execution and authority

The initial bytecode instruction set is implemented and specified in
[Bytecode API](Bytecode%20API.md) and
[DR-004](<decisions/DR-004 Write through file handles with typed bytecode operands.md>).
RETURN takes an immediate result; WRITE takes an immediate file handle and
inline bytes interleaved with instructions. Memory-reference mode is reserved
and rejected. A VM WRITE is not the x86 CPU `syscall` instruction.

Every submission starts with fresh VM-local state. Verify instruction encoding,
operand types, branch targets, constant ranges, and supported operations before
service effects. Enforce a per-program instruction budget, memory bounds, and
bounded service work at runtime. The syscall batch limit multiplied by these
per-program limits bounds one entry's work; no operation may wait indefinitely.

Service calls resolve opaque application-local capability handles. Check type,
rights, lifetime, and arguments when executing each call. Programs cannot name
kernel addresses or acquire authority by naming an opcode. File handles are
application-local capabilities. Handle `0` is stdout, initially bound to COM1
for debug output through generic WRITE. Every file operation checks the
context's grants and write rights; the UART and HAL handles remain private to
the kernel implementation. Other handles are rejected by the current boot
context.

Verification failure has no service effects. A runtime trap, denied capability,
resource limit, or service failure stops the program and produces a failure
completion, but earlier service effects are not rolled back. There is no
transactional batch or automatic retry guarantee. VM traps are expected request
failures, not native CPU exceptions that halt the kernel.

## Lifecycle and implementation boundaries

Keep mappings and queue storage alive while an entry is executing. Initially
there is no cancellation or live resize, and all accepted programs finish before
the syscall returns. Application termination disables its pair and releases
resources after execution stops; completions are not promised to a dead owner.
Future asynchronous services need an explicit outstanding-request lifecycle.

Implement portable queue validation, bytecode decoding, verification, and VM
execution in testable `no_std` code. Keep the privileged entry, user mappings,
application context, and native service adapters in the kernel. Continue using
the compile-time HAL for device I/O.

The fixed userspace image implements protected mappings, per-instance queue
identity, trusted stacks, bounded known-page copies, CPU entry/return, and
user-fault containment. General process creation and arbitrary user-copy remain
future work. Kernel supervisor mappings still need broader W^X hardening.

Verify queue wraparound, full/empty behavior, invalid index advances, CQ
backpressure, correlation and ordering, concurrent mutation, invalid bytecode,
capability misuse, fuel exhaustion, partial effects, and repeated syscall entry.
Use QEMU to check actual ring-3 transitions and ensure malicious requests fail
without crashing the kernel or accessing another application's resources.

## Experimental userspace codec

[DR-009](<decisions/DR-009 Run isolated native programs through queue syscalls.md>)
defines the first isolated userspace milestone. The portable engine uses four
slots, 256 program bytes, four submissions per entry, 64 VM instructions, and
128 output bytes per program. SQ entries are 272 bytes: the fields above occupy
bytes 0–15, followed by 256 inline program bytes. CQ entries are 32 bytes:
sequence at 0, cookie at 8, status u32 at 16, zero reserved u32 at 20, result at
24. All scalars are little-endian. Completion statuses are numbered 0–5 in the
order Success, InvalidProgram, Denied, LimitExceeded, ServiceError, VmTrap.
Non-success result details are initially zero.

Each queue page has an aligned u64 counter at 0 and slots beginning at 64.
The SQ consumer and CQ consumer each occupy separate pages. Stop values are
Idle=0, BatchLimit=1, CompletionFull=2, Busy=3 (reserved), QueueFault=4, and
BadArgument=5. A disabled pair remains disabled until process teardown.
Native syscall 1 implements process exit with status in RDI; it is a
lifecycle operation, while all file effects continue through bytecode.

The bootstrap descriptor is a 128-byte immutable record identified by
`BRUSR001`, version u32=1 at 8, page size u32 at 12, SQ/SQ-head/CQ/CQ-consumer
addresses u64 at 16/24/32/40, capacity/program/batch u32 at 48/52/56, image base
u64 at 64, stack bounds u64 at 72/80, stdout handle u64 at 88, and VM
instruction and output bounds u32 at 96/100. Other bytes are zero. RDI names
this descriptor and RSI carries an opaque initial argument. Layout constants and
codecs live in `kernel_core::user_abi`; applications never select another
process's pair.
