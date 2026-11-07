# Byte Ring OS

An experimental Rust operating system exploring **ring buffers and a bytecode
VM as the interface for all kernel interactions**.

The goal is to make kernel interactions faster, safer, more reliable, and easier
to extend. Clients would submit bounded bytecode programs through ring buffers;
the kernel would validate and execute them against explicitly granted
capabilities, publishing completions through ring buffers as well.

The initial architecture gives each application a submission queue (SQ) and
completion queue (CQ). Applications fill the SQ with bytecode programs and use
CPU `syscall` to request a bounded synchronous batch. The kernel copies,
validates, and executes programs, then publishes correlated CQ results. See
[the kernel API design](docs/Kernel%20API.md) for the intended contract; this
application interface is not implemented yet.

These are research goals, not demonstrated guarantees. Batching may reduce
boundary crossings, but verification, scheduling, synchronization, and copying
can outweigh those savings. We will measure the tradeoffs.

## Current state

The Rust workspace boots a minimal x86_64 kernel in QEMU and prints
`Byte Ring OS: COM1 ready` through a polling 16550 UART driver. A HAL selected
at compile time supplies native port I/O or a test-owned scripted backend.
Host unit tests exercise the driver; a QEMU integration test exercises the
native HAL and checks exact serial output, now produced through bytecode
WRITE to stdout. A rejection case also checks that malformed programs and an
ungranted handle do not emit output. CPU faults print exception names,
register context, error codes, and page-fault address/access details before
halting. Double faults use an emergency stack.

Boot support is a minimal Multiboot v1 adapter with an initial identity map and
stack, plus fatal exception handling. There is no memory manager, device
interrupt handling, ring buffer, userspace, or CPU syscall entry yet. A small
bytecode VM now supports RETURN and generic WRITE, with stdout handle 0 bound
to COM1 in the boot context. See [Bytecode API](docs/Bytecode%20API.md).

## Development

The tooling is adapted from `../pixui`: `./t` bootstraps a checksum-verified
Tool Tool binary and resolves pinned tools; `./n` runs Nao tasks. A checkout is
self-contained and does not depend on the sibling repository.

The initial supported development host is x86_64 Linux with Bash, curl,
sha256sum, and standard archive tools. First use downloads tools and Rust
components into the ignored `.cache/` directory. Rust and the bare-metal
standard library are pinned together at 1.99.0.

```sh
./n check          # Formatting, host tests, kernel build, and QEMU test
./n format         # Apply Rust and Markdown formatting
./n demo-errors    # Show six real CPU fault/panic cases live, then exit
./n run            # Build and boot QEMU with live COM1 output; Ctrl+C stops it
./n build-kernel   # Build the kernel ELF and Multiboot .bin image
./n qemu-test      # Check boot output, CPU fault diagnostics, and panic output
./t cargo test -p byte-ring-drivers --features test-hal
```

The kernel artifacts are
`target/x86_64-unknown-none/release/byte-ring-kernel` and
`byte-ring-kernel.bin` in the same directory. The `.bin` image boots directly:

```sh
qemu-system-x86_64 -accel tcg -cpu qemu64 -m 64M -display none \
  -monitor none -serial stdio \
  -kernel target/x86_64-unknown-none/release/byte-ring-kernel.bin
```

`./n run` streams the normal kernel's COM1 output to the host terminal and
keeps QEMU running until Ctrl+C. It uses the same host/container launcher as
the integration test.

For `./n run` and `./n check`, install `qemu-system-x86_64` or provide a working
Docker/Podman Docker CLI. When host QEMU is absent, the test builds a Debian
Bookworm QEMU container from `tools/qemu/Dockerfile`. CPU emulation uses TCG and
requires no KVM access. Each test guest has a 20-second timeout; serial and
error logs remain in `target/qemu-test/integration/<case>/`. The test kernel has
a separate output directory and enables an emulator exit device; the normal
kernel simply halts.

Optionally enable Pixui's staged Markdown formatting hook:

```sh
git config core.hooksPath .githooks
```

## Project direction

- **Performance:** batch submissions, bound allocation and queue work, and
  compare latency and throughput against simpler interaction mechanisms.
- **Safety:** validate bytecode, capabilities, buffer ranges, and resource
  budgets before executing privileged operations.
- **Reliability:** define ownership, queue-full behavior, cancellation,
  completion ordering, and recovery rather than relying on implicit contracts.
- **Extensibility:** evolve versioned operations and VM programs without
  exposing kernel pointers or arbitrary native kernel code to clients.

Start with one bounded request/completion path and a small interpreter. Earn
concurrency, zero-copy transport, and richer VM features through evidence.

See [architecture](docs/Architecture.md), [development](docs/Development.md),
[code style](<docs/Code style.md>), [planning](docs/plans/README.md), and
[decision records](docs/decisions/).
