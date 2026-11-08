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
assert output.count(b'hello from ring 3 via SQ/CQ') == 3, output
assert b'userspace: isolation and recovery passed\r\n' in output, output
assert b'PANIC' not in output and b'EXCEPTION' not in output, output
for case in range(19): assert ('userspace case=%d result=' % case).encode() in output
for name in ['compute', 'loop-peer', 'fault-peer', 'queue-peer']:
    assert f'scheduler batch={name} begin'.encode() in output
    assert f'scheduler batch={name} switches='.encode() in output
compute=output.split(b'scheduler batch=compute begin',1)[1].split(b'scheduler batch=compute switches=',1)[0]
for name in ['A','B']:
    for count in range(1,4): assert f'task {name} quantum {count}'.encode() in compute
assert compute.index(b'task B quantum 1') < compute.index(b'task A quantum 3'), compute
print('Task outcome: ring-3 queues, resumable preemption, isolated peers, and recovery passed')
PY

# Reuse the same built image: scenario selection must happen at runtime.
for scenario in 5 6 99; do
  status=0
  QEMU_TIMEOUT=20s ./scripts/qemu.sh \
    -machine pc -accel tcg -cpu qemu64 -m 64M -smp 1 \
    -display none -monitor none -serial stdio -no-reboot \
    -device isa-debug-exit,iobase=0xf4,iosize=0x01 \
    -kernel target/userspace-test/x86_64-unknown-none/release/byte-ring-kernel.bin \
    -append "user-scenario=${scenario}" \
    > "${logs}/scenario-${scenario}.serial" 2> "${logs}/scenario-${scenario}.stderr" || status=$?
  expected=33
  if [[ "${scenario}" == 99 ]]; then expected=35; fi
  if [[ "${status}" != "${expected}" ]]; then
    cat "${logs}/scenario-${scenario}.serial" "${logs}/scenario-${scenario}.stderr" >&2
    exit 1
  fi
  python3 - "${logs}/scenario-${scenario}.serial" "${scenario}" <<'PY'
from pathlib import Path
import sys
output=Path(sys.argv[1]).read_text()
scenario=int(sys.argv[2])
if scenario == 99:
    assert 'invalid user-scenario option' in output and 'hello from ring 3' not in output, output
else:
    assert output.count('userspace case=') == 1, output
    assert f'userspace case={scenario} result=' in output, output
    assert ('Timeout' if scenario == 6 else 'Fault(13, 0)') in output, output
PY
done
printf 'Task outcome: ring-3 isolation, recovery, and runtime CLI selection passed\n'
