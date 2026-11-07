# Virtio networking and static IPv4 ping

Status: implementing. Decisions are recorded in
[DR-005](<../decisions/DR-005 Use polled virtio networking with static IPv4.md>).

## Goal and scope

Add a modern PCI virtio-net driver to the x86_64 QEMU kernel. Give the guest a
static IPv4 address, log valid ICMP echo requests, and send correct echo
replies. Provide an automated, unprivileged QEMU test and an optional host TAP
workflow for using the host's normal `ping` command.

Initial configuration:

| Setting | Value |
| --- | --- |
| Guest IPv4 | `192.0.2.2/24` |
| Host/test peer IPv4 | `192.0.2.1/24` |
| Guest MAC | `52:54:00:12:34:56`, configured in QEMU and read from the NIC |
| Device | `virtio-net-pci,disable-legacy=on` |
| Driver | Modern virtio PCI transport, split RX/TX queues, polling |
| Network | Isolated local Ethernet segment; no gateway required |
| Protocols | Ethernet II, ARP for IPv4, IPv4, ICMP echo request/reply |

No TCP, UDP, DHCP, DNS, IPv6, routing, IP fragmentation/reassembly, VLANs,
multiqueue, offloads, packed rings, device interrupts, or general allocator in
this first scope. This is native kernel networking infrastructure; application
network handles, bytecode network operations, SQ/CQ, and ring-3 syscalls remain
separate work. Do not conflate the NIC's virtqueues with application queues.

## Current foundation and constraints

- The kernel boots one QEMU CPU with a first-GiB identity map, a fixed stack,
  exception handling, and bounded COM1 output. It currently halts after boot.
- The compile-time HAL supports byte port I/O and independent scripted test
  state. PCI configuration needs wider accesses; modern virtio needs MMIO.
- There is no PCI discovery, MMIO mapper, DMA allocator, timer service,
  interrupt controller, scheduler, or network stack.
- Kernel-core holds portable parsers/VM code; drivers are separate and currently
  forbid unsafe code. Put necessary memory/device unsafety behind HAL handles
  and explicit kernel boot/mapping boundaries rather than weakening that rule.
- Stdout handle 0 is bound to COM1; normal diagnostic logging can use the
  existing generic bytecode WRITE path. Fatal CPU diagnostics remain direct.
- `scripts/qemu.sh` supports host QEMU and a Debian Bookworm container fallback.
  That container has a read-only workspace, no network, and dropped
  capabilities. Ordinary tests must preserve those restrictions.
- The current mapping does not cover arbitrary PCI BARs. Discovery alone does
  not make a BAR dereferenceable, and device registers need appropriate memory
  attributes rather than ordinary cacheable RAM mappings.

## Proposed design

### HAL, PCI, and mapping

Extend the existing centrally selected HAL with aligned 16/32-bit port accesses
and bounded volatile MMIO accesses of the required widths. Preserve concrete
handles and test-owned state; log access widths, addresses, and values in tests.
Treat PCI configuration address/data transactions as one exclusively owned
operation, not independently interleavable writes and reads.

Scan the bounded PCI configuration space available on the QEMU PC bus. Discover
the modern virtio network device by IDs and capabilities, not fixed slot or BAR
addresses. Check multifunction devices, absent functions, BAR types/sizes,
capability-list bounds/cycles, region offsets, and notify offset multiplication.
Handle 32/64-bit memory BAR encoding. Explicitly reject devices or address
ranges outside the initial mapper's supported bounds rather than truncating
addresses.

Add only enough statically backed page-table mapping support to map the
validated MMIO regions. Preserve kernel RAM mappings; use supervisor-only,
appropriately uncached device mappings and check region overlap/alignment.
Enable PCI memory decoding and bus mastering only after resource validation.
Record mapping and DMA assumptions in a decision record before implementation.

### DMA and virtqueues

Reserve aligned, statically allocated queue storage and packet buffers in
kernel-owned memory below the supported DMA limit. No heap is required.
Represent device-visible addresses explicitly; the identity mapping initially
makes their physical addresses derivable, but not from arbitrary pointers.

Start with 32 descriptors per RX and TX queue when supported, with a negotiated
power-of-two queue size and 2048-byte packet buffers. Each packet uses a simple
bounded buffer containing its virtio header and Ethernet frame. Derive the
header format from the modern transport and negotiated features; do not assume
the legacy header layout. No scatter/gather or indirect descriptor support is
needed for this milestone.

Define descriptor ownership transitions and free-list accounting explicitly.
Populate RX buffers before DRIVER_OK, publish descriptors before available
indices, and acquire device completion state before reading received bytes.
Document actual DMA/compiler ordering requirements; volatile access alone is
not an ordering protocol. Avoid live Rust references that alias memory being
mutated by the device. CPU access and buffer reuse must follow ownership.

Validate used IDs, lengths, duplicate completions, ring wraparound, descriptor
exhaustion, and header sizes. Never recycle a buffer before device completion.
On corruption or reset failure, stop processing and retain device-owned storage
until DMA is stopped; do not free or reuse it prematurely.

