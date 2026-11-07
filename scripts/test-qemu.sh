#!/usr/bin/env bash
# Boot real CPU fault cases; require valid guest frames and serial diagnostics.
set -euo pipefail
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "${repository_root}"

for test_case in boot divide invalid-opcode general-protection page-fault double-fault panic; do
  ./scripts/build-kernel.sh --qemu-test "${test_case}"
  kernel="target/qemu-test/${test_case}/x86_64-unknown-none/release/byte-ring-kernel.bin"
  logs="target/qemu-test/integration/${test_case}"
  mkdir -p -- "${logs}"
  status=0
  QEMU_TIMEOUT=20s ./scripts/qemu.sh \
    -machine pc -accel tcg -cpu qemu64 -m 64M -smp 1 \
    -display none -monitor none -serial stdio -no-reboot -no-shutdown \
    -device isa-debug-exit,iobase=0xf4,iosize=0x01 -kernel "${kernel}" \
    > "${logs}/actual.serial" 2> "${logs}/qemu.stderr" || status=$?
  if [[ "${status}" != 33 ]]; then
    printf 'QEMU %s failed: expected exit 33, got %s (124 means timeout).\n' "${test_case}" "${status}" >&2
    cat -- "${logs}/qemu.stderr" "${logs}/actual.serial" >&2
    exit 1
  fi
  if [[ "${test_case}" == boot ]]; then
    printf 'Byte Ring OS: COM1 ready\r\n' > "${logs}/expected.serial"
    if ! cmp -s -- "${logs}/expected.serial" "${logs}/actual.serial"; then
      printf 'Unexpected COM1 boot output. Logs: %s\n' "${logs}" >&2
      exit 1
    fi
  else
    case "${test_case}" in
      divide) expected='EXCEPTION vector=0 name=Divide error' ;;
      invalid-opcode) expected='EXCEPTION vector=6 name=Invalid opcode' ;;
      general-protection) expected='EXCEPTION vector=13 name=General protection fault' ;;
      page-fault) expected='EXCEPTION vector=14 name=Page fault' ;;
      double-fault) expected='EXCEPTION vector=8 name=Double fault' ;;
      panic) expected='intentional QEMU panic' ;;
    esac
    output="$(<"${logs}/actual.serial")"
    if [[ "${output}" != *"${expected}"* ]]; then
      printf 'Missing %s diagnostic. Logs: %s\n' "${test_case}" "${logs}" >&2
      exit 1
    fi
    if [[ "${test_case}" == page-fault && "${output}" != *'CR2=0x0000000040000000 access=read privilege=supervisor reason=not-present'* ]]; then
      printf 'Missing page-fault address/access details. Logs: %s\n' "${logs}" >&2
      exit 1
    fi
  fi
  printf 'QEMU integration passed: %s.\n' "${test_case}"
done
