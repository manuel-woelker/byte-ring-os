#!/usr/bin/env bash
# An existing TAP is required; never modify the host network implicitly.
set -euo pipefail
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "${repository_root}"
readonly tap="${1:-byte-ring0}"
if [[ ! "${tap}" =~ ^[a-zA-Z0-9_.-]{1,15}$ ]]; then
  printf 'Invalid TAP interface name.\n' >&2; exit 2
fi
if ! command -v qemu-system-x86_64 > /dev/null || [[ ! -c /dev/net/tun ]]; then
  printf 'Host QEMU and /dev/net/tun are required for TAP. Use ./n demo-ping for the unprivileged socket demo.\n' >&2
  exit 1
fi
if ! command -v ip > /dev/null || ! ip link show dev "${tap}" > /dev/null 2>&1; then
  printf 'Preconfigure TAP %s with host 192.0.2.1/24; see docs/Networking.md.\n' "${tap}" >&2
  exit 1
fi
addresses="$(ip -4 -o address show dev "${tap}")"
if [[ "${addresses}" != *'192.0.2.1/24'* ]]; then
  printf 'TAP %s must have host address 192.0.2.1/24; no host settings were changed.\n' "${tap}" >&2
  exit 1
fi
./scripts/build-kernel.sh --network
printf 'Guest: 192.0.2.2/24. From another terminal: ping 192.0.2.2. Ctrl+C stops QEMU.\n'
exec ./scripts/qemu.sh -machine pc -accel tcg -cpu qemu64 -m 64M -smp 1 \
  -display none -monitor none -serial stdio -no-reboot -no-shutdown \
  -netdev "tap,id=net0,ifname=${tap},script=no,downscript=no" \
  -device virtio-net-pci,disable-legacy=on,netdev=net0,mac=52:54:00:12:34:56 \
  -kernel target/network/x86_64-unknown-none/release/byte-ring-kernel.bin
