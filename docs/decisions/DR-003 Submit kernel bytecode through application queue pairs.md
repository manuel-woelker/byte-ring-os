# DR-003: Submit kernel bytecode through application queue pairs

- Status: Accepted
- Date: 2026-11-07

## Decision

Give each user application one initial submission queue (SQ) and completion
queue (CQ), established when the kernel creates the application. Applications
publish bounded bytecode programs into their SQ. The kernel validates and
executes those programs, then publishes their results into the CQ.

Initially use the x86_64 CPU `syscall` instruction to enter the kernel and
process a bounded batch of published submissions synchronously. This is a
queue-processing entry point, not one syscall per device operation. Service
arguments and results travel through the queues. A bytecode service-call
instruction is a separate VM concept from the CPU entry instruction.

Use single-producer/single-consumer rings initially: the application produces
SQ entries and consumes CQ entries; the kernel consumes SQ entries and
produces CQ entries. The application serializes its threads before accessing
its pair. The kernel independently prevents concurrent execution of one pair.

Put one bounded inline bytecode program in each SQ entry and produce one CQ
entry per consumed submission, including validation and execution failures.
Reserve completion capacity before consuming a submission. Copy its bounded
contents into private kernel memory, then validate and execute that same copy.
Do not execute mutable shared code or accept raw kernel pointers.

VM service operations resolve application-local capabilities and check their
rights. Bytecode validation does not grant authority. There is no direct
client access to HAL handles, ports, or kernel memory.

The companion [kernel API design](../Kernel%20API.md) defines the initial queue
lifecycle, entry fields, ordering, execution, error, and proposed syscall
contracts. This is an experimental design, not an implemented or released ABI;
exact wire encoding and the bytecode instruction set require follow-up work.

## Rationale

Queue pairs give the application one consistent way to submit kernel work and
receive results. One CPU entry can process multiple programs and multiple
service operations, making batching possible without requiring a kernel poller
or scheduler in the first implementation.

Synchronous execution, inline programs, and one completion per submission keep
ownership and resource accounting explicit. Copying before validation avoids
executing bytes that the application changes after verification. Bounded work
and CQ reservation prevent unlimited kernel execution or dropped completions.

## Context

The project's central experiment is using ring buffers and a bytecode VM for
all application-facing kernel interactions. The existing kernel has boot,
COM1, and exception diagnostics, but no applications, queues, VM, address-space
isolation, or syscall entry/return path. This decision establishes the intended
boundary before implementing those components.

## Consequences

- The first API supports batching but is synchronous: a producer must execute
  `syscall` to request progress, and programs finish before that entry returns.
- CQ backpressure stops SQ consumption. Applications drain completions and enter
  again to process remaining work; the kernel never overwrites unread results.
- Shared indices and program bytes remain untrusted. Kernel-owned state and
  mappings enforce ownership, bounds, and publication contracts.
- Completion order is SQ order initially. Asynchronous services, cancellation,
  blocking waits, live queue resizing, and multiple pairs are deferred.
- Partial service effects are possible before a runtime failure. Programs are
  not transactions, and retries are not automatically safe.
- Real userspace requires isolated mappings, application identity, safe syscall
  entry/return, and user-fault containment before this model is a security
  boundary.
- Boot, CPU exceptions, and native drivers remain privileged infrastructure;
  establishing the queues is necessarily outside their ordinary submission path.

## Considered alternatives

### One ordinary syscall per kernel service

Rejected as the application-facing model because it bypasses the queue/VM
boundary and prevents programs from batching dependent service operations.
Keep it as a possible performance comparison rather than a parallel API.

### Kernel polling or background workers immediately

Deferred because they require scheduling and notification contracts before
there is an end-to-end queue path to measure. They can later drive the same SQ
and CQ protocol without changing programs into direct syscalls.

### Execute bytecode directly from shared memory

Rejected because verification would not ensure the executed bytes remain the
verified bytes. A private bounded copy is simpler than read-only ownership
transfer or page revocation for the initial version.

### Multi-producer queues and asynchronous completions immediately

Deferred because ordering, arbitration, cancellation, and outstanding resource
lifetimes would obscure the first execution and safety contracts.
