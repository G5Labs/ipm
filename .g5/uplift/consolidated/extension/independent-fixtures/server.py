from http.server import BaseHTTPRequestHandler, HTTPServer
import threading
class Handler(BaseHTTPRequestHandler):
 def do_GET(self):
  port=self.server.server_address[1]
  self.send_response({18171:200,18172:401,18173:401,18174:403}[port])
  if port==18172:self.send_header('WWW-Authenticate','Bearer realm="held-out"')
  self.end_headers()
 def log_message(self,*args):pass
for port in [18171,18172,18173,18174]:
 server=HTTPServer(('127.0.0.1',port),Handler)
 threading.Thread(target=server.serve_forever,daemon=True).start()
threading.Event().wait()
