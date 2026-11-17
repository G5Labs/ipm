"""Stand-in HTTP server for OrasContentTrust tests: python3 server.py <mode> <port> <seconds>

serve:  answers every GET with 64 bytes of "x", like a registry returning tampered content.
stall:  answers GET /v2/ like a trust server, then never answers anything else.
Stops after <seconds>.
"""
import http.server
import sys
import threading
import time

mode, port, lifetime = sys.argv[1], int(sys.argv[2]), float(sys.argv[3])


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if mode == "stall" and self.path != "/v2/":
            time.sleep(lifetime)
            return
        data = b"{}" if self.path == "/v2/" else b"x" * 64
        self.send_response(200)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *args):
        pass


server = http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler)
server.daemon_threads = True
threading.Timer(lifetime, server.shutdown).start()
server.serve_forever()
