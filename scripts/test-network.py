#!/usr/bin/env python3
"""Independent Ethernet peer for QEMU's stream netdev (no TAP or raw socket)."""
import argparse
import os
from pathlib import Path
import shutil
import signal
import socket
import struct
import subprocess
import tempfile
import threading
import time

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


def validate_echo(frame, sequence, payload):
    assert frame[:6] == PEER_MAC and frame[6:12] == GUEST_MAC
    assert frame[12:14] == b"\x08\x00" and frame[14] == 0x45
    total, = struct.unpack("!H", frame[16:18])
    assert total == 28 + len(payload) and len(frame) >= total + 14
    assert frame[23] == 1 and frame[26:30] == GUEST_IP and frame[30:34] == PEER_IP
    assert checksum(frame[14:34]) == 0 and checksum(frame[34:14 + total]) == 0
    assert frame[34:36] == b"\0\0" and frame[38:42] == struct.pack("!HH", 0x1234, sequence)
    assert frame[42:14 + total] == payload
    assert not any(frame[14 + total:]), "Ethernet padding leaked old bytes"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--demo", action="store_true")
    parser.add_argument("--wrap", action="store_true")
    args = parser.parse_args()
    os.chdir(ROOT)
    subprocess.run(["./scripts/build-kernel.sh", "--network"], check=True)
    mode = "wrap" if args.wrap else ("demo" if args.demo else "integration")
    logs = ROOT / "target/network" / mode / str(time.time_ns())
    logs.mkdir(parents=True, exist_ok=True)
    directory = tempfile.mkdtemp(prefix="byte-ring-net-")
    os.chmod(directory, 0o711)
    path = directory + "/net.sock"
    listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    listener.bind(path)
    # Only the random test socket is writable across container UID mappings;
    # the directory and entire repository remain non-writable by the guest.
    os.chmod(path, 0o666)
    listener.listen(1)
    listener.settimeout(30)
    environment = dict(os.environ, QEMU_SOCKET_DIRECTORY=directory,
                       QEMU_TIMEOUT="180s" if args.wrap else "40s")
    command = ["./scripts/qemu.sh", "-machine", "pc", "-accel", "tcg",
               "-cpu", "qemu64", "-m", "64M", "-smp", "1", "-display", "none",
               "-monitor", "none", "-serial", "stdio", "-no-reboot", "-no-shutdown",
               "-netdev", "stream,id=net0,addr.type=unix,addr.path=" + path,
               "-device", "virtio-net-pci,disable-legacy=on,netdev=net0,mac=52:54:00:12:34:56",
               "-kernel", "target/network/x86_64-unknown-none/release/byte-ring-kernel.bin"]
    lines = []
    ready = threading.Event()
    child = subprocess.Popen(command, env=environment, stdout=subprocess.PIPE,
                             stderr=subprocess.STDOUT, start_new_session=True)

    def collect():
        with (logs / "serial.log").open("w") as output:
            for raw in child.stdout:
                line = raw.decode("utf-8", errors="replace")
                lines.append(line)
                output.write(line)
                output.flush()
                if args.demo:
                    print(line, end="", flush=True)
                if "virtio-net ready ip=192.0.2.2/24 mac=52:54:00:12:34:56" in line:
                    ready.set()
    collector = threading.Thread(target=collect, daemon=True)
    collector.start()
    packet_log = None
    try:
        connection, _ = listener.accept()
        with connection:
            connection.settimeout(3)
            if not ready.wait(15):
                raise RuntimeError("guest did not become ready:\n" + "".join(lines))
            packet_log = (logs / "packets.log").open("w")
            trace_count = 0

            def trace(direction, frame):
                nonlocal trace_count
                if trace_count < 512:
                    packet_log.write(direction + " " + frame.hex() + "\n")
                    packet_log.flush()
                trace_count += 1

            def send_packet(frame):
                trace("TX", frame)
                send(connection, frame)

            def receive_packet():
                frame = receive(connection)
                trace("RX", frame)
                return frame

            send_packet(arp())
            reply = receive_packet()
            assert reply[:14] == PEER_MAC + GUEST_MAC + b"\x08\x06"
            assert reply[14:22] == struct.pack("!HHBBH", 1, 0x800, 6, 4, 2)
            assert reply[22:42] == GUEST_MAC + GUEST_IP + PEER_MAC + PEER_IP
            assert not any(reply[42:])
            count = 65540 if args.wrap else (4 if args.demo else 96)
            for index in range(count):
                sequence = index & 0xffff
                payload = [b"", b"odd", b"\0\xff\x01", bytes(range(256)), b"x" * 1472][index % 5]
                send_packet(echo(sequence, payload))
                validate_echo(receive_packet(), sequence, payload)
                if args.demo:
                    print("reply from 192.0.2.2 seq=%d bytes=%d" % (sequence, len(payload)), flush=True)
                    time.sleep(0.05)
            # Known malformed/unsupported traffic must not receive a response.
            malformed = bytearray(echo(500))
            malformed[36] ^= 1
            options = bytearray(echo(501)); options[14] = 0x46
            fragment = bytearray(echo(502)); fragment[20:22] = b"\x20\x00"
            fragment[24:26] = b"\0\0"
            fragment[24:26] = struct.pack("!H", checksum(bytes(fragment[14:34])))
            for bad in [bytes(malformed), echo(503, destination=bytes([192,0,2,3])), bytes(options), bytes(fragment), echo(504)[:30]]:
                send_packet(bad)
                connection.settimeout(0.08)
                try:
                    unexpected = connection.recv(1)
                    raise AssertionError("reply bytes or disconnect after invalid packet: " + unexpected.hex())
                except socket.timeout:
                    pass
            connection.settimeout(3)
            send_packet(echo(600, b"still-alive"))
            validate_echo(receive_packet(), 600, b"still-alive")
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline and not any("ICMP echo src=192.0.2.1" in line and "seq=0 " in line and "reply=queued" in line for line in lines):
                time.sleep(0.01)
            assert any("ICMP echo src=192.0.2.1" in line and "seq=0 " in line and "reply=queued" in line for line in lines), "missing ping log"
            assert not any("PANIC" in line or "EXCEPTION" in line for line in lines)
            assert child.poll() is None, "guest exited unexpectedly"
            (logs / "result.txt").write_text("ARP and %d echo exchanges passed; malformed traffic ignored.\n" % (count + 1))
            print("Logs: %s" % logs)
            print("Network test passed: static IPv4, ARP, %d echo replies, malformed traffic, and serial logs." % (count + 1))
    except BaseException:
        print("Network test failed. Logs: %s\n%s" % (logs, "".join(lines[-12:])), flush=True)
        raise
    finally:
        if child.poll() is None:
            os.killpg(child.pid, signal.SIGTERM)
            try:
                child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
        collector.join(timeout=3)
        if packet_log is not None:
            packet_log.close()
        listener.close()
        shutil.rmtree(directory)


if __name__ == "__main__":
    main()
