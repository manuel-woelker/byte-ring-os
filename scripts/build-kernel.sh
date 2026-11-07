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
  test_case="${2:-boot}"
  case "${test_case}" in
    boot) features=qemu-test ;;
    vm-errors|divide|invalid-opcode|general-protection|page-fault|double-fault|panic)
      features="qemu-${test_case}" ;;
    *) printf 'Unknown QEMU test case: %s\n' "${test_case}" >&2; exit 2 ;;
  esac
  build_arguments+=(--features "${features}" --target-dir "target/qemu-test/${test_case}")
  image_directory="target/qemu-test/${test_case}/x86_64-unknown-none/release"
elif [[ "$#" != 0 ]]; then
  printf 'Usage: %s [--qemu-test [case]]\n' "$0" >&2
  exit 2
fi
./t cargo build -p byte-ring-kernel --bin byte-ring-kernel \
  --target x86_64-unknown-none --release "${build_arguments[@]}" \
  --config 'target.x86_64-unknown-none.rustflags=["--sysroot", ".cache/kernel-sysroot/1.99.0", "-C", "relocation-model=static", "-C", "link-arg=-Tcrates/kernel/linker.ld"]'

./t llvm-objcopy -O binary "${image_directory}/byte-ring-kernel" \
  "${image_directory}/byte-ring-kernel.bin"
