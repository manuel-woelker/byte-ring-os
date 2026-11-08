#!/usr/bin/env bash
set -euo pipefail
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "${repository_root}"
scenario="${BYTE_RING_USER_SCENARIO:-0}"
if [[ ! "${scenario}" =~ ^([0-9]|1[0-6])$ ]]; then
  printf 'Invalid user scenario. Use ./n run-user --list-scenarios.\n' >&2
  exit 2
fi
./scripts/build-kernel.sh --userspace
exec ./scripts/qemu.sh -machine pc -accel tcg -cpu qemu64 -m 64M -smp 1 \
  -display none -monitor none -serial stdio -no-reboot -no-shutdown \
  -kernel target/userspace/x86_64-unknown-none/release/byte-ring-kernel.bin \
  -append "user-scenario=${scenario}"
