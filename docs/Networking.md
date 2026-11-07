# Networking

The network image provides a polled modern PCI virtio-net device with static
IPv4, ARP replies, and ICMP echo replies. It has no TCP/UDP sockets, DHCP, DNS,
IPv6, routing, fragmentation, or application-facing network API yet.

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
  split queue pair, and keeps device interrupts suppressed. It acknowledges ISR
  state while polling and bounds initialization/configuration retries.
- `drivers::virtqueue` owns 32 descriptors and 2048-byte buffers per queue,
  reduced if the device supports fewer entries. Static DMA memory is aligned,
  permanently pinned, and accessed through HAL memory operations without Rust
  references aliasing device writes. Release/acquire fences order queue
  publication/completion; publication is ordered before MMIO notification.
- The CPU cannot reuse a published buffer until the device returns ownership.
  Invalid used IDs, lengths, advances, or duplicate unowned completions poison
  the queue. A device error stops the kernel with diagnostics; static storage
  remains pinned even if reset or processing fails. Full TX queues drop/count
  replies rather than overwrite descriptors or wait indefinitely.
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
never receive raw virtqueue descriptors or DMA/MMIO handles. Polling consumes
CPU while idle. Interrupt-driven sleep is a later improvement once wakeups and
scheduling have a defined contract.

See
[DR-005](<decisions/DR-005 Use polled virtio networking with static IPv4.md>)
and the
[completed plan](<plans/completed/2026-11-07 Virtio networking and static IPv4 ping.md>).
