#!/usr/bin/env python3
"""Disposable two-origin package registry for iteration 5 native cases (iteration 4 provider plus redirects).

Port 18501 is the configured registry origin; port 18502 is a different origin that the
metadata can name as the download URL. Every request is appended to requests.jsonl with
the authentication headers it carried, so a case can tell which origin saw which credential.
"""
import gzip, hashlib, http.server, io, json, tarfile, threading
from pathlib import Path

ROOT = Path('/tmp/g5-iter5')
LOG = ROOT / 'requests.jsonl'


def archive(name):
    xml = ('<?xml version="1.0" encoding="UTF-8"?>\n<Export generator="IRIS" version="26">'
           f'<Document name="{name}.ZPM"><Module><Name>{name}</Name><Version>1.0.0</Version>'
           '<Packaging>module</Packaging></Module></Document></Export>\n').encode()
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode='w', format=tarfile.USTAR_FORMAT) as tf:
        info = tarfile.TarInfo('module.xml'); info.size = len(xml); info.mtime = 0
        tf.addfile(info, io.BytesIO(xml))
    return gzip.compress(raw.getvalue(), mtime=0)


A = archive('g5i4-plain')
BYTES = {'same': A, 'cross': A, 'mismatch': A, 'upper': A, 'nohash': b'not an archive',
         'g5i4-good': archive('g5i4-good'), 'g5i4-bad': archive('g5i4-bad'), 'g5i4-junk': b'not an archive'}
SHA = {k: hashlib.sha1(v).hexdigest() for k, v in BYTES.items()}
META = {
    'same': ('http://localhost:18501/download/same/a.tgz', SHA['same']),
    'cross': ('http://127.0.0.1:18502/download/cross/a.tgz', SHA['cross']),
    'mismatch': ('http://localhost:18501/download/mismatch/a.tgz', '0' * 40),
    'nohash': ('http://localhost:18501/download/nohash/a.tgz', ''),
    'upper': ('http://localhost:18501/download/upper/a.tgz', SHA['upper'].upper()),
    'redirect': ('http://localhost:18501/redirect/cross', SHA['same']),
    'dl404': ('http://localhost:18501/download/missing/a.tgz', SHA['same']),
    'g5i4-good': ('http://localhost:18501/download/g5i4-good/a.tgz', SHA['g5i4-good']),
    'g5i4-bad': ('http://localhost:18501/download/g5i4-bad/a.tgz', '0' * 40),
    'g5i4-junk': ('http://localhost:18501/download/g5i4-junk/a.tgz', ''),
}
LISTED = {'g5i4-good', 'g5i4-bad', 'g5i4-junk'}
lock = threading.Lock()


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def record(self):
        row = {'port': self.server.server_address[1], 'method': self.command, 'path': self.path,
               'host': self.headers.get('Host'), 'authorization': self.headers.get('Authorization'),
               'apikey': self.headers.get('apikey'), 'installtoken': self.headers.get('INSTALLTOKEN')}
        with lock, LOG.open('a') as f:
            f.write(json.dumps(row) + '\n')

    def send(self, code, body=b'', ctype='application/json', headers=()):
        self.send_response(code)
        self.send_header('Content-Type', ctype)
        self.send_header('Content-Length', str(len(body)))
        for k, v in headers:
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        self.record()
        length = int(self.headers.get('Content-Length') or 0)
        self.rfile.read(length)
        self.send(200, b'{}')

    def do_GET(self):
        self.record()
        path = self.path.split('?')[0].strip('/')
        parts = path.split('/') if path else []
        if parts == ['_ping']:
            return self.send(200, b'{}')
        if not parts:
            return self.send(200, json.dumps({'version': 'g5i4', 'available': True}).encode())
        if parts[0] == 'redirect' and len(parts) == 2:
            self.send_response(302)
            self.send_header('Location', 'http://127.0.0.1:18502/download/' + parts[1] + '/a.tgz')
            self.send_header('Content-Length', '0')
            self.end_headers()
            return
        if parts[0] == 'download' and len(parts) >= 2 and parts[1] in BYTES:
            return self.send(200, BYTES[parts[1]], 'application/octet-stream',
                             [('INSTALLTOKEN', 'tok-' + parts[1])])
        if parts[0] == 'packages' and len(parts) == 2 and parts[1] in LISTED:
            row = {'name': parts[1], 'versions': ['1.0.0'], 'allVersions': ['1.0.0'],
                   'repository': '', 'description': '', 'deployed': False}
            return self.send(200, json.dumps(row).encode())
        if parts[0] == 'packages' and len(parts) == 3 and parts[1] in META:
            url, digest = META[parts[1]]
            return self.send(200, json.dumps({'name': parts[1], 'version': parts[2], 'url': url, 'hash': digest}).encode())
        if parts[0] == 'packages' and len(parts) == 4 and parts[3] == 'manifest' and parts[1] in LISTED:
            source = BYTES['g5i4-good' if parts[1] == 'g5i4-junk' else parts[1]]
            xml = tarfile.open(fileobj=io.BytesIO(gzip.decompress(source))).extractfile('module.xml').read()
            xml = xml.replace(b'g5i4-good', parts[1].encode())
            return self.send(200, xml, 'application/xml')
        return self.send(404, b'{}')


if __name__ == '__main__':
    ROOT.mkdir(parents=True, exist_ok=True)
    (ROOT / 'registry-sha1.json').write_text(json.dumps(SHA, indent=2))
    servers = [http.server.ThreadingHTTPServer(('0.0.0.0', p), Handler) for p in (18501, 18502)]
    for s in servers[1:]:
        threading.Thread(target=s.serve_forever, daemon=True).start()
    servers[0].serve_forever()
