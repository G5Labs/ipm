#!/usr/bin/env python3
"""Drive the real passt socketpair with guest Ethernet frames in an isolated netns."""
import select
import socket
import struct
import subprocess
import sys
import threading
import time


def serve(address, port, udp=False):
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM if udp else socket.SOCK_STREAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind((address, port))
    if udp:
        while True:
            data, peer = sock.recvfrom(4096)
            sock.sendto(data, peer)
    sock.listen()
    while True:
        conn, _ = sock.accept()
        with conn:
            conn.settimeout(3)
            try:
                if conn.recv(4096):
                    conn.sendall(b"fixture-only\n")
            except (OSError, TimeoutError):
                pass


def probe_host_neighbor():
    # Raw ICMPv6 lets the fixture exercise host-generated neighbor replies
    # without depending on ping. No passt-owned socket participates here.
    with socket.socket(socket.AF_INET6, socket.SOCK_RAW, socket.IPPROTO_ICMPV6) as sock:
        index = socket.if_nametoindex("egress1")
        sock.bind(("fe80::2", 0, 0, index))
        sock.sendto(struct.pack("!BBHHH", 128, 0, 0, 1234, 1) + b"fixture", ("fe80::1", 0, 0, index))
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            sock.settimeout(max(0.01, deadline - time.monotonic()))
            data, _ = sock.recvfrom(4096)
            if data[0] == 129 and data[4:8] == struct.pack("!HH", 1234, 1):
                print("PASS: host IPv6 neighbor discovery and echo reply", flush=True)
                return
        raise AssertionError("host IPv6 neighbor discovery or echo reply failed")


if len(sys.argv) > 1 and sys.argv[1] == "--probe-host-neighbor":
    probe_host_neighbor()
    raise SystemExit()


if len(sys.argv) > 1 and sys.argv[1] == "--serve":
    serve(sys.argv[2], int(sys.argv[3]))
    raise SystemExit()

wire = socket.socket(fileno=int(sys.argv[1]))
policy = sys.argv[2]
guest = "10.85.0.2"
guest_mac = bytes.fromhex("02b500000007")
passt_mac = bytes.fromhex("9a55559a5555")
buffer = bytearray()
source_port = 40000


def checksum(data):
    if len(data) % 2:
        data += b"\0"
    total = sum(struct.unpack(f"!{len(data) // 2}H", data))
    while total >> 16:
        total = (total & 65535) + (total >> 16)
    return (~total) & 65535


def frame(ethertype, payload, target=passt_mac):
    packet = target + guest_mac + struct.pack("!H", ethertype) + payload
    wire.sendall(struct.pack("!I", len(packet)) + packet)


def ipv4(target, protocol, payload):
    src, dst = socket.inet_aton(guest), socket.inet_aton(target)
    header = struct.pack("!BBHHHBBH4s4s", 0x45, 0, 20 + len(payload), 1, 0x4000, 64, protocol, 0, src, dst)
    header = header[:10] + struct.pack("!H", checksum(header)) + header[12:]
    frame(0x800, header + payload)


def tcp(target, port, local, seq, ack, flags, data=b""):
    header = struct.pack("!HHIIBBHHH", local, port, seq, ack, 0x50, flags, 65535, 0, 0)
    pseudo = socket.inet_aton(guest) + socket.inet_aton(target) + struct.pack("!BBH", 0, 6, len(header) + len(data))
    header = header[:16] + struct.pack("!H", checksum(pseudo + header + data)) + header[18:]
    ipv4(target, 6, header + data)


def receive(deadline):
    while time.monotonic() < deadline:
        if len(buffer) >= 4:
            length = struct.unpack("!I", buffer[:4])[0]
            if len(buffer) >= 4 + length:
                packet = bytes(buffer[4:4 + length])
                del buffer[:4 + length]
                # Answer passt's neighbour discovery for its guest endpoint.
                if packet[12:14] == b"\x08\x06":
                    arp = packet[14:]
                    if arp[6:8] == b"\x00\x01":
                        frame(0x806, arp[:6] + b"\x00\x02" + guest_mac + socket.inet_aton(guest) + arp[8:14] + arp[14:18], arp[8:14])
                    continue
                if packet[12:14] != b"\x08\x00":
                    continue
                ip = packet[14:]
                ihl = (ip[0] & 15) * 4
                return socket.inet_ntoa(ip[12:16]), ip[9], ip[ihl:struct.unpack("!H", ip[2:4])[0]]
        ready, _, _ = select.select([wire], [], [], max(0, deadline - time.monotonic()))
        if not ready:
            break
        data = wire.recv(65536)
        if not data:
            raise AssertionError("passt exited before guest test completed")
        buffer.extend(data)
    return None


def probe_tcp(target, port, timeout=1, require_reset=False):
    global source_port
    source_port += 1
    local, seq = source_port, source_port * 1000
    tcp(target, port, local, seq, 0, 2)
    deadline = time.monotonic() + timeout
    while (packet := receive(deadline)) is not None:
        src, proto, body = packet
        if proto != 6 or src != target or struct.unpack("!HH", body[:4]) != (port, local):
            continue
        peer_seq, ack = struct.unpack("!II", body[4:12])
        flags = body[13]
        if flags & 4:
            return False
        if flags & 2:
            tcp(target, port, local, seq + 1, peer_seq + 1, 16)
            tcp(target, port, local, seq + 1, peer_seq + 1, 24, b"probe\n")
        payload = body[(body[12] >> 4) * 4:]
        if payload:
            tcp(target, port, local, ack, peer_seq + len(payload), 20)
            assert payload == b"fixture-only\n", payload
            return True
    assert not require_reset, f"no explicit rejection from {target}:{port}"
    return False


