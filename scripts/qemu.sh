#!/usr/bin/env bash
# Shared host/container launcher; callers supply the image and console options.
set -euo pipefail
# Nao owns the foreground terminal. These consoles are output-only; inheriting
# TTY stdin can stop background QEMU during terminal setup or reads.
exec < /dev/null
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "${repository_root}"

command=(qemu-system-x86_64)
if [[ -n "${QEMU_TIMEOUT:-}" ]]; then
  command=(timeout --kill-after=2s "${QEMU_TIMEOUT}" "${command[@]}")
fi
if command -v qemu-system-x86_64 > /dev/null; then
  exec "${command[@]}" "$@"
fi
if ! command -v docker > /dev/null; then
  printf 'Install qemu-system-x86_64 or a working Docker/Podman Docker CLI.\n' >&2
  exit 1
fi
readonly image="byte-ring-os-qemu:bookworm"
if ! docker image inspect "${image}" > /dev/null 2>&1; then
  docker build --tag "${image}" tools/qemu
fi
readonly container_name="byte-ring-os-qemu-${BASHPID}"
cleanup() {
  docker rm --force "${container_name}" > /dev/null 2>&1 || true
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

socket_mount=()
if [[ -n "${QEMU_SOCKET_DIRECTORY:-}" ]]; then
  socket_mount=(--volume "${QEMU_SOCKET_DIRECTORY}:${QEMU_SOCKET_DIRECTORY}:ro")
fi
docker run --rm "${socket_mount[@]}" --name "${container_name}" --network none --read-only --cap-drop ALL \
  --security-opt no-new-privileges --user "$(id -u):$(id -g)" \
  --volume "${repository_root}:/workspace:ro" --workdir /workspace \
  "${image}" "${command[@]}" "$@"
