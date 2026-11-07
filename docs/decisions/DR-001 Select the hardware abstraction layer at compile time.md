# DR-001: Select the hardware abstraction layer at compile time

- Status: Accepted
- Date: 2026-11-07

## Decision

Select the hardware abstraction layer (HAL) backend through centralized Rust
`cfg` declarations. Drivers use concrete handles and ordinary methods, without
trait objects, virtual dispatch, or backend conditionals scattered through
hardware-independent code. The production backend initially targets x86_64 port
I/O. An explicit `test-hal` feature selects a host test backend.

Keep hardware primitives in a HAL crate and device protocols in a driver crate.
The first driver is a polling 16550 COM1 debug transmitter. Register offsets,
UART initialization, polling policy, and formatting belong to the driver.
The HAL provides narrow byte port reads and writes. Add other primitives only
when a caller needs them.

Production handles contain their port address. Test handles also reference
state owned by one test, with scripted reads and an ordered operation log.
Both implement the same API; production methods remain visible for inlining.
There is no global mock state or runtime backend selector.

Constructing a hardware port range is unsafe: the caller must establish
privileged execution, device ownership, and the validity of accesses. Safe
methods operate only within that range. Handles are not cloneable; callers
must serialize access to shared devices. The test backend offers a safe
constructor backed by test-local state.

Use the explicit feature for dependency-based driver tests because `cfg(test)`
is not propagated into dependencies. Reject `test-hal` on bare-metal targets
and reject production privileged port I/O on host targets at compile time.
Keep feature-specific host checks separate from the production kernel build.

## Rationale

Compile-time selection leaves the compiler free to inline small operations and
remove abstraction overhead, while allowing identical driver code to run in
host unit tests. Concrete handles avoid unnecessary traits and dynamic
allocation in the hardware path. Optimization remains something to verify,
not an unconditional guarantee implied by `cfg`.

Test-local state supports parallel tests and explicit ownership. Scripted reads
and operation logs can check polling, device setup, and failure behavior without
trying to emulate a complete machine.

## Context

Byte Ring OS needs early debug output before interrupts, allocation, the VM,
and submission queues exist. The kernel must remain testable without executing
privileged CPU instructions in host tests. Its client-facing ring-buffer and
bytecode design does not remove the need for trusted native hardware drivers.

The repository initially contains only a halting entry scaffold. Exercising
COM1 under QEMU also requires a minimal boot path. Use a Multiboot v1 image
with an explicit physical layout, a 32-bit entry, an identity mapping, and a
transition to 64-bit Rust. This is an initial QEMU boot adapter, not a decision
to support all PCs or a complete memory-management design.

## Consequences

- Driver tests execute the same driver implementation against a selected HAL.
- A QEMU integration test must exercise the production backend and inspect
  actual COM1 output; passing mocks does not prove correct hardware behavior.
- Polling is bounded and reports timeout instead of hanging indefinitely.
  Early debug output has one owner and does not depend on a global logger lock.
- Boot and panic diagnostics can access the native driver directly. Future VM
  logging must go through capability-checked services rather than raw ports.
- Test and production backend APIs must remain aligned; both configurations
  need compilation checks. Cargo feature unification requires separate host and
  bare-metal commands.
- Additional architectures need their own primitive implementation and device
  discovery; the COM1 address is a PC convention, not a universal guarantee.
- The initial boot mapping is intentionally permissive and small in scope.
  Interrupt tables, memory protection, SMP, and production boot discovery remain
  future work.

## Considered alternatives

### Runtime trait objects

Rejected for these primitives because backend selection is fixed per binary.
Dynamic dispatch adds indirection and a runtime concept without providing a
needed capability.

### Traits and generics throughout drivers

Static dispatch could optimize equally well, but spreading backend type
parameters through every caller is unnecessary when one backend is selected
for the whole build. Revisit if one binary must use multiple hardware backends.

### Global mock port map selected by cfg(test)

Rejected because dependency crates do not inherit `cfg(test)`, and global
mutable test state creates ordering dependencies and parallel-test interference.

### Direct assembly in each driver

Rejected because it duplicates unsafe access contracts and makes host tests
exercise a different implementation from the real driver.

### A complete machine emulator in unit tests

Rejected because scripted I/O covers the first driver's observable contracts
with much less complexity. QEMU supplies the separate hardware integration test.
