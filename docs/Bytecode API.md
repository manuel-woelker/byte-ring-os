# Bytecode API

Version 1 is implemented as a portable, allocation-free interpreter in
`byte-ring-kernel-core::bytecode`. It has two instructions: `RETURN` and
`WRITE`. The kernel's normal boot message already uses this path. Submission
queues, completion queues, userspace, and CPU syscall entry remain design work;
this document specifies the executable program format independently of them.

[DR-004](<decisions/DR-004 Write through file handles with typed bytecode operands.md>)
records the instruction and file-service decisions. See
[Kernel API](Kernel%20API.md) for the intended application queue boundary.

## Example

```text
WRITE  immediate.u64(0), inline.bytes("hello\n")
RETURN immediate.u64(123)
```

Handle `0` is stdout in the caller's file context. The current boot context
binds it to COM1; the VM itself does not know about UARTs or port addresses.
A future application context can bind stdout to a different writable file.
The program returns `123` after writing six raw bytes.

## Encoding

All integer fields are little-endian, with no alignment padding. Instructions
and inline data share one sequential byte stream. The submission header carries
bytecode version `1`; there is no duplicate version header inside the program.
The current `vm::execute` API accepts version-one program bytes only. A future
submission decoder must reject unsupported versions before invoking it.

| Opcode | Instruction | Operands |
| --- | --- | --- |
| `0x01` | `RETURN` | One immediate unsigned 64-bit scalar |
| `0x02` | `WRITE` | Immediate unsigned 64-bit file handle, then inline bytes |

Each operand begins with a mode tag. Modes have types and are not accepted
interchangeably at every operand position.

| Tag | Mode | Encoding after tag | Current support |
| --- | --- | --- | --- |
| `0x01` | Immediate scalar | `u64` value | RETURN value and WRITE handle |
| `0x02` | Inline bytes | `u32` byte length, followed by exactly that many bytes | WRITE data |
| `0x03` | Memory reference | Reserved; no accepted encoding yet | Rejected explicitly |

Thus RETURN occupies 10 bytes. WRITE occupies 15 bytes plus its payload.
For example, `WRITE immediate(0), inline("hi")` encodes as:

```text
02                         WRITE
01 00 00 00 00 00 00 00 00 immediate handle 0
02 02 00 00 00             inline mode and byte length 2
68 69                      payload: "hi"
```

Payload bytes may contain zeros, invalid UTF-8, or bytes equal to instruction
opcodes. The decoder advances by the checked length; it never interprets payload
bytes as instructions. There are no jumps, registers, or separately addressed
constant areas in version 1. Exactly one terminal RETURN is required, and any
trailing bytes, including a second RETURN, invalidate the entire program.

## File writes

WRITE calls the kernel's file service with `(handle, bytes)`. The service
resolves the handle in the caller's context and checks ownership and write
rights for every call. Numeric handles are authority only through that context;
a client cannot access another application's file by guessing its number.

The interpreter does not special-case stdout or restrict writes to handle 0.
Its `FileWriter` service boundary is statically dispatched and can implement
other files. The initial kernel boot adapter grants only stdout and rejects
every other handle with `BadHandle`. It forwards stdout to the existing bounded
UART driver through the selected HAL.

WRITE uses raw bytes: no newline conversion, UTF-8 requirement, implicit zero
terminator, or added prefix. A successful WRITE must accept its complete payload
before execution continues. RETURN supplies the program result independently of
the write's byte count.

The service returns the accepted byte count or `BadHandle`, `Denied`, or `Io`.
A short write stops the VM with `ShortWrite { written, expected }`. A count
larger than the requested payload is an adapter contract violation, reported as
`InvalidWriteCount`. No failed or short write is retried automatically, and
prior output is not rolled back. Even an empty WRITE checks its handle.

## Verification and execution limits

The interpreter first verifies the entire immutable program, then decodes and
executes that same byte slice. Malformed later instructions cannot cause an
earlier WRITE to run. In the eventual application path, the caller must copy a
bounded submission into private kernel memory before creating this slice;
a Rust borrow alone does not make application-writable shared memory immutable.

Verification checks opcodes, operand modes, lengths, termination, and resource
limits. Defaults are 4096 program bytes, 1024 instructions including RETURN,
and 1024 total WRITE payload bytes per program. These are experimental
kernel-selected bounds, not parameters an application may raise unchecked.
A boundary value is permitted; exceeding it rejects the whole program before
any service call. Since the instruction set is straight-line, verified
instruction count is also its maximum execution count. Services must separately
bound their own work; the UART retains its per-byte polling limit.

The internal Rust API returns `Result<u64, vm::Error>`:

| Result/error | Meaning |
| --- | --- |
| `Ok(value)` | RETURN executed successfully |
| `InvalidProgram` | Truncation, unknown opcode, wrong/reserved mode, missing RETURN, or trailing bytes |
| `LimitExceeded` | Program bytes, instructions, or total output exceeds a trusted limit |
| `File(error)` | File-service handle, rights, or device error |
| `ShortWrite` | Only a prefix of the payload was accepted |
| `InvalidWriteCount` | A file adapter returned an impossible count |

Structural and limit failures have no file effects. File-service failures occur
at execution time and may follow earlier successful writes. The future queue
adapter will map these errors into the CQ statuses; that adapter is not yet
implemented. Errors are ordinary results rather than kernel panics.

## Encoding programs in Rust

```rust
use byte_ring_kernel_core::bytecode::builder::ProgramBuilder;
use byte_ring_kernel_core::file::STDOUT;

let mut storage = [0u8; 128];
let mut program = ProgramBuilder::new(&mut storage);
program.write(STDOUT, b"hello\n")?;
program.return_imm(123)?;
let encoded = program.finish()?;
```

This snippet assumes the caller propagates `EncodeError`. The builder owns no
allocation, checks storage capacity before changing encoded bytes, and refuses
appends after RETURN. Failed appends leave the previous encoding intact. Its
`finish` method requires a terminal RETURN; VM resource limits are checked by
the verifier separately.

## Memory references and next steps

The memory-reference tag reserves room for a future capability-authorized
buffer range, not an arbitrary pointer. Buffer identity, rights, lifetime,
range checking, mutation, and safe copy/snapshot semantics must be established
before defining or accepting its encoding. Version 1 rejects the mode even if
its following bytes would otherwise look well formed.

Next, connect this VM to bounded SQ/CQ storage and specify the submission and
completion codecs. Real ring-3 syscall entry remains dependent on application
isolation and user-fault containment. Do not treat the boot demonstration as an
implemented application ABI.
