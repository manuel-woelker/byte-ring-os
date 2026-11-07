#!/usr/bin/env bash
# Build a separate test image, then assert both real serial bytes and guest exit.
set -euo pipefail
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "${repository_root}"
./scripts/build-kernel.sh --qemu-test

readonly kernel="target/qemu-test/x86_64-unknown-none/release/byte-ring-kernel.bin"
readonly logs="${repository_root}/target/qemu-test/integration"
mkdir -p -- "${logs}"
printf 'Byte Ring OS: COM1 ready\r\n' > "${logs}/expected.serial"

arguments=(
  -machine pc -accel tcg -cpu qemu64 -m 64M -smp 1
  -display none -monitor none -serial stdio -no-reboot -no-shutdown
  -device isa-debug-exit,iobase=0xf4,iosize=0x01
  -kernel "${kernel}"
)

status=0
QEMU_TIMEOUT=20s ./scripts/qemu.sh "${arguments[@]}" \
  > "${logs}/actual.serial" 2> "${logs}/qemu.stderr" || status=$?
if [[ "${status}" != 33 ]]; then
  printf 'QEMU failed: expected guest success exit 33, got %s (124 means timeout).\n' "${status}" >&2
  cat -- "${logs}/qemu.stderr" "${logs}/actual.serial" >&2
  exit 1
fi
if ! cmp -s -- "${logs}/expected.serial" "${logs}/actual.serial"; then
  printf 'COM1 bytes differ from the expected boot message. Logs: %s\n' "${logs}" >&2
  diff -u -- "${logs}/expected.serial" "${logs}/actual.serial" >&2 || true
  exit 1
fi
printf 'QEMU integration passed: production HAL emitted the exact COM1 boot message.\n'
