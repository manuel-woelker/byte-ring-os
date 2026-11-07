#!/usr/bin/env bash
set -euo pipefail
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "${repository_root}"
printf 'Booting Byte Ring OS; COM1 output follows. Press Ctrl+C to stop.\n'
exec ./scripts/qemu.sh \
  -machine pc -accel tcg -cpu qemu64 -m 64M -smp 1 \
  -display none -monitor none -serial stdio -no-reboot -no-shutdown \
  -kernel target/x86_64-unknown-none/release/byte-ring-kernel.bin
