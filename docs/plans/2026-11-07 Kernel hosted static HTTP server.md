# Kernel hosted static HTTP server

## Goal and scope

Serve an HTML page embedded in the kernel at `http://192.0.2.2:8080/` over
the existing modern virtio-net device. Provide a host TAP task and an automated
rootless QEMU integration test. This is an experimental kernel service; it
does not establish an application socket or bytecode networking ABI.

Start with one connection, bounded storage, GET and HEAD, HTTP/1.0 and HTTP/1.1,
and connection close after every response. No filesystem, allocation, TLS,
HTTP/2, request bodies, persistent connections, DHCP, DNS, routing, or
concurrency.

## Foundation and design

The current kernel has polled virtio Ethernet, static IPv4, and custom ARP/ICMP
handling, but no TCP, clock, scheduler, allocator, or device interrupts.
The HAL isolates port and DMA operations and has an explicit host test backend.

- Pin smoltcp with only Ethernet, IPv4, TCP, and ICMP echo support. Use its
  bounded socket buffers and protocol state machines instead of writing TCP.
  Keep the existing ping-only image and its tests available.
- Adapt the safe virtio driver to smoltcp using private frame buffers. Expose
  TX capacity before issuing tokens, preserve queue ownership, and propagate
  device failures rather than silently dropping successful token writes.
- Calibrate the single-core x86 TSC against PIT channel 2 through HAL ports.
  Keep conversion logic portable and testable. Fail clearly if calibration
  fails; do not use polling iterations as elapsed time.
- Put bounded incremental HTTP parsing and response selection in kernel-core.
  Validate request framing, reject unsupported bodies, and send explicit
  errors for malformed, oversized, unsupported, or unknown requests.
- The kernel owns NIC, clock, interface, socket, and request state. Handle
  partial reads/writes, deadlines, disconnects, graceful FIN, and reuse after
  abort. Log readiness and bounded diagnostics through stdout WRITE.
- Document the service boundary, limits, and dependency choice in a decision
  record and architecture/networking documentation.

## Implementation checklist

- [x] Record the HTTP/TCP decision and implement portable HTTP parsing,
  response formatting, embedded page, and clock conversion with host tests.
- [x] Add the pinned TCP stack, safe virtio adapter, hardware clock, and
  separate HTTP kernel image with bounded connection lifecycle handling.
- [ ] Add `http-test` and `run-http` tasks, independent packet-level QEMU
  verification, and usage documentation.
- [ ] Run the complete repository checks, verify host HTTP where available,
  commit the implementation, and move this plan to `completed/`.

## Verification

- Host tests cover split requests, limits, malformed HTTP, route/method/version
  handling, framing ambiguity, body rejection, exact Content-Length, and
  portable clock conversion. Driver tests cover TX backpressure where feasible.
- An independent Python Ethernet/TCP peer drives QEMU without TAP or sudo.
  Verify handshake, split input, exact page and headers, segmentation, FIN,
  reconnect, errors, retransmission after loss, and stalled-client recovery.
- `./n check` includes HTTP integration plus all existing host, boot, exception,
  bytecode, and ping checks. Use the pinned tools through `./t`.
- If the existing host TAP is available and unused, verify the same service
  with `curl`. Do not create host routes/interfaces or stop unrelated guests.

## Assumptions, risks, and alternatives

Port 8080 and one active connection are deliberate initial limits. The kernel
continues polling with interrupts disabled and consumes idle CPU. TSC timing
assumes the supported single-core QEMU PC; other hardware requires a validated
clock source. Initial TCP sequence numbers are development seeds, not a
production entropy facility. The service has no security isolation from the
kernel and must remain a development endpoint on a controlled link.

Writing a custom TCP stack adds substantial reliability work without advancing
the bytecode architecture. Hosting an existing userspace server requires an
allocator, processes, isolation, and a socket ABI that do not exist yet.
smoltcp supplies protocol machinery while keeping ownership and memory bounds
explicit. No open question blocks this initial implementation.

## Results

Portable HTTP and clock conversion are implemented. See
[DR-006](<../decisions/DR-006 Serve a bounded HTTP endpoint using smoltcp.md>).
HEAD is supported alongside GET to permit `curl -I`. The HTTP image boots in
QEMU and reports readiness after PIT calibration. The pinned Rust wrapper now
supplies the host sysroot for dependency build scripts; native dependency flags
use absolute paths. Packet-level verification and host tasks are next.
