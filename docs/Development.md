# Development

## Tooling

Use `./t` for development tools and `./n` for repository tasks. The wrappers,
Tool Tool configuration, Nao recipes, Markdown rules, and optional staged
Markdown hook originate in Pixui; GUI, browser, font, and renderer dependencies
are intentionally omitted.

Tool versions live in `.tool-tool/tool-tool.v2.kdl`. Keep Rust and kernel-std
versions aligned with the versioned sysroot path in `scripts/build-kernel.sh`.
The kernel build installs the target library into a repository-local sysroot;
it does not modify a global rustup installation. The host uses Pixui's Zig
linker wrapper and explicit Rust tool paths.

`./n check` checks formatting without changing files, checks and lints the
portable crate, runs nextest and documentation tests, builds the kernel, and
boots the QEMU integration image. `./n ci` runs the same checks. Use
`./n format` to apply formatting. Do not use host `--workspace --all-targets`
checks: the kernel binary requires its freestanding target and must not acquire
a host test harness.

## Running the kernel

Use `./n run` to build the normal kernel and boot QEMU with COM1 output streamed
live to the host terminal. Serial consoles are output-only: stdin is
disconnected so QEMU cannot be stopped by background terminal job control.
Press Ctrl+C to stop. The task has direct output
enabled so Nao does not buffer the console until QEMU exits. It uses system
QEMU when available and otherwise the same container fallback as the tests.
The shared launcher removes its container when the run stops.

Use `./n demo-errors` to watch divide errors, invalid opcodes, general
protection faults, page faults, double faults, and a Rust panic. Each case
builds its own fault-injection image, prints the report live, validates the
guest's result, and exits automatically before the next demonstration. Each
guest has a 20-second timeout. Normal kernel images are unaffected.

`./n demo-ping` demonstrates the static-IP network image with live serial
request/reply logs. `./n net-test` is part of `./n check`; `./n net-wrap-test`
separately tests 65,541 echo exchanges and real 16-bit virtqueue index
wraparound. The independent peer needs Python 3 and QEMU 7.2+ or the container
fallback. See [Networking](Networking.md) for artifacts and optional host TAP
setup. `./n run-net` requires a preconfigured interface and never changes host
routes.

## Verification strategy

Driver and HAL host tests run with `--features byte-ring-drivers/test-hal`.
The feature is forwarded into the HAL dependency; `cfg(test)` alone cannot
select a backend in another crate. The native backend refuses host builds, and
the test backend refuses bare-metal builds. Keep these configurations in
separate Cargo invocations. `./n check-hal-config` also verifies that both
invalid backend selections fail with the intended configuration diagnostic.

`./n qemu-test` builds separate Multiboot images for normal boot, VM rejection
checks, divide error, invalid opcode, general protection, page fault, double
fault, and Rust panic. Each boots with software CPU emulation and must exit with
status 33 after guest validation. Normal boot executes WRITE to stdout handle 0
and RETURN 123, checking the exact COM1 bytes including CRLF. VM rejection
checks verify that malformed programs and invalid file handles produce errors
without output. Fault cases check the hardware frame and printed diagnostic; the
double-fault case uses an invalid interrupted stack and must enter its emergency
IST stack. Timeouts, unexpected panics, guest resets, missing output, invalid
frames, and unexpected boot bytes all fail. Logs remain in
`target/qemu-test/integration/<case>/`. Use a system `qemu-system-x86_64` or a
working Docker-compatible CLI; the fallback container contains Debian Bookworm's
QEMU and SeaBIOS. Emulator versions follow the host or container distribution
rather than Tool Tool. CI installs QEMU explicitly.

Add host behavior tests as ring buffers, the protocol, and the VM are
implemented. Test invalid bytecode, stale capabilities, full queues, wraparound,
resource limits, and completion ordering. Maintain the emulator test alongside
boot changes; linking an ELF alone does not demonstrate that the kernel boots.

Keep unsafe blocks small and document their safety obligations. Shared-memory
and device invariants need explicit tests and review; the compiler cannot prove
correctness across an untrusted address-space boundary.

To build one diagnostic image for investigation:

```sh
./scripts/build-kernel.sh --qemu-test page-fault
```

Its image is at
`target/qemu-test/page-fault/x86_64-unknown-none/release/byte-ring-kernel.bin`.
Test-only fault injection and emulator exit code are absent from normal builds.
CPU exceptions are currently terminal; recovery and device interrupts remain
future work. See
[DR-002](<decisions/DR-002 Report fatal CPU exceptions through COM1.md>).

Automated fault/demo images permit guest shutdown so `isa-debug-exit` can
terminate QEMU; `-no-shutdown` is reserved for persistent interactive runs.

## Repository conventions

Follow [code style](Code%20style.md), [planning](plans/README.md), and
[DR-000](<decisions/DR-000 Record decisions in the repository.md>). Update
architecture and user-facing documentation alongside meaningful changes.
The optional pre-commit hook formats staged Markdown while preserving unrelated
working-tree edits. Enable it with `git config core.hooksPath .githooks`.