### Device initialization and polling

Implement reset, ACKNOWLEDGE, DRIVER, feature negotiation, FEATURES_OK check,
queue configuration, and DRIVER_OK in the required order. Require the modern
VERSION_1 and MAC features; optionally negotiate link status. Do not negotiate
checksum/GSO/TSO, mergeable RX buffers, packed rings, indirect descriptors, or
other features not implemented. Fail clearly if required features or queues are
unavailable. Bound reset and initialization polling.

Use one RX and one TX queue. Keep device interrupts disabled/suppressed and
acknowledge transport status where required. The network kernel runs an explicit
bounded-work polling loop instead of the existing halt loop. Reclaim TX buffers
and replenish RX independently of packet logging. A full TX ring causes a
counted dropped reply rather than an unbounded wait or descriptor overwrite.

### Ethernet, ARP, IPv4, and ICMP

Keep parsing, checksums, and reply construction in portable, allocation-free
code with caller-provided buffers and explicit error/drop results. Use checked
byte decoding rather than casting packets to Rust header structs.

- Accept relevant unicast frames and ARP broadcasts; ignore other destinations
  and unsupported EtherTypes. Bound frames to the initial Ethernet MTU 1500.
- Validate ARP Ethernet/IPv4 types, hardware/protocol lengths, operation, and
  available bytes. Answer requests for the guest IP with its configured MAC.
  An outbound ARP cache is unnecessary for replying directly on this segment.
- Validate IPv4 version, total length, header checksum, destination address,
  and protocol. Initially accept only IHL 5 and unfragmented packets; reject
  options and fragmentation explicitly. Ignore Ethernet padding beyond the
  declared IP length. Reject expired TTL and invalid source addressing.
- Accept ICMP echo request type 8/code 0 with a valid checksum and complete
  header. Build type 0/code 0 replies, preserving identifier, sequence, and
  payload; swap IP/MAC endpoints and recompute both checksums.
- Reject broadcast/multicast echo destinations, malformed/truncated packets,
  and oversize payloads without panicking or reading outside received bytes.
- Zero output padding so short Ethernet frames cannot expose old buffer data.
  No Ethernet FCS is supplied by the guest driver.

### Static configuration and logging

Keep the initial IP/netmask and limits in one named kernel configuration module;
read the MAC from virtio device configuration and print both at startup.
Do not depend on DHCP or add a gateway for this isolated subnet.

At normal ping rates, log each accepted echo request with source/destination,
identifier, sequence, payload length, and reply outcome. Send bounded log text
through the existing WRITE-to-stdout path. Log successful queue submission as
such; do not claim remote delivery before observing it. Record TX-full and
malformed-packet counters. Suppress/count excess logs under a fixed per-poll
logging budget so serial output cannot starve RX/TX progress under a flood.

### QEMU workflows and topology

Add a separate network-enabled image so existing boot/fault/VM tests continue to
run without a NIC. Use `-nodefaults` or explicit NIC options where appropriate
to avoid an accidental second/default network device.

The automated `./n net-test` harness should use QEMU's stream backend over a
per-run Unix socket. A host-side Python standard-library peer listens first;
QEMU connects and exchanges length-prefixed Ethernet frames. Implement bounded
framing with partial-read handling. This does not require TAP, raw sockets,
external connectivity, root, or a full host networking stack.

For the container fallback, share only the per-run socket directory and arrange
narrow socket permissions/UID mapping so the container can connect while the
workspace stays read-only and `--network none` remains in force. Test both
Docker and rootless Podman behavior where available. QEMU stream networking
requires 7.2 or newer, matching the current Bookworm fallback. Verify backend
availability and fail with an actionable message rather than silently changing
topology.

Add `./n demo-ping` to run this packet peer with live serial logs and a short
series of echo requests/results, exiting automatically. Add `./n run-net` for
interactive host ping against a preconfigured TAP interface using
`script=no,downscript=no`. Document explicit host setup, interface ownership,
`192.0.2.1/24`, and `ping 192.0.2.2`; do not create interfaces, routes, bridges,
or firewall rules implicitly. Preserve a no-root default test path.

QEMU's user-mode NAT backend does not provide an ordinary inbound host-to-guest
ICMP ping path; TCP/UDP host forwarding is not an ICMP solution. TAP is optional
manual verification, not a CI dependency. Initially require host QEMU for the
TAP workflow rather than broadening container privileges.

## Implementation checklist

- [x] Record the modern virtio transport, static DMA, mapping, polling, and
      topology decisions in `docs/decisions/`; link the record here.
- [x] Extend HAL access widths/MMIO and test backends; verify bounds, alignment,
      access ordering, and independent test state.
- [ ] Add PCI discovery/capability validation and bounded MMIO mappings without
      changing unrelated boot behavior.
- [ ] Add static DMA ownership primitives and split virtqueues with tests for
      completion, exhaustion, invalid used entries, wraparound, and buffer
      reuse.
