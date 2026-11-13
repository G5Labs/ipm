"""Authenticated smart-HTTP fixture backed by the real git http-backend."""
import base64
import http.server
import os
import subprocess
import sys
import urllib.parse

root, receipt, git = sys.argv[1:]
expected = "Basic " + base64.b64encode(b"x-access-token:test-worker-token").decode()


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def serve_git(self):
        url = urllib.parse.urlsplit(self.path)
        authorized = self.headers.get("Authorization") == expected
        with open(receipt, "a", encoding="utf-8") as output:
            output.write(f"{self.command} {url.path} auth={str(authorized).lower()}\n")
        if not authorized:
            self.send_response(401)
            self.end_headers()
            return
        env = dict(os.environ, GIT_PROJECT_ROOT=root, GIT_HTTP_EXPORT_ALL="1",
                   PATH_INFO=url.path.replace("/git/repo-1", "/origin.git", 1),
                   QUERY_STRING=url.query, REQUEST_METHOD=self.command,
                   CONTENT_TYPE=self.headers.get("Content-Type", ""),
                   REMOTE_USER="fixture")
        length = int(self.headers.get("Content-Length", "0"))
        env["CONTENT_LENGTH"] = str(length)
        result = subprocess.run([git, "http-backend"], env=env,
                                input=self.rfile.read(length), capture_output=True,
                                check=True, timeout=15)
        headers, body = result.stdout.split(b"\r\n\r\n", 1)
        lines = [line.decode().split(":", 1) for line in headers.split(b"\r\n")]
        status = next((int(v.strip().split()[0]) for k, v in lines if k == "Status"), 200)
        self.send_response(status)
        for key, value in lines:
            if key != "Status":
                self.send_header(key, value.strip())
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    do_GET = serve_git
    do_POST = serve_git


server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
print(server.server_address[1], flush=True)
server.serve_forever()
