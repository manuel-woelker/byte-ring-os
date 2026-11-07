#!/usr/bin/env bash
# Show real CPU faults and a panic, validating each guest before continuing.
set -euo pipefail
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "${repository_root}"

for test_case in divide invalid-opcode general-protection page-fault double-fault panic; do
  printf '\nError handling demo: %s\n' "${test_case}"
  ./scripts/build-kernel.sh --qemu-test "${test_case}"
  status=0
  QEMU_TIMEOUT=20s ./scripts/qemu.sh \
    -machine pc -accel tcg -cpu qemu64 -m 64M -smp 1 \
    -display none -monitor none -serial stdio -no-reboot \
    -device isa-debug-exit,iobase=0xf4,iosize=0x01 \
    -kernel "target/qemu-test/${test_case}/x86_64-unknown-none/release/byte-ring-kernel.bin" \
    || status=$?
  if [[ "${status}" != 33 ]]; then
    printf 'Demo %s failed: expected guest success exit 33, got %s.\n' "${test_case}" "${status}" >&2
    exit 1
  fi
done
printf '\nAll six error handling demonstrations completed successfully.\n'