def probe_dns(target):
    global source_port
    source_port += 1
    data = b"\x12\x34\x01\x00" + b"\0" * 8
    ipv4(target, 17, struct.pack("!HHHH", source_port, 53, 8 + len(data), 0) + data)
    deadline = time.monotonic() + 2
    while (packet := receive(deadline)) is not None:
        src, proto, body = packet
        if proto == 17 and src == target and struct.unpack("!HH", body[:4]) == (53, source_port):
            assert body[8:] == data
            return
    raise AssertionError(f"DNS failed through {target}")


def forwarded_reply():
    result = []

    def client():
        try:
            with socket.create_connection(("127.128.0.7", 49152), timeout=3) as conn:
                conn.sendall(b"forward\n")
                result.append(conn.recv(4096))
        except OSError as error:
            result.append(error)

    thread = threading.Thread(target=client)
    thread.start()
    deadline = time.monotonic() + 3
    while thread.is_alive() and (packet := receive(deadline)) is not None:
        src, proto, body = packet
        if proto != 6 or struct.unpack("!H", body[2:4])[0] != 8081:
            continue
        peer_port = struct.unpack("!H", body[:2])[0]
        seq, ack = struct.unpack("!II", body[4:12])
        if body[13] & 2:
            tcp(src, peer_port, 8081, 500, seq + 1, 18)
        data = body[(body[12] >> 4) * 4:]
        if data:
            assert data == b"forward\n"
            tcp(src, peer_port, 8081, ack, seq + len(data), 24, b"forwarded\n")
    thread.join(timeout=4)
    assert result == [b"forwarded\n"], result


subprocess.run(["nft", "list", "table", "inet", "g5_guest_499"], check=True, stdout=subprocess.DEVNULL)
# Flush the baseline's neighbor entries so a cached MAC cannot hide a policy
# that accidentally rejects kernel-generated packets without a socket owner.
subprocess.run(["ip", "-6", "neigh", "flush", "dev", "egress0"], check=True)
subprocess.run(["ip", "netns", "exec", "sec002-peer", "ip", "-6", "neigh", "flush", "dev", "egress1"], check=True)
subprocess.run(["ip", "netns", "exec", "sec002-peer", "python3", __file__, "--probe-host-neighbor"], check=True)
for address, port in [("169.254.169.254", 80), ("169.254.169.254", 443), ("169.254.2.253", 80), ("169.254.169.254", 53), ("127.0.0.1", 80), ("127.0.0.1", 7111), ("127.0.0.1", 8080), ("198.18.0.1", 8888)]:
    threading.Thread(target=serve, args=(address, port), daemon=True).start()
threading.Thread(target=serve, args=("169.254.169.254", 53, True), daemon=True).start()
peer = subprocess.Popen(["ip", "netns", "exec", "sec002-peer", "python3", __file__, "--serve", "198.18.0.2", "8888"])
try:
    # Establish guest MAC ownership and give local fixture listeners time to bind.
    frame(0x806, struct.pack("!HHBBH", 1, 0x800, 6, 4, 1) + guest_mac + socket.inet_aton(guest) + b"\0" * 6 + socket.inet_aton("10.85.0.1"), b"\xff" * 6)
    time.sleep(0.1)
    for target, port in [("169.254.169.254", 80), ("169.254.169.254", 443), ("169.254.2.253", 80), ("169.254.1.2", 80), ("127.0.0.1", 80), ("198.18.0.1", 8888)]:
        assert not probe_tcp(target, port, require_reset=target != "127.0.0.1"), f"forbidden endpoint reachable: {target}:{port}"
    for target, port in [("169.254.1.2", 7111), ("169.254.1.2", 8080), ("198.18.0.2", 8888), ("169.254.169.254", 53)]:
        assert probe_tcp(target, port), f"required endpoint blocked: {target}:{port}"
    probe_dns("169.254.1.1")
    probe_dns("169.254.169.254")
    forwarded_reply()
    print("PASS: actual passt denies metadata and host services; DNS, cache/API, external egress and forwarded replies work", flush=True)

    # A control establishes actual reachability when enforcement is absent.
    subprocess.run(["nft", "delete", "table", "inet", "g5_guest_499"], check=True)
    for target, port in [("169.254.169.254", 80), ("169.254.169.254", 443), ("169.254.2.253", 80), ("169.254.1.2", 80), ("198.18.0.1", 8888)]:
        assert probe_tcp(target, port), f"baseline did not reproduce reachability: {target}:{port}"
    subprocess.run(["nft", "-f", policy], check=True)
    print("PASS: unfiltered baseline reproduces the exposure using fixture-only responses", flush=True)

    # Atomic replace remains safe on retry/concurrent VM starts; failed batches
    # preserve the prior policy instead of leaving the guest unfiltered.
    for _ in range(3):
        first = subprocess.Popen(["nft", "-f", policy])
        second = subprocess.Popen(["nft", "-f", policy])
        assert first.wait() == second.wait() == 0
        assert not probe_tcp("169.254.169.254", 80)
        assert probe_tcp("169.254.1.2", 7111)
    bad = subprocess.run(["nft", "-f", "-"], input=b"delete table inet g5_guest_499\ninvalid rule\n", stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    assert bad.returncode != 0
    assert not probe_tcp("169.254.169.254", 80)
    assert probe_tcp("198.18.0.2", 8888)
    print("PASS: concurrent installs, retries and failed policy replacement retain enforcement", flush=True)
finally:
    peer.terminate()
    peer.wait(timeout=3)
