#!/usr/bin/env python3
"""Independent Ethernet/IPv4/TCP client for the real kernel HTTP image.

Only Python's standard library is used. The peer checks wire checksums and
sequence numbers rather than sharing the guest's protocol implementation.
"""
from dataclasses import dataclass
import os
import signal
import socket
import struct
import subprocess
import tempfile
import threading
import time

from network_peer import ROOT, GUEST_MAC, PEER_MAC, GUEST_IP, PEER_IP, checksum, arp, echo, receive, send, Qmp

SYN, ACK, FIN, RST, PSH = 2, 16, 1, 4, 8
MASK = 0xffffffff


def tcp(port, sequence, acknowledgement, flags, data=b"", window=4096):
    options = b"\x02\x04\x02\x00" if flags & SYN else b""  # MSS 512
    segment = struct.pack("!HHIIBBHHH", port, 8080, sequence, acknowledgement,
                          (20 + len(options)) // 4 << 4, flags, window, 0, 0) + options + data
    pseudo = PEER_IP + GUEST_IP + struct.pack("!BBH", 0, 6, len(segment))
    segment = segment[:16] + struct.pack("!H", checksum(pseudo + segment)) + segment[18:]
    ip = struct.pack("!BBHHHBBH4s4s", 0x45, 0, 20 + len(segment), 0,
                     0x4000, 64, 6, 0, PEER_IP, GUEST_IP)
    ip = ip[:10] + struct.pack("!H", checksum(ip)) + ip[12:]
    return GUEST_MAC + PEER_MAC + b"\x08\x00" + ip + segment


@dataclass
class Packet:
    port: int
    sequence: int
    acknowledgement: int
    flags: int
    data: bytes


class Peer:
    def __init__(self, connection, trace):
        self.connection = connection
        self.trace = trace
        self.next_port = 40000

    def send(self, frame):
        self.trace.write("TX " + frame.hex() + "\n")
        self.trace.flush()
        send(self.connection, frame)

    def receive(self, timeout=4):
        self.connection.settimeout(timeout)
        frame = receive(self.connection)
        self.trace.write("RX " + frame.hex() + "\n")
        self.trace.flush()
        return frame

    def packet(self, port, deadline):
        while time.monotonic() < deadline:
            frame = self.receive(max(.01, deadline - time.monotonic()))
            assert frame[:6] in [PEER_MAC, b"\xff" * 6]
            assert frame[6:12] == GUEST_MAC
            if frame[12:14] == b"\x08\x06":
                if frame[20:22] == b"\0\x01" and frame[38:42] == PEER_IP:
                    self.send(GUEST_MAC + PEER_MAC + b"\x08\x06" +
                              struct.pack("!HHBBH", 1, 0x800, 6, 4, 2) +
                              PEER_MAC + PEER_IP + GUEST_MAC + GUEST_IP)
                continue
            assert frame[12:14] == b"\x08\x00"
            ip = frame[14:]
            header = (ip[0] & 15) * 4
            total, = struct.unpack("!H", ip[2:4])
            assert ip[0] >> 4 == 4 and header >= 20 and total <= len(ip)
            assert checksum(ip[:header]) == 0
            assert ip[12:20] == GUEST_IP + PEER_IP
            assert ip[9] == 6, "unexpected IP protocol during TCP exchange"
            segment = ip[header:total]
            pseudo = GUEST_IP + PEER_IP + struct.pack("!BBH", 0, 6, len(segment))
            assert checksum(pseudo + segment) == 0, "invalid TCP checksum"
            source, destination, seq, ack, offset, flags, _, _, _ = struct.unpack("!HHIIBBHHH", segment[:20])
            assert source == 8080
            if destination != port:
                continue  # Late ACK/reset from a previous connection.
            size = (offset >> 4) * 4
            assert 20 <= size <= len(segment)
            return Packet(destination, seq, ack, flags, segment[size:])
        raise TimeoutError("TCP peer deadline expired")


class Client:
    def __init__(self, peer):
        self.peer = peer
        self.port = peer.next_port
        peer.next_port += 1
        self.sequence = 100000 + self.port
        self.acknowledgement = 0
        peer.send(tcp(self.port, self.sequence, 0, SYN))
        packet = peer.packet(self.port, time.monotonic() + 4)
        assert packet.flags & (SYN | ACK | RST) == SYN | ACK
        assert packet.acknowledgement == self.sequence + 1
        self.sequence += 1
        self.acknowledgement = (packet.sequence + 1) & MASK
        self.ack()

    def ack(self):
        self.peer.send(tcp(self.port, self.sequence, self.acknowledgement, ACK))

    def write(self, data, wait=False, duplicate=False, corrupt=False):
        frame = tcp(self.port, self.sequence, self.acknowledgement, ACK | PSH, data)
        if corrupt:
            bad = bytearray(frame)
            bad[-1] ^= 1
            self.peer.send(bytes(bad))
        self.peer.send(frame)
        self.sequence = (self.sequence + len(data)) & MASK
        if wait:
            packet = self.peer.packet(self.port, time.monotonic() + 4)
            assert packet.flags & ACK and not packet.data
            assert packet.acknowledgement == self.sequence
        if duplicate:
            self.peer.send(frame)
            packet = self.peer.packet(self.port, time.monotonic() + 4)
            assert packet.acknowledgement == self.sequence and not packet.data

    def response(self, lose_first=False, timeout=9):
        response = bytearray()
        segments = 0
        lost = None
        retransmitted = False
        closed = False
        deadline = time.monotonic() + timeout
        while True:
            packet = self.peer.packet(self.port, deadline)
            assert not packet.flags & RST, "server reset before completing HTTP"
            assert not packet.flags & SYN
            if packet.data:
                if lose_first and lost is None:
                    lost = (packet.sequence, packet.data)
                    continue
                if packet.sequence == self.acknowledgement:
                    if lost is not None and packet.sequence == lost[0]:
                        overlap = min(len(packet.data), len(lost[1]))
                        assert packet.data[:overlap] == lost[1][:overlap], "retransmission changed bytes"
                        retransmitted = True
                    response.extend(packet.data)
                    assert len(response) <= 16_384, "unbounded HTTP response"
                    self.acknowledgement = (self.acknowledgement + len(packet.data)) & MASK
                    segments += 1
                self.ack()
            if packet.flags & FIN and (packet.sequence + len(packet.data)) & MASK == self.acknowledgement:
                self.acknowledgement = (self.acknowledgement + 1) & MASK
                self.peer.send(tcp(self.port, self.sequence, self.acknowledgement, FIN | ACK))
                self.sequence = (self.sequence + 1) & MASK
                closed = True
                continue
            if closed and packet.flags & ACK and packet.acknowledgement == self.sequence:
                break
        if lose_first:
            assert retransmitted, "dropped data was not retransmitted"
        return bytes(response), segments

    def reset(self):
        self.peer.send(tcp(self.port, self.sequence, self.acknowledgement, RST | ACK))


def validate(response, status=200, body=None, head=False):
    header, actual = response.split(b"\r\n\r\n", 1)
    lines = header.split(b"\r\n")
    assert lines[0].startswith(b"HTTP/1.1 " + str(status).encode() + b" "), lines[0]
    fields = {}
    for line in lines[1:]:
        key, value = line.split(b":", 1)
        key = key.lower()
        assert key not in fields, "duplicate response header"
        fields[key] = value.strip()
    assert fields[b"connection"] == b"close"
    assert b"content-type" in fields
    if head:
        assert actual == b"" and int(fields[b"content-length"]) == len(body)
    else:
        assert int(fields[b"content-length"]) == len(actual)
        if body is not None:
            assert actual == body, "HTTP body differs from embedded page"
    return fields


def exercise(peer, qmp):
    qmp.assert_idle()
    print("Idle: CPU halted before network traffic; no periodic idle tick")
    peer.send(arp())
    reply = peer.receive()
    assert reply[:14] == PEER_MAC + GUEST_MAC + b"\x08\x06"
    assert reply[22:42] == GUEST_MAC + GUEST_IP + PEER_MAC + PEER_IP
    peer.send(echo(1, b"http-image-ping"))
    reply = peer.receive()
    assert reply[23] == 1 and reply[34:36] == b"\0\0"
    assert checksum(reply[34:14 + struct.unpack('!H', reply[16:18])[0]]) == 0

    # More RX completions than the adapter's per-poll budget must not be lost
    # when notifications are rearmed between foreground batches.
    for sequence in range(24):
        peer.send(echo(100 + sequence, b"idle-burst"))
    seen = set()
    for _ in range(24):
        frame = peer.receive()
        assert frame[23] == 1 and frame[34:36] == b"\0\0"
        sequence, = struct.unpack("!H", frame[40:42])
        assert 100 <= sequence < 124 and sequence not in seen
        seen.add(sequence)
    qmp.assert_idle()
    print("Wakeup: NIC interrupt and 24-packet RX burst drain successfully")

    page = (ROOT / "assets/index.html").read_bytes()
    client = Client(peer)
    client.write(b"GET / HTTP/1.1\r\nHo", wait=True, duplicate=True, corrupt=True)
    client.write(b"st: byte-ring\r\n\r\n")
    response, segments = client.response(lose_first=True)
    validate(response, body=page)
    assert segments >= 3, "large page was not segmented"
    print("GET: exact embedded page, split/duplicate input, corrupt TCP rejected, lost data retransmitted")

    for request, status, expected, head in [
        (b"HEAD / HTTP/1.1\r\nHost: byte-ring\r\n\r\n", 200, page, True),
        (b"GET / HTTP/1.0\r\n\r\n", 200, page, False),
        (b"GET /missing HTTP/1.0\r\n\r\n", 404, b"Not found\n", False),
        (b"POST / HTTP/1.0\r\nContent-Length: 0\r\n\r\n", 405, None, False),
        (b"GET / HTTP/1.1\r\nHost: a\r\nHost: b\r\n\r\n", 400, None, False),
        (b"GET / HTTP/1.0\r\nContent-Length: 1\r\n\r\nx", 400, None, False),
        (b"GET / HTTP/1.0\r\nTransfer-Encoding: chunked\r\n\r\n", 400, None, False),
        (b"GET / HTTP/9.9\r\n\r\n", 505, None, False),
        (b"GET / HTTP/1.0\r\nX: " + b"a" * 1024, 431, None, False),
    ]:
        client = Client(peer)
        client.write(request)
        response, _ = client.response()
        fields = validate(response, status, expected, head)
        if status == 405:
            assert fields[b"allow"] == b"GET, HEAD"
    print("HTTP: HEAD, HTTP/1.0, 404, 405, 400, 505, and 431; graceful close and reconnect")

    client = Client(peer)
    client.write(b"GET /", wait=True)
    started = time.monotonic()
    initial = qmp.assert_apic_deadline()
    peer.send(echo(999, b"timer-rearm"))
    reply = peer.receive()
    assert reply[23] == 1 and reply[34:36] == b"\0\0" and reply[40:42] == struct.pack("!H", 999)
    time.sleep(.02)
    rearmed, current = qmp.apic_timer()
    assert 0 < current <= rearmed < initial, "NIC wakeup must rearm the remaining deadline"
    print("APIC: multi-second one-shot keeps counting past 55 ms; NIC wakeup rearms remaining time")
    response, _ = client.response(timeout=9)
    validate(response, 408)
    assert 3 <= time.monotonic() - started <= 8, "request deadline did not use elapsed time"
    client = Client(peer)
    client.reset()
    time.sleep(.05)
    client = Client(peer)
    client.write(b"GET / HTTP/1.0\r\n\r\n")
    response, _ = client.response()
    validate(response, body=page)
    print("Recovery: incomplete headers time out; client reset and subsequent GET succeed")


def main():
    os.chdir(ROOT)
    subprocess.run(["./scripts/build-kernel.sh", "--http"], check=True)
    logs = ROOT / "target/http/integration" / str(time.time_ns())
    logs.mkdir(parents=True)
    with tempfile.TemporaryDirectory(prefix="byte-ring-http-") as directory, \
            socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener, \
            socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as monitor:
        os.chmod(directory, 0o711)
        path = directory + "/net.sock"
        listener.bind(path)
        os.chmod(path, 0o666)
        listener.listen(1)
        listener.settimeout(30)
        monitor_path = directory + "/qmp.sock"
        monitor.bind(monitor_path)
        os.chmod(monitor_path, 0o666)
        monitor.listen(1)
        monitor.settimeout(30)
        environment = dict(os.environ, QEMU_SOCKET_DIRECTORY=directory, QEMU_TIMEOUT="60s")
        command = ["./scripts/qemu.sh", "-machine", "pc", "-accel", "tcg", "-cpu", "qemu64",
                   "-m", "64M", "-smp", "1", "-display", "none", "-monitor", "none",
                   "-serial", "stdio", "-no-reboot", "-no-shutdown",
                   "-qmp", "unix:" + monitor_path + ",server=off",
                   "-netdev", "stream,id=net0,addr.type=unix,addr.path=" + path,
                   "-device", "virtio-net-pci,disable-legacy=on,netdev=net0,mac=52:54:00:12:34:56",
                   "-kernel", "target/http/x86_64-unknown-none/release/byte-ring-kernel.bin"]
        child = subprocess.Popen(command, env=environment, stdout=subprocess.PIPE,
                                 stderr=subprocess.STDOUT, start_new_session=True)
        ready = threading.Event()
        lines = []

        def collect():
            with (logs / "serial.log").open("w") as output:
                for raw in child.stdout:
                    line = raw.decode("utf-8", errors="replace")
                    lines.append(line)
                    output.write(line)
                    output.flush()
                    if "HTTP ready http://192.0.2.2:8080/" in line:
                        ready.set()
        collector = threading.Thread(target=collect, daemon=True)
        collector.start()
        print("Logs: " + str(logs), flush=True)
        try:
            connection, _ = listener.accept()
            control, _ = monitor.accept()
            with connection, control, (logs / "packets.log").open("w") as trace:
                if not ready.wait(15):
                    raise RuntimeError("HTTP guest did not become ready:\n" + "".join(lines))
                qmp = Qmp(control)
                try:
                    exercise(Peer(connection, trace), qmp)
                    qmp.assert_idle()
                finally:
                    qmp.close()
                assert child.poll() is None, "guest exited unexpectedly"
                assert not any("PANIC" in line or "EXCEPTION" in line for line in lines)
                (logs / "result.txt").write_text("IRQ idle/wakeup, APIC long deadline/rearm, burst drain, HTTP, TCP loss recovery, and reconnect passed.\n")
            print("Task outcome: kernel HTTP and TCP integration passed")
        finally:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait(timeout=5)
            collector.join(timeout=2)



if __name__ == "__main__":
    main()