- [ ] Implement modern virtio-net negotiation, RX/TX, MAC reading, and bounded
      initialization/error handling.
- [x] Implement portable Ethernet/ARP/IPv4/ICMP parsing, checksums, and bounded
      echo replies with negative-input tests.
- [ ] Integrate a network-enabled boot image, static IP configuration, polling
      loop, counters, and bounded stdout logging.
- [ ] Add independent packet-peer test tooling and compatible host/container
      Unix-socket launch/cleanup support.
- [ ] Add `net-test`, `demo-ping`, and `run-net` tasks; keep optional TAP setup
      documented and explicit. Add rootless `net-test` to `./n check` and CI.
- [ ] Update README, architecture, development instructions, and failure/log
      expectations. Keep application networking APIs explicitly unimplemented.
- [ ] Run required host checks, native driver checks, and all existing/new QEMU
      scenarios. Run `./n check` and commit each completed unit of work.
- [ ] Complete demo and documented TAP verification where host permissions and
      facilities are available; report any unavailable manual checks explicitly.
      Move this plan to `completed/` only after required verification is done.

## Verification and acceptance criteria

Host tests cover HAL/PCI invalid accesses, malformed capability chains, MMIO
range calculations, feature refusal, queue ownership, wraparound, exhaustion,
invalid device lengths, and reuse after completion. Packet tests cover good and
bad checksums, odd-length payloads, truncation at every header boundary, invalid
length fields, wrong addresses/protocols, fragmentation/options rejection, and
zeroed output padding. Malformed traffic must return errors/drops, not panic.

The independent QEMU packet peer must:

1. Observe a serial ready line identifying virtio-net, the configured MAC, and
   static `192.0.2.2/24` before testing packets.
2. Send an ARP request and validate the returned addresses and operation.
3. Send multiple valid echo requests with known IDs, sequences, and varied
   payloads; independently verify reply endpoints, lengths, IP/ICMP checksums,
   and unchanged ICMP identifier, sequence, and payload.
4. Verify corresponding serial request/reply logs at normal rates. Confirm the
   packet reply on the peer, not only a guest log claiming success.
5. Inject malformed, wrong-destination, and unsupported packets; assert no reply
   within a bounded interval, then send a good request to prove continued life.
6. Send more packets than the queue capacity, and enough sequential cycles to
   exercise 16-bit ring-index wraparound; verify buffer reuse and continued
   progress. Keep the long wraparound run separate if needed for CI duration.
7. Detect guest panic/reset, missing replies, unexpected frames, and timeout as
   failures. Preserve serial/packet diagnostics and clean up QEMU, sockets,
   peers, and containers on success, failure, or Ctrl+C.

Acceptance: `./n demo-ping` visibly logs and answers requests at the static IP;
`./n net-test` passes without network-administration privileges; `./n check`
retains all boot, VM, HAL, and fault regressions. When TAP is available, the
host's `ping -c 4 192.0.2.2` succeeds with matching guest logs. Performance
claims require later native/KVM measurements; TCG tests establish correctness.

## Assumptions, risks, and open questions

- Assumption: modern PCI virtio-net is preferred over the legacy I/O-port
  transport. Supporting both now would add unnecessary compatibility work.
- Assumption: the proposed isolated addresses and MAC are acceptable. Detect
  subnet/interface conflicts in the manual workflow; do not change host routes
  to resolve them automatically.
- Assumption: polling on one CPU is acceptable initially. It consumes CPU while
  idle; interrupt-driven sleep needs a later wakeup and scheduler design.
- MMIO mapping and DMA ownership are the largest correctness risks. Document
  that this initial QEMU PC path assumes coherent DMA and no IOMMU translation;
  do not imply general physical-hardware support.
- Need to verify exact BAR placement/size and feature support on both the host
  QEMU and the Bookworm container. Reject unsupported resources explicitly.
- Decide whether to keep a small custom ARP/IPv4/ICMP responder or use an
  existing `no_std` stack during implementation review. Default: a small bounded
  responder for this narrow scope, with no general socket API or stack
  framework.
- Python 3 is proposed for the independent test peer; add/document that test
  dependency explicitly. Do not share the guest parser as the test oracle.
- Full host TAP access may be unavailable in the development sandbox. The
  rootless packet test remains mandatory; unavailable manual TAP checks must be
  reported rather than described as passed.

## References

- [Virtio specification: PCI transport, split queues, and network device](https://docs.oasis-open.org/virtio/virtio/v1.3/virtio-v1.3.html)
- [QEMU networking: TAP, user mode, and socket backends](https://www.qemu.org/docs/master/system/devices/net.html)
- [QEMU stream backend, available since 7.2](https://www.qemu.org/docs/master/interop/qemu-qmp-ref.html#object-QMP-netdevstreamoptions)
- [ARP: RFC 826](https://www.rfc-editor.org/rfc/rfc826)
- [ICMP echo: RFC 792](https://www.rfc-editor.org/rfc/rfc792)
