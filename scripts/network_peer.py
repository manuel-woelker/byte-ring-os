"""Independent Ethernet helpers for rootless QEMU network tests."""
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parent.parent
GUEST_MAC = bytes.fromhex("525400123456")
PEER_MAC = bytes.fromhex("020000000001")
GUEST_IP = bytes([192, 0, 2, 2])
PEER_IP = bytes([192, 0, 2, 1])


def checksum(data):
    data += b"\0" if len(data) % 2 else b""
    value = sum(struct.unpack("!%dH" % (len(data) // 2), data))
    while value >> 16:
        value = (value & 0xffff) + (value >> 16)
    return (~value) & 0xffff


def arp():
    return (b"\xff" * 6 + PEER_MAC + b"\x08\x06" +
            struct.pack("!HHBBH", 1, 0x800, 6, 4, 1) +
            PEER_MAC + PEER_IP + b"\0" * 6 + GUEST_IP)


def echo(sequence, payload=b"byte-ring-ping", destination=GUEST_IP):
    icmp = struct.pack("!BBHHH", 8, 0, 0, 0x1234, sequence) + payload
    icmp = icmp[:2] + struct.pack("!H", checksum(icmp)) + icmp[4:]
    ip = struct.pack("!BBHHHBBH4s4s", 0x45, 0, 20 + len(icmp), sequence,
                     0x4000, 64, 1, 0, PEER_IP, destination)
    ip = ip[:10] + struct.pack("!H", checksum(ip)) + ip[12:]
    return GUEST_MAC + PEER_MAC + b"\x08\x00" + ip + icmp


def exact(connection, length):
    result = bytearray()
    while len(result) < length:
        chunk = connection.recv(length - len(result))
        if not chunk:
            raise RuntimeError("QEMU disconnected before a complete packet")
        result.extend(chunk)
    return bytes(result)


def receive(connection):
    length, = struct.unpack("!I", exact(connection, 4))
    if not 14 <= length <= 2048:
        raise RuntimeError("invalid QEMU packet length: %d" % length)
    return exact(connection, length)


def send(connection, frame):
    connection.sendall(struct.pack("!I", len(frame)) + frame)




class Qmp:
    """Small synchronous QMP client; unsolicited events do not consume replies."""
    def __init__(self, connection):
        import json
        self.json = json
        connection.settimeout(4)
        self.stream = connection.makefile("rwb", buffering=0)
        assert "QMP" in self.json.loads(self.stream.readline())
        self.identifier = 0
        self.execute("qmp_capabilities")

    def execute(self, command, arguments=None):
        self.identifier += 1
        self.stream.write(self.json.dumps({"execute": command, "arguments": arguments or {}, "id": self.identifier}).encode() + b"\n")
        while True:
            message = self.json.loads(self.stream.readline())
            if message.get("id") == self.identifier:
                assert "error" not in message, message
                return message["return"]

    def halted(self):
        # HMP register inspection reports the actual x86 CPU halt state. A
        # monitor query is not a guest interrupt and cannot hide a missing IRQ.
        registers = self.execute("human-monitor-command", {"command-line": "info registers"})
        return "HLT=1" in registers

    def assert_idle(self):
        import time
        time.sleep(.2)
        assert self.halted(), "quiet guest is busy instead of halted"
        before = self.execute("human-monitor-command", {"command-line": "info irq"})
        time.sleep(.1)
        assert self.halted(), "quiet guest did not remain halted"
        after = self.execute("human-monitor-command", {"command-line": "info irq"})
        assert before == after, "interrupt activity continued during quiet idle"

    def apic_timer(self):
        import re
        state = self.execute("human-monitor-command", {"command-line": "info lapic"})
        timer = re.search(r"LVTT\s+0x([0-9a-fA-F]+)", state)
        counts = re.search(r"initial_count = (\d+) current_count = (\d+)", state)
        assert timer and counts, "missing local APIC timer state: " + state
        assert int(timer[1], 16) == 48, "timer must be unmasked one-shot vector 48"
        assert "divide by 128" in state and "APIC enabled" in state
        return int(counts[1]), int(counts[2])

    def assert_apic_deadline(self):
        import time
        time.sleep(.05)  # Let ACK TX completion wakeups drain before sampling.
        initial, current = self.apic_timer()
        assert 0 < current <= initial
        assert self.halted(), "deadline wait should halt the CPU"
        before = self.execute("human-monitor-command", {"command-line": "info irq"})
        time.sleep(.15)  # Nearly three old PIT maximum countdowns.
        after_initial, after_current = self.apic_timer()
        assert after_initial == initial, "quiet long deadline was rearmed"
        assert 0 < after_current < current, "APIC counter did not keep counting down"
        assert self.halted(), "long deadline wait should remain halted"
        after = self.execute("human-monitor-command", {"command-line": "info irq"})
        assert before == after, "legacy PIC timer activity during APIC deadline"
        return initial

    def close(self):
        self.stream.close()
