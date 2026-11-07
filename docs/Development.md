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
live to the host terminal. Press Ctrl+C to stop. The task has direct output
enabled so Nao does not buffer the console until QEMU exits. It uses system
QEMU when available and otherwise the same container fallback as the tests.
The shared launcher removes its container when the run stops.

## Verification strategy

Driver and HAL host tests run with `--features byte-ring-drivers/test-hal`.
The feature is forwarded into the HAL dependency; `cfg(test)` alone cannot
select a backend in another crate. The native backend refuses host builds, and
the test backend refuses bare-metal builds. Keep these configurations in
separate Cargo invocations. `./n check-hal-config` also verifies that both
invalid backend selections fail with the intended configuration diagnostic.

`./n qemu-test` builds a separate Multiboot test image, boots with software CPU
emulation, checks the exact COM1 bytes including CRLF, and requires guest exit
status 33. Timeout, panic, guest reset, missing output, and unexpected bytes all
fail. Logs remain in `target/qemu-test/integration/`. Use a system
`qemu-system-x86_64` or a working Docker-compatible CLI; the fallback container
contains Debian Bookworm's QEMU and SeaBIOS. Emulator versions follow the host
or container distribution rather than Tool Tool. CI installs QEMU explicitly.

Add host behavior tests as ring buffers, the protocol, and the VM are
implemented. Test invalid bytecode, stale capabilities, full queues, wraparound,
resource limits, and completion ordering. Maintain the emulator test alongside
boot changes; linking an ELF alone does not demonstrate that the kernel boots.

Keep unsafe blocks small and document their safety obligations. Shared-memory
and device invariants need explicit tests and review; the compiler cannot prove
correctness across an untrusted address-space boundary.

## Repository conventions

Follow [code style](Code%20style.md), [planning](plans/README.md), and
[DR-000](<decisions/DR-000 Record decisions in the repository.md>). Update
architecture and user-facing documentation alongside meaningful changes.
The optional pre-commit hook formats staged Markdown while preserving unrelated
working-tree edits. Enable it with `git config core.hooksPath .githooks`.
