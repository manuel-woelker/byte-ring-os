#!/usr/bin/env bash
set -euo pipefail
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "${repository_root}"
./scripts/build-kernel.sh --userspace-test
logs=target/userspace-test/integration
mkdir -p "${logs}"
status=0
QEMU_TIMEOUT=20s ./scripts/qemu.sh \
  -machine pc -accel tcg -cpu qemu64 -m 64M -smp 1 \
  -display none -monitor none -serial stdio -no-reboot \
  -device isa-debug-exit,iobase=0xf4,iosize=0x01 \
  -kernel target/userspace-test/x86_64-unknown-none/release/byte-ring-kernel.bin \
  > "${logs}/actual.serial" 2> "${logs}/qemu.stderr" || status=$?
if [[ "${status}" != 33 ]]; then
  printf 'Userspace QEMU failed: exit=%s, expected 33. Logs: %s\n' "${status}" "${logs}" >&2
  cat "${logs}/qemu.stderr" "${logs}/actual.serial" >&2
  exit 1
fi
python3 - "${logs}/actual.serial" <<'PY'
from pathlib import Path
import sys
output=Path(sys.argv[1]).read_bytes()
assert output.count(b'hello from ring 3 via SQ/CQ') == 2, output
assert b'userspace: isolation and recovery passed\r\n' in output, output
assert b'PANIC' not in output and b'EXCEPTION' not in output, output
for case in range(17): assert ('userspace case=%d result=' % case).encode() in output
print('Task outcome: ring-3 queues, privilege isolation, watchdog, and recovery passed')
PY
