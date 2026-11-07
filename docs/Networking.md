# Networking

The network image provides a modern PCI virtio-net device with static
IPv4, ARP replies, and ICMP echo replies. A separate HTTP image adds TCP for
a kernel-hosted static page. Neither image has DHCP, DNS, IPv6, routing,
fragment reassembly, or an application-facing network API.

| Setting | Value |
| --- | --- |
| Guest | `192.0.2.2/24` |
| Host or synthetic peer | `192.0.2.1/24` |
| Guest MAC | `52:54:00:12:34:56` |
| NIC | `virtio-net-pci,disable-legacy=on` |
| MTU | 1500 bytes; maximum ICMP payload 1472 bytes |

## Demonstration and checks

```sh
./n demo-ping      # Live guest logs and four synthetic peer ping exchanges
./n net-test       # ARP, 97 echo replies, malformed traffic, and recovery
./n net-wrap-test  # 65,541 echo replies, crossing real 16-bit queue indices
./n check         # Host tests, native network lint, old and new QEMU checks
```

These tasks use QEMU's Unix-stream Ethernet backend and an independent Python 3
standard-library packet peer. They require QEMU 7.2 or newer, or the existing
Debian Bookworm Docker/Podman fallback. They do not need raw sockets, TAP,
privileged containers, host routes, or external network access.

Each guest starts with a line like:

```text
virtio-net ready ip=192.0.2.2/24 mac=52:54:00:12:34:56
ICMP echo src=192.0.2.1 dst=192.0.2.2 id=4660 seq=1 bytes=3 reply=queued drops=0 tx_full=0 suppressed=0
```

`reply=queued` means the driver submitted the reply, not that the peer received
it. The test peer independently verifies received addresses, checksums,
identifier, sequence, payload, and zero padding. It also checks continued
operation after malformed packets. Logging uses bounded bytecode WRITE to
stdout, with at most one echo log per polling batch. `suppressed` counts logs
omitted under load; `drops` counts ignored packets; `tx_full` counts replies
that could not be queued. Human-rate pings are logged individually.

Per-run artifacts are under `target/network/integration/<run>/`,
`target/network/demo/<run>/`, or `target/network/wrap/<run>/`: `serial.log`,
`result.txt`, and a bounded `packets.log` containing the first 512 transmitted
or received Ethernet frames as hex. Failed tests print their artifact location.
Tests stop QEMU and remove their Unix sockets and containers when finished.

The fallback retains a read-only workspace, `--network none`, and dropped
capabilities. A random per-run Unix socket directory is mounted read-only;
only that socket's permissions permit connection across container UID mappings.
The peer uses no host IP interface, despite exercising real guest Ethernet/IP.
The packet path was verified with rootless Podman via its Docker CLI
(Bookworm QEMU 7.2) and host QEMU 11.1.3. Standalone Docker was not available
for manual comparison.

## Ping from the host with TAP

This optional Linux workflow needs host QEMU, `/dev/net/tun`, `ip`, and
authority to create/configure one TAP interface. Check that `192.0.2.0/24` does
not conflict with an existing host route. On Arch/CachyOS, install host QEMU
and load the TUN/TAP driver first:

```sh
sudo pacman -S --needed qemu-system-x86
sudo modprobe tun
```

On Debian/Ubuntu, use `sudo apt-get install qemu-system-x86` instead. Confirm
that `command -v qemu-system-x86_64` finds the executable and `/dev/net/tun`
exists. A container may hide that device even when the host driver is loaded;
run this workflow in a host terminal. The existing restricted QEMU container
fallback supports packet-peer tests, not host TAP access.

`run-net` reports each missing prerequisite separately and prints the relevant
setup commands; it does not install packages or change device permissions.
Configure an isolated TAP explicitly:

```sh
sudo ip tuntap add dev byte-ring0 mode tap user "$(id -un)"
sudo ip address add 192.0.2.1/24 dev byte-ring0
sudo ip link set dev byte-ring0 up
./n run-net
```

Then, in another host terminal:

```sh
ping -c 4 192.0.2.2
```

