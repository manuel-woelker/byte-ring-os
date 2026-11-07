# DR-006: Serve a bounded HTTP endpoint using smoltcp

- Status: Accepted
- Date: 2026-11-07

## Decision

Add a separate kernel-hosted HTTP image using pinned smoltcp without its
allocator or standard library features. Enable Ethernet, IPv4, TCP, and ICMP
echo replies. Keep the existing ping-only image. The safe virtio driver adapts
to the stack through copied frames and explicit transmission capacity.

The server has one socket, fixed request and TCP buffers, static IPv4
`192.0.2.2/24`, and port 8080. GET and HEAD select an embedded HTML page at `/`.
Support HTTP/1.0 and HTTP/1.1, exact Content-Length, and connection close after
one response. Reject request bodies, ambiguous framing, and oversized headers.
Use monotonic deadlines and bounded polling; partial I/O is normal.

Calibrate the single-core x86 TSC against PIT channel 2 using HAL port access.
Keep time conversion and HTTP logic portable. No device interrupts or heap
are required. Kernel bootstrap code owns the counter and calibration lifetime.

## Rationale

TCP needs sequencing, checksums, acknowledgement, retransmission, flow control,
and shutdown state. Reusing smoltcp keeps that work independent of the
bytecode experiment while retaining explicit bounded memory. The HTTP layer
can remain small, directly testable, and independent of transport details.

One socket and connection close make resource ownership and recovery visible.
A real clock is necessary for TCP correctness; polling iterations cannot stand
in for time. A host-independent Ethernet/TCP test checks observable wire
behavior, and host TAP permits ordinary browsers and curl.

## Context

The kernel already boots modern virtio-net and answers ARP and ICMP with static
addresses. It has no allocator, scheduler, application isolation, general socket
ABI, or clock. The requested server is an experimental service inside the
kernel, serving a page from memory. The application SQ/CQ architecture remains
a separate milestone.

## Consequences

The HTTP image gains third-party protocol code and a Cargo lockfile dependency
set. Default features stay disabled; protocol offload and fragmentation remain
unsupported. Static bounds and deadlines limit individual clients but do not
provide denial-of-service resistance or kernel isolation. Single-core QEMU
is the supported timing platform; the TSC must be reconsidered for SMP or
other hardware. Development sequence-number seeds are not production entropy.

Polling consumes idle CPU. Only one client progresses at a time; a stalled
client temporarily occupies the sole socket. TLS, HTTP/2, persistent
connections, request bodies, routing, and application networking are out of
scope. HTTP runs natively in the kernel; ordinary diagnostics still use bytecode
WRITE to stdout. This does not add bytecode socket instructions or grant raw HAL
access.

## Considered alternatives

- **Write TCP locally:** rejected because reliable TCP is substantial work
  unrelated to proving the kernel interaction model.
- **Run an existing userspace HTTP server:** requires processes, isolation,
  allocation, and a socket ABI that do not exist yet.
- **Use polling iterations as a timer:** rejected because CPU load and QEMU
  execution speed would change retransmission and timeout behavior.
- **Enable interrupts and a timer scheduler first:** useful future work, but
  unnecessary for a correct polled single-core prototype with a calibrated
  clock.
- **Support multiple clients and persistent HTTP now:** rejected because it
  expands lifecycle and resource policy before the first endpoint is verified.
