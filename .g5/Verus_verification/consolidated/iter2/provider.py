"""Disposable loopback registry for source-selected iter2 contrasts, no upstream calls."""
from http.server import BaseHTTPRequestHandler, HTTPServer
import json

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass
    def do_GET(self):
        path = self.path.split('?', 1)[0]
        status, body = 200, {}
        if path.endswith('/tags/list'):
            name = path.split('/')[2]
            body = {'name': name, 'tags': ['2.0.0','1.0.0'] if name == 'g5iter2order' else ['1.0.0','2.0.0']}
        elif '/manifests/' in path:
            good = {'mediaType':'application/vnd.oci.image.layer.v1.tar','size':1,'digest':'sha256:'+'a'*64}
            bad = {'mediaType':'application/vnd.oci.image.layer.v1.tar','size':2,'digest':'sha256:'+'b'*64}
            if 'g5iter2skip' in path:
                bad['mediaType'] = 'application/example'
            layers = [good,bad] if 'g5iter2match' in path else [bad,good]
            body = {'schemaVersion':2,'config':{'mediaType':'application/vnd.unknown.config.v1+json','size':2,'digest':'sha256:'+'0'*64},'layers':layers,'annotations':{'com.intersystems.ipm.module.v1+xml':' <Export/> '}}
        elif path in ['/v2/', '/_ping']:
            body = {}
        else:
            status = 404
        data = json.dumps(body).encode()
        self.send_response(status)
        self.send_header('Content-Type','application/json')
        self.send_header('Docker-Distribution-API-Version','registry/2.0')
        self.send_header('Content-Length',str(len(data)))
        self.end_headers()
        self.wfile.write(data)

if __name__ == '__main__':
    HTTPServer(('127.0.0.1',18291),Handler).serve_forever()