The guest logs each accepted request and its reply outcome. The launch task
uses an output-only serial console with stdin disconnected, so QEMU cannot
be suspended by terminal job control while Nao owns the foreground terminal.
Stop QEMU with
Ctrl+C. Remove the interface explicitly when finished:

```sh
sudo ip link delete dev byte-ring0
```

`run-net` uses an existing `byte-ring0` interface with QEMU's host network
scripts disabled. It never changes host networking. For a different interface,
use `./scripts/run-network.sh <interface>` after configuring the same subnet.
TAP mode requires host QEMU and is not enabled inside the restricted fallback
container. Use `demo-ping` when those host facilities are unavailable.

QEMU user-mode NAT/host forwarding does not provide this inbound ICMP path.
TCP/UDP `hostfwd` options are not a replacement for TAP when using host `ping`.
Host TAP ping was verified with QEMU 11.1.3: four requests received four
replies with no packet loss. The unprivileged packet path remains in the normal
check suite so CI does not need a configured TAP.

## HTTP from the host

Use the same preconfigured TAP and host prerequisites described above. Stop
`run-net` before starting the HTTP image; only one guest may own the TAP and
static address at a time.

```sh
./n run-http
```

Wait for `HTTP ready http://192.0.2.2:8080/`, then use another host terminal:

```sh
curl --noproxy '*' http://192.0.2.2:8080/
curl --noproxy '*' -I http://192.0.2.2:8080/
```

A browser can open that address too. The HTML is embedded from
[assets/index.html](../assets/index.html); edit it and restart the task to
rebuild. `GET /` returns the page, `HEAD /` returns its headers, and unknown
paths return
404. HTTP/1.0 and HTTP/1.1 are supported; HTTP/1.1 requires Host. Responses have
Content-Length and Connection: close. Unsupported methods return 405, malformed
or body-bearing requests 400, oversized headers 431, incomplete headers 408,
and unsupported HTTP versions 505. The HTTP image also answers ICMP echo.

The server has one connection, a 1024-byte header limit, 2048-byte TCP RX and
1024-byte TCP TX buffers, a five-second request deadline, and a fifteen-second
connection cap. TCP handles segmented input, partial output, retransmission,
flow control, and FIN. Request bodies, persistent connections, TLS, HTTP/2,
files, and application sockets are unimplemented. A slow client temporarily
occupies the sole connection. This kernel service is a development endpoint,
with no isolation or production entropy; do not expose it to an untrusted link.

For a different TAP use `./scripts/run-network.sh --http <interface>`.
The task never changes host interfaces, routes, or firewalls. It uses the same
output-only console and Ctrl+C handling as `run-net`.

## HTTP integration test

```sh
./n http-test
```

This rootless test uses an independent standard-library Python TCP client over
QEMU's Unix-stream Ethernet backend. No host port, TAP, root privilege, browser,
raw socket, or external network is needed. It verifies ARP, ping, checksums, TCP
handshake, split/duplicate input, rejected corrupt TCP, the exact embedded page
across several segments, retransmission of dropped data, graceful FIN, HEAD,
HTTP errors, reconnect, stalled headers, and recovery after a client reset. The
same test is required by `./n check` alongside the original ping checks.

QMP inspects halted quiet idle and a one-shot APIC countdown that stays loaded
across 150 ms, with no PIC timer activity. The test then wakes the CPU with
ICMP during an incomplete HTTP request and verifies the remaining deadline is
rearmed rather than restarted. A 24-packet burst exercises receive-budget
boundaries. Existing retransmission and timeout tests verify timer delivery and
EOI across repeated use.

Artifacts remain in `target/http/integration/<run>/`: `serial.log`,
`packets.log`, and `result.txt`. The test stops its own QEMU and cleans its
socket on completion or failure. smoltcp is pinned to 0.14.0 without
std/alloc/default features; only Ethernet, IPv4, TCP, and automatic ICMP echo
replies are enabled. A PIT-calibrated single-core TSC supplies actual elapsed
milliseconds for TCP and HTTP deadlines. PIT channel 2 also calibrates the
local APIC; PIT channel 0 never arms a runtime deadline. See
[DR-006](<decisions/DR-006 Serve a bounded HTTP endpoint using smoltcp.md>).

