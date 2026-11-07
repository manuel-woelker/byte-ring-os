#!/usr/bin/env bash
set -euo pipefail
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "${repository_root}"
readonly sysroot="${repository_root}/.cache/kernel-sysroot/1.99.0"
if [[ ! -f "${sysroot}/lib/rustlib/x86_64-unknown-none/lib/libcore.rlib" ]] &&
   ! compgen -G "${sysroot}/lib/rustlib/x86_64-unknown-none/lib/libcore-*.rlib" > /dev/null; then
  ./t kernel-std --prefix="${sysroot}" --disable-ldconfig
fi
build_arguments=()
image_directory="target/x86_64-unknown-none/release"
if [[ "${1:-}" == "--qemu-test" ]]; then
  build_arguments+=(--features qemu-test --target-dir target/qemu-test)
  image_directory="target/qemu-test/x86_64-unknown-none/release"
elif [[ "$#" != 0 ]]; then
  printf 'Usage: %s [--qemu-test]\n' "$0" >&2
  exit 2
fi
./t cargo build -p byte-ring-kernel --bin byte-ring-kernel \
  --target x86_64-unknown-none --release "${build_arguments[@]}" \
  --config 'target.x86_64-unknown-none.rustflags=["--sysroot", ".cache/kernel-sysroot/1.99.0", "-C", "relocation-model=static", "-C", "link-arg=-Tcrates/kernel/linker.ld"]'

./t llvm-objcopy -O binary "${image_directory}/byte-ring-kernel" \
  "${image_directory}/byte-ring-kernel.bin"
