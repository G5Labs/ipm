"""Disposable TLS provider; fixture credentials only. Records request boundaries."""
from http.server import BaseHTTPRequestHandler,HTTPServer
from pathlib import Path
import io,json,ssl,tarfile
ROOT=Path('/tmp/g5-iter3')
buf=io.BytesIO()
with tarfile.open(fileobj=buf,mode='w:gz') as t:
    b=b'provider-content';m=tarfile.TarInfo('new.txt');m.size=len(b);t.addfile(m,io.BytesIO(b))
PAYLOAD=buf.getvalue()
class Handler(BaseHTTPRequestHandler):
    def log_message(self,*a):pass
    def respond(self,status,body,**headers):
        self.send_response(status)
        for k,v in headers.items():self.send_header(k,v)
        self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
    def record(self):
        with (ROOT/'requests.jsonl').open('a') as f:
            f.write(json.dumps({'method':self.command,'path':self.path,'auth':self.headers.get('Authorization','')})+'\n')
    def do_POST(self):
        self.record();self.rfile.read(int(self.headers.get('Content-Length',0)))
        if self.headers.get('Authorization')!='Bearer fixture-token':return self.respond(401,b'denied')
        names=['payload.tgz']
        if self.path=='/aql-first':names=['older.tgz','newer.tgz']
        if self.path=='/aql-second':names=['newer.tgz','older.tgz']
        repo='secure' if self.path=='/aql-secure' else ('corrupt' if self.path=='/aql-corrupt' else 'public')
        body=json.dumps({'results':[{'repo':repo,'path':'.','name':n} for n in names]}).encode()
        self.respond(200,body)
    def do_GET(self):
        self.record()
        if self.path.startswith('/artifactory/api/search/artifact'):
            return self.respond(200,json.dumps({'results':[{'repo':'corrupt','path':'/corrupt.tgz','lastUpdated':'3'}]}).encode())
        if self.path.startswith('/artifactory/corrupt/'):
            return self.respond(200,b'invalid archive',**{'X-ARTIFACTORY-FILENAME':'invalid.tgz'})
        if '/secure/' in self.path and self.headers.get('Authorization')!='Bearer fixture-token':return self.respond(401,b'protected artifact requires credentials')
        if self.path.startswith('/artifactory/public/'):
            return self.respond(200,PAYLOAD,**{'X-ARTIFACTORY-FILENAME':'payload.tgz'})
        self.respond(404,b'missing')
(ROOT/'provider.pid').write_text(str(__import__('os').getpid()))
(ROOT/'provider-running.sha256').write_text(__import__('hashlib').sha256(Path(__file__).read_bytes()).hexdigest())
server=HTTPServer(('127.0.0.1',18433),Handler)
context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);context.load_cert_chain(ROOT/'cert.pem',ROOT/'key.pem')
server.socket=context.wrap_socket(server.socket,server_side=True)
server.serve_forever()