## Troubleshooting host ping

If `ping` reports `Destination Host Unreachable`, check that `run-net` printed
both `Byte Ring OS: COM1 ready` and the `virtio-net ready` line. Starting the
QEMU process alone does not mean the guest has reached its network loop.

A QEMU process with `STAT=T` in `ps` is stopped. Older launchers inherited
Nao's terminal stdin while running in a background process group; QEMU's
terminal setup/read could stop the entire VM before boot. Stop the old task
and rerun `./n run-net` after updating. The current launcher disconnects stdin
while still streaming serial output and supporting Nao's Ctrl+C cancellation.

Use `ip neigh show dev byte-ring0` to inspect ARP resolution. A failed neighbor
entry and no guest-ready line suggest a stopped/unbooted guest; a ready guest
with no replies needs further packet inspection. Do not disable host firewalls
or change routes without identifying where the traffic is lost.

## Implementation boundaries

- `drivers::pci` owns PCI configuration transactions, sizes 32/64-bit BARs, and
  validates capability chains, region bounds, alignment, and overlap.
- The kernel maps validated register windows above the first GiB and below
  4 GiB using uncached, supervisor-only 2 MiB mappings. Other MMIO layouts fail
  explicitly. This bootstrap mapper is not a general memory manager.
- `drivers::virtio_net` negotiates VERSION_1 and MAC only, configures one RX/TX
  split queue pair, and suppresses notifications while foreground code polls.
  Before sleep it enables RX/TX notifications and rechecks used rings,
  preventing lost wakeups. It acknowledges ISR state in foreground code and
  bounds initialization/configuration retries.
- `drivers::virtqueue` owns 32 descriptors and 2048-byte buffers per queue,
  reduced if the device supports fewer entries. Static DMA memory is aligned,
  permanently pinned, and accessed through HAL memory operations without Rust
  references aliasing device writes. Release/acquire fences order queue
  publication/completion; publication is ordered before MMIO notification.
- The CPU cannot reuse a published buffer until the device returns ownership.
  Invalid used IDs, lengths, advances, or duplicate unowned completions poison
  the queue. A device error stops the kernel with diagnostics; static storage
  remains pinned even if reset or processing fails. Full TX queues drop/count
  ping-only replies; the HTTP adapter withholds transmit tokens until space is
  available. Neither path overwrites descriptors or waits indefinitely.
- The modern virtio header occupies 12 bytes without offloads. QEMU 7.2 leaves
  `num_buffers=0` with mergeable RX disabled; the driver accepts zero or one in
  this single-buffer mode, rejects other counts, and still validates flags,
  GSO type, and lengths. TX sets a zeroed header.
- `kernel_core::network` parses bytes without casting them to native structs.
  It accepts only local /24 unicast IPv4 sources and our destination address,
  IHL 5, positive TTL, complete unfragmented ICMP echo requests, and valid
  checksums. It ignores unsupported traffic rather than generating ICMP errors.
- `kernel::net` connects the driver, static configuration, bounded polling,
  counters, and bytecode stdout logging. Ordinary kernel builds remain separate
  and retain the original boot-and-halt behavior.

The current platform assumes one x86_64 QEMU PC CPU, coherent DMA, and no IOMMU
translation. Application SQ/CQ queues remain separate future work; applications
never receive raw virtqueue descriptors or DMA/MMIO handles. Both network images
now use interrupt-driven HLT during idle. The firmware PCI INTx route and legacy
PIC wake the foreground owner without running device code inside handlers. HTTP
arms a calibrated local xAPIC one-shot timer for TCP/request deadlines. Its
32-bit countdown replaces repeated PIT wakeups. The CPU must expose enabled
BSP xAPIC below 4 GiB; x2APIC and disabled/unsupported modes fail explicitly.
PIT channel 2 remains a boot calibration reference. A quiet listener and the
ping image have no periodic timer wakeups. See
[DR-008](<decisions/DR-008 Use the local APIC for deadline wakeups.md>).

See
[DR-005](<decisions/DR-005 Use polled virtio networking with static IPv4.md>)
and the
[completed plan](<plans/completed/2026-11-07 Virtio networking and static IPv4 ping.md>).
