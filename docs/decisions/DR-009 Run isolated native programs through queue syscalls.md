# DR-009: Run isolated native programs through queue syscalls

- Status: Accepted
- Date: 2026-11-07

## Decision

Implement one embedded freestanding x86_64 Rust ELF program at a time in a
separate ring-3 mapping. Accept static little-endian ELF64 ET_EXEC with bounded,
page-aligned PT_LOAD segments and no dynamic linking/TLS. Enforce user W^X,
zero BSS, absent guards, supervisor kernel mappings, NX, and CR0.WP. Keep all
backing storage static and reset it between instances. No general allocator
or scheduler is introduced.

Each instance receives a read-only bootstrap descriptor and a four-entry
SQ/CQ pair. Separate pages enforce producer/consumer rights. Kernel cursors
are private authority; all application-owned advances are checked. Every
accepted slot is copied in full to private memory before header/VM validation
and execution. Reserve CQ space before SQ consumption. All file effects use
the existing bytecode interpreter and per-instance stdout capability context.

Native syscall 0 progresses a bounded snapshot of the default queue pair.
Native syscall 1 exits the process; this lifecycle operation complements queue
bootstrap and does not introduce direct native file/network services. Unknown
calls and zero work limits consume nothing. Define fixed wire constants and
entry offsets in kernel-core, without exposing Rust layout.

SYSCALL switches to a trusted kernel stack before Rust, masks unsafe flags,
saves general and FP/SIMD state, and restores trusted kernel FP state. Return
uses a validated IRET frame with fixed user selectors and safe flags, preserving
user FP/SIMD and all non-clobbered registers. No SYSRET fast path yet. Initial
registers/FP state are cleared. No GS/TLS user state or FSGSBASE is enabled.

User CPU faults terminate the current program, preserving kernel fatal handling
for kernel faults, NMI, double fault, and machine check. A one-second local APIC
budget terminates native loops. Users run with IF enabled and IOPL zero;
foreground kernel work remains bounded with IF clear. Queue syscalls copy only
known pinned mappings and take no arbitrary user pointers. Program exit/fault
restores the boot kernel context and allows another fresh instance.

## Rationale

This proves the intended queue/bytecode boundary with real CPU isolation.
A static embedded ELF and fixed mappings make ownership and cleanup explicit
without requiring a filesystem or virtual-memory allocator. IRET avoids
SYSRET's privileged return failure hazards while the ABI is experimental.
Preserving SIMD state prevents native Rust kernel code from corrupting users.
A hardware execution budget prevents an uncooperative program from retaining
the only CPU forever.

## Context

The bytecode VM and stdout adapter exist, but SQ/CQ, process mappings, and CPU
syscalls are still designs in DR-003. The kernel has fatal exceptions and a
calibrated local APIC timer. Its original supervisor identity map alone cannot
protect user programs or contain their faults.

## Consequences

The initial image runs one program at a time, with fixed code/data/stack and
queue limits. This is an experimental ABI, not ELF/Linux/POSIX compatibility.
There is no preemptive scheduling or concurrent network service. Supervisor
kernel mappings remain permissive, though user mappings enforce W^X. General
user-copy, arbitrary mappings, TLS, dynamic linking, and SMP require further
contracts. Queue death discards pending work; file effects are not rolled back.

## Considered alternatives

- **Call native code at ring 0:** fails to prove privilege, queue authority,
  or user-fault containment and is rejected.
- **INT-based service entry:** easier initially, but would defer the accepted
  CPU SYSCALL convention rather than exercise its stack-switch requirements.
- **General ELF loader/process manager first:** unnecessary allocation and
  lifecycle machinery for one bounded embedded program.
- **Direct WRITE syscalls:** would bypass the chosen bytecode/SQ/CQ model.
- **SYSRET immediately:** faster but adds return-state hazards before entry,
  permissions, and fault containment have native test coverage.
