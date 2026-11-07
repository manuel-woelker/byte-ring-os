# DR-004: Write through file handles with typed bytecode operands

- Status: Accepted
- Date: 2026-11-07

## Decision

Implement version-one bytecode with RETURN and a generic WRITE instruction.
WRITE takes an immediate file handle and an inline byte payload. Handle `0`
is stdout in each caller's file context. Bind the initial boot context's
stdout to COM1; do not put UART-specific behavior into the VM opcode.

Use explicit operand mode tags: immediate unsigned 64-bit scalar, inline bytes,
and a reserved memory-reference mode. Accept only the modes supported by the
operand's type. Implement immediate and inline modes now. Reject memory
references until buffer ownership and safe user-copy semantics exist.

Interleave inline payloads with their instructions. Verify checked lengths,
operand types, whole-program termination, and resource budgets before any file
operation. Decode and execute the same immutable program bytes. A program must
end in RETURN without trailing bytes. Its return operand becomes its result.

Keep the interpreter and encoder in portable `no_std` code. Use a statically
dispatched FileWriter service boundary to resolve handles and enforce write
rights in the caller's context. That boundary can support other writable files
without changing the opcode. Require complete writes for initial programs;
short writes and service failures stop execution without retry or rollback.

The concrete encoding, API, and errors are documented in
[Bytecode API](../Bytecode%20API.md). Use the VM to produce the normal kernel
boot message, and test real stdout output and rejected requests in QEMU.
This implements the bytecode/service layer of DR-003; SQ/CQ storage and CPU
syscall entry are not implemented by this decision.

## Rationale

A file-oriented WRITE keeps applications independent of the debug transport.
The handle is a context-local grant rather than a global device identifier;
redirecting stdout later need not change bytecode. Immediate scalars and inline
bytes cover the first useful programs with no register file or referenced
memory lifetime to manage.

Whole-program validation prevents malformed trailing instructions from causing
partial service effects. Inline bytes simplify construction of short messages
and avoid external program buffers. Their cache behavior is an experiment, not
a demonstrated performance advantage.

## Context

The queue architecture is documented, but no VM exists yet. The first proposed
instruction was RETURN with an immediate value, followed by debug output. The
user selected a generic file-handle WRITE with stdout at handle 0. Operand modes
were discussed to support immediates, inline data, and later buffer references.

## Consequences

- Generic VM tests can write to arbitrary granted handle numbers; the initial
  boot adapter accepts only handle 0 and returns BadHandle otherwise.
- File services must enforce bounded work and rights. Verifying bytecode does
  not validate an application's authority over a file handle.
- Version-one WRITE requires a complete accepted payload. Short-write handling
  and exposing counts to subsequent instructions require future VM extensions.
- An output prefix may remain visible after runtime failure. Programs are not
  transactions, and neither programs nor writes are automatically retried.
- Memory references are reserved rather than approximated using raw pointers.
- There are no application mappings, filesystem namespace, open/close calls,
  shared queues, or syscall execution yet. Early fatal diagnostics remain a
  direct COM1 path so they do not depend on a functioning VM.

## Considered alternatives

### A dedicated DEBUG_WRITE opcode

Rejected because it couples application bytecode to a debug-only service.
A generic WRITE and a context-bound stdout provide the same initial output
while supporting future redirection and other writable files.

### Untagged operands or every mode at every position

Rejected because explicit typed modes make decoding and validation clear.
Not every instruction needs every addressing mode; supporting combinations
without a real use case adds unnecessary validation and lifetime rules.

### Implement raw memory addresses immediately

Rejected because user-buffer validation, mutation, ownership, and lifetimes
have not been established. Reserved mode rejection gives a clear failure until
capability-authorized buffer references have a complete contract.

### Execute instructions while validating them

Rejected because a malformed final instruction could produce earlier output.
A complete validation pass over a stable copy avoids those structural effects.
