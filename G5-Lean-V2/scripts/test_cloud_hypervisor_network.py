#!/usr/bin/env python3
"""Two real CH guests; synthetic peer, ingress and forbidden-route assertions."""
import http.server
import json
import os
import pwd
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request


def serve_http(address, port, marker, requests):
    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            requests.append(self.path)
            self.send_response(200)
            self.end_headers()
            self.wfile.write(marker.encode())
        def log_message(self, *args):
            pass
    server = http.server.ThreadingHTTPServer((address, port), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def serve_dns(address, port):
    def dns_reply(packet):
        if len(packet) < 12:
            return None
        offset = 12
        while offset < len(packet) and packet[offset]:
            if packet[offset] > 63:
                return None
            offset += packet[offset] + 1
        end = offset + 5
        if end > len(packet):
            return None
        query = packet[12:end]
        is_a = packet[end-4:end-2] == b'\x00\x01'
        header = packet[:2] + b'\x81\x80\x00\x01' + (b'\x00\x01' if is_a else b'\x00\x00') + b'\x00\x00\x00\x00'
        answer = b'\xc0\x0c\x00\x01\x00\x01\x00\x00\x00\x01\x00\x04' + socket.inet_aton('198.18.0.2') if is_a else b''
        return header + query + answer
    udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    udp.bind((address, port))
    def udp_dns():
        while True:
            data, addr = udp.recvfrom(4096)
            reply = dns_reply(data)
            if reply is not None:
                udp.sendto(reply, addr)
    threading.Thread(target=udp_dns, daemon=True).start()
    tcp = socket.socket()
    tcp.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    tcp.bind((address, port))
    tcp.listen()
    def tcp_dns():
        while True:
            connection, _ = tcp.accept()
            with connection:
                size = connection.recv(2)
                if len(size) != 2:
                    continue
                length = int.from_bytes(size, 'big')
                data = b''
                while len(data) < length:
                    part = connection.recv(length - len(data))
                    if not part:
                        break
                    data += part
                reply = dns_reply(data)
                if reply is not None:
                    connection.sendall(len(reply).to_bytes(2, 'big') + reply)
    threading.Thread(target=tcp_dns, daemon=True).start()
    return udp, tcp


def peer():
    serve_http('198.18.0.2', 8120, 'PEER_OK', [])
    serve_http('169.254.169.254', 8130, 'FORBIDDEN_METADATA', [])
    serve_dns('169.254.169.254', 53)
    print('PEER_READY', flush=True)
    threading.Event().wait()


GUEST = r'''
import http.server, json, os, socket, struct, threading, urllib.request
from pathlib import Path
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
def get(url):
    return opener.open(url, timeout=3).read().decode()
def denied(host, port):
    try:
        s = socket.create_connection((host, port), timeout=1)
    except OSError:
        return
    s.close()
    raise AssertionError(('forbidden destination reachable', host, port))
assert get('http://g5-host:8110/') == 'API_OK'
assert get('http://legacy-api:8110/') == 'API_OK'
assert get('http://g5-host:8112/') == 'CACHE_OK'
assert get('http://g5-api:8120/') == 'PEER_OK'
# Guest loopback remains private; no TSI transparent host-loopback behavior.
for address, port in [('127.0.0.1', 8110), ('g5-host', 8111),
                      ('169.254.169.254', 8130), ('198.18.0.1', 8111),
                      ('10.85.0.1', 8111), ('198.18.0.1', 1053),
                      ('g5-host', 1053)]:
    denied(address, port)
# Both resolver transports reach only the synthetic DNS in the isolated peer.
query = b'\x12\x34\x01\x00\x00\x01\x00\x00\x00\x00\x00\x00\x06g5-api\x00\x00\x01\x00\x01'
for kind in [socket.SOCK_DGRAM, socket.SOCK_STREAM]:
    with socket.socket(socket.AF_INET, kind) as s:
        s.settimeout(3)
        s.connect(('169.254.1.1', 53))
        s.sendall((len(query).to_bytes(2, 'big') if kind == socket.SOCK_STREAM else b'') + query)
        if kind == socket.SOCK_STREAM:
            size = s.recv(2, socket.MSG_WAITALL)
            reply = s.recv(int.from_bytes(size, 'big'), socket.MSG_WAITALL)
        else:
            reply = s.recv(4096)
        assert socket.inet_aton('198.18.0.2') in reply
role = os.environ['CHECK_ROLE']
assert not Path('/tmp/private-owner').exists()
Path('/tmp/private-owner').write_text(role)
assert Path('/work/owner').read_text() == role
if role == 'second':
    denied(os.environ['OTHER_VM_IP'], 49152)
    denied('g5-host', 49152)
    assert Path('/tmp/private-owner').read_text() == 'second'
    print('NETWORK_SECOND_OK', flush=True)
else:
    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path == '/unfiltered':
                assert get('http://169.254.169.254:8130/') == 'FORBIDDEN_METADATA'
                assert get('http://g5-host:8111/') == 'DENIED'
            elif self.path == '/filtered':
                denied('169.254.169.254', 8130)
                denied('g5-host', 8111)
            self.send_response(200)
            self.end_headers()
            self.wfile.write(Path('/tmp/private-owner').read_bytes())
            if self.path == '/stop':
                threading.Thread(target=self.server.shutdown, daemon=True).start()
        def log_message(self, *args): pass
    server = http.server.ThreadingHTTPServer(('0.0.0.0', 8081), Handler)
    print('NETWORK_FIRST_READY', flush=True)
    server.serve_forever()
    print('NETWORK_FIRST_OK', flush=True)
'''


def main():
    root = Path(os.environ['G5_TEST_VM_ROOT'])
    root.mkdir(parents=True, exist_ok=True)
    peer_proc = subprocess.Popen(['ip', 'netns', 'exec', 'g5-ch-peer', sys.executable, __file__, '--peer'], stdout=subprocess.PIPE, text=True)
    assert peer_proc.stdout.readline().strip() == 'PEER_READY'
    # Validate the synthetic resolver itself before attributing a failure to passt.
    query = b'\x12\x34\x01\x00\x00\x01\x00\x00\x00\x00\x00\x00\x06g5-api\x00\x00\x01\x00\x01'
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as probe:
        probe.settimeout(2)
        probe.sendto(b'bad', ('169.254.169.254', 53))
        probe.sendto(query, ('169.254.169.254', 53))
        assert socket.inet_aton('198.18.0.2') in probe.recv(4096)
    with socket.create_connection(('169.254.169.254', 53), timeout=2) as probe:
        probe.sendall(b'\x00\x03bad')
    print('SYNTHETIC_DNS_READY', flush=True)
    forbidden = []
    servers = [serve_http('127.0.0.1', 8110, 'API_OK', []),
               serve_http('0.0.0.0', 8111, 'DENIED', forbidden),
               serve_http('127.0.0.1', 8112, 'CACHE_OK', []),
               serve_http('198.18.0.1', 1053, 'DENIED', forbidden),
               serve_http('127.0.0.1', 1053, 'DENIED', forbidden)]
    guests = []
    directories = []
    success = False
    def boot(role, other=''):
        owned = Path(tempfile.mkdtemp(prefix='net-', dir=root))
        directories.append(owned)
        work = Path(tempfile.mkdtemp(prefix='net-work-', dir=root))
        directories.append(work)
        (work / 'owner').write_text(role)
        (work / 'check.py').write_text(GUEST)
        rootfs = owned / 'rootfs'
        (rootfs / '.g5').mkdir(parents=True)
        environment = os.environ.copy()
        environment.update(G5D_INTERNAL_OWNED_WORKLOAD_ROOT=str(rootfs),
                           G5D_INTERNAL_WORKLOAD_TEMPLATE=os.environ['G5_TEST_TEMPLATE'],
                           G5D_PASST_HOST_API_URL='http://localhost:8110',
                           G5D_CACHE_DOOR_PORT='8112',
                           G5_HOST_JWT='environment-hygiene-fixture',
                           OPENAI_API_KEY='environment-hygiene-fixture')
        log = (owned / 'console.log').open('w')
        process = subprocess.Popen([os.environ['G5D_BIN'], 'vm-exec', '--rootfs', str(rootfs),
            '--cpus', '2', '--mem-mib', '2048', '--scratch-disk-mib', os.environ.get('G5_TEST_DISK_MIB', '32768'),
            '--block-workload', '--guest-host-alias', 'legacy-api=127.0.0.1', '-V', f'{work}:/work', '-V', os.environ['G5_TEST_RUNTIME'] + ':/opt/g5:ro',
            '-p', ('49152:8081' if role == 'first' else '49153:8081'),
            '-e', f'CHECK_ROLE={role}', '-e', f'OTHER_VM_IP={other}', '--', 'python3', '/work/check.py'],
            env=environment, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        guests.append((process, owned, log))
        pid = process.pid
        address = f'127.{128 | ((pid >> 16) & 63)}.{(pid >> 8) & 255}.{pid & 255}'
        return process, owned, address
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    try:
        first, first_root, address = boot('first')
        deadline = time.monotonic() + 90
        while True:
            assert first.poll() is None, (first_root / 'console.log').read_text()
            try:
                assert opener.open(f'http://{address}:49152/', timeout=1).read() == b'first'
                break
            except OSError:
                assert time.monotonic() < deadline, (first_root / 'console.log').read_text()
                time.sleep(.2)
        # Inspect the production constructors, not a separately sanitized harness.
        children = []
        for proc in Path('/proc').iterdir():
            if not proc.name.isdigit():
                continue
            try:
                stat = (proc / 'stat').read_text().rsplit(') ', 1)[1].split()
                if int(stat[1]) == first.pid:
                    executable = (proc / 'exe').resolve().name.split('.')[0]
                    if executable in ('cloud-hypervisor', 'passt'):
                        assert (proc / 'environ').read_bytes() == b'', executable
                        children.append(executable)
            except (FileNotFoundError, ProcessLookupError):
                continue
        assert sorted(children) == ['cloud-hypervisor', 'passt'], children
        assert not forbidden, forbidden
        # The namespace contains only synthetic services. Prove that the policy
        # caused refusal, then let the next normal boot reinstall it atomically.
        uid = pwd.getpwnam('g5-passt').pw_uid
        subprocess.run(['nft', 'delete', 'table', 'inet', f'g5_guest_{uid}'], check=True)
        assert opener.open(f'http://{address}:49152/unfiltered', timeout=10).read() == b'first'
        assert forbidden == ['/'], forbidden
        forbidden.clear()
        # Docker's embedded resolver DNATs port 53 to a loopback high port
        # before the output filter runs. Exercise both transports through that
        # shape while direct connections to the same high port remain denied.
        serve_dns('127.0.0.11', 1053)
        subprocess.run(['nft', '-f', '-'], input="""table ip test_embedded_dns {
 chain output { type nat hook output priority -100; policy accept;
  ip daddr 127.0.0.11 udp dport 53 dnat to 127.0.0.11:1053
  ip daddr 127.0.0.11 tcp dport 53 dnat to 127.0.0.11:1053
 }
}
""", text=True, check=True)
        Path('/etc/resolv.conf').write_text('nameserver 127.0.0.11\n')
        second, second_root, _ = boot('second', address)
        assert second.wait(timeout=90) == 0, (second_root / 'console.log').read_text()
        assert 'NETWORK_SECOND_OK' in (second_root / 'console.log').read_text()
        print('CH_EMBEDDED_DNS_NAT_OK: UDP/TCP DNS; direct high-port access denied', flush=True)
        assert opener.open(f'http://{address}:49152/filtered', timeout=10).read() == b'first'
        assert opener.open(f'http://{address}:49152/stop', timeout=3).read() == b'first'
        assert first.wait(timeout=30) == 0, (first_root / 'console.log').read_text()
        assert 'NETWORK_FIRST_OK' in (first_root / 'console.log').read_text()
        assert not forbidden, forbidden
        success = True
        print('CH_NETWORK_ISOLATION_OK: DNS UDP/TCP, API/cache doors, ingress, metadata/other-host-port/cross-VM denial, private writes')
    finally:
        for process, owned, log in guests:
            if process.poll() is None:
                process.kill()
            process.wait(timeout=10)
            log.close()
            print((owned / 'console.log').read_text())
        peer_proc.terminate()
        peer_proc.wait(timeout=5)
        for server in servers:
            server.shutdown()
        # No host mount/parse of guest-written disks. The enclosing test unit
        # kills remaining descendants on failure; retain failed inputs for diagnosis.
        if success:
            for owned in directories:
                shutil.rmtree(owned)


if __name__ == '__main__':
    if '--peer' in sys.argv:
        peer()
    else:
        main()
