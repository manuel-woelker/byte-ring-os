#!/usr/bin/env bash
# A wrong HAL must fail deliberately, not merely because of an unrelated error.
set -euo pipefail
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "${repository_root}"
readonly logs="target/hal-configuration"
mkdir -p -- "${logs}"

expect_rejected() {
  local label="$1" diagnostic="$2" output
  shift 2
  if "$@" > "${logs}/${label}.log" 2>&1; then
    printf 'Incorrect HAL configuration unexpectedly compiled: %s\n' "${label}" >&2
    exit 1
  fi
  output="$(<"${logs}/${label}.log")"
  if [[ "${output}" != *"${diagnostic}"* ]]; then
    printf 'HAL configuration failed for an unexpected reason: %s\n' "${label}" >&2
    printf '%s\n' "${output}" >&2
    exit 1
  fi
}

expect_rejected native-on-host 'host builds must enable test-hal' \
  ./t cargo check -p byte-ring-hal
expect_rejected test-on-bare-metal 'test-hal must never be enabled in a bare-metal kernel' \
  ./t cargo check -p byte-ring-hal --features test-hal \
  --target x86_64-unknown-none \
  --config 'target.x86_64-unknown-none.rustflags=["--sysroot", ".cache/kernel-sysroot/1.99.0"]'
printf 'HAL configuration guards passed.\n'
