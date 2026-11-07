#!/usr/bin/env bash
# An existing TAP is required; never modify the host network implicitly.
set -euo pipefail
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "${repository_root}"
mode=network
task=run-net
if [[ "${1:-}" == "--http" ]]; then
  mode=http
  task=run-http
  shift
fi
readonly tap="${1:-byte-ring0}"
if [[ ! "${tap}" =~ ^[a-zA-Z0-9_.-]{1,15}$ ]]; then
  printf 'Invalid TAP interface name.\n' >&2; exit 2
fi
missing=0
if ! command -v qemu-system-x86_64 > /dev/null; then
  printf 'Missing host executable: qemu-system-x86_64.\n' >&2
  if command -v pacman > /dev/null; then
    printf 'Install on Arch/CachyOS: sudo pacman -S --needed qemu-system-x86\n' >&2
  elif command -v apt-get > /dev/null; then
    printf 'Install on Debian/Ubuntu: sudo apt-get install qemu-system-x86\n' >&2
  else
    printf 'Install the QEMU x86 system emulator using your host package manager.\n' >&2
  fi
  missing=1
fi
if [[ ! -c /dev/net/tun ]]; then
  printf 'Missing host TUN/TAP device: /dev/net/tun.\n' >&2
  printf 'On the Linux host, load the driver: sudo modprobe tun\n' >&2
  printf 'Run TAP mode on the host; a container without TUN device access cannot use it.\n' >&2
  missing=1
elif [[ ! -r /dev/net/tun || ! -w /dev/net/tun ]]; then
  printf 'The current user needs read/write access to /dev/net/tun.\n' >&2
  missing=1
fi
if ! command -v ip > /dev/null; then
  printf 'Missing host executable: ip (install iproute2).\n' >&2
  missing=1
fi
if [[ "${missing}" != 0 ]]; then
  printf 'After resolving these prerequisites, configure the TAP using docs/Networking.md and rerun ./n %s.\n' "${task}" >&2
  exit 1
fi
if ! ip link show dev "${tap}" > /dev/null 2>&1; then
  printf 'Missing TAP interface %s. Configure it once on the host:\n' "${tap}" >&2
  printf '  sudo ip tuntap add dev %s mode tap user %q\n' "${tap}" "$(id -un)" >&2
  printf '  sudo ip address add 192.0.2.1/24 dev %s\n' "${tap}" >&2
  printf '  sudo ip link set dev %s up\n' "${tap}" >&2
  exit 1
fi
addresses="$(ip -4 -o address show dev "${tap}")"
if [[ "${addresses}" != *'192.0.2.1/24'* ]]; then
  printf 'TAP %s must have host address 192.0.2.1/24; no host settings were changed.\n' "${tap}" >&2
  exit 1
fi
./scripts/build-kernel.sh "--${mode}"
if [[ "${mode}" == http ]]; then
  printf 'Guest HTTP: http://192.0.2.2:8080/. From another terminal: curl http://192.0.2.2:8080/. Ctrl+C stops QEMU.\n'
else
  printf 'Guest: 192.0.2.2/24. From another terminal: ping 192.0.2.2. Ctrl+C stops QEMU.\n'
fi
exec ./scripts/qemu.sh -machine pc -accel tcg -cpu qemu64 -m 64M -smp 1 \
  -display none -monitor none -serial stdio -no-reboot -no-shutdown \
  -netdev "tap,id=net0,ifname=${tap},script=no,downscript=no" \
  -device virtio-net-pci,disable-legacy=on,netdev=net0,mac=52:54:00:12:34:56 \
  -kernel "target/${mode}/x86_64-unknown-none/release/byte-ring-kernel.bin"
