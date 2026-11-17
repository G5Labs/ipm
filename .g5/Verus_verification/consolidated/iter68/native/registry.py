"""Loopback OCI fixture with two benign revisions of a single test module."""

import gzip
import hashlib
import io
import json
import tarfile
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parent
REPO = "fixture/g5-admission"
TAG = "1.0.0"


def archive(files):
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w") as tar:
        for name, content in files.items():
            data = content.encode()
            entry = tarfile.TarInfo(name)
            entry.size = len(data)
            tar.addfile(entry, io.BytesIO(data))
    return gzip.compress(output.getvalue(), mtime=0)


def digest(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def descriptor(data, media, title=None):
    value = {"digest": digest(data), "size": len(data), "mediaType": media}
    if title:
        value["annotations"] = {"org.opencontainers.image.title": title}
    return value


MANIFESTS = {}
BLOBS = {}
for revision in (1, 2):
    xml = f"""<?xml version="1.0" encoding="UTF-8"?>
<Export generator="IRIS" version="26"><Document name="g5-admission.ZPM"><Module>
<Name>g5-admission</Name><Version>1.0.0</Version><Description>revision-{revision}</Description>
<Packaging>module</Packaging><SourcesRoot>src</SourcesRoot>
<Resource Name="G5Admission.Sample.CLS"/>
</Module></Document></Export>"""
    code = f"""Class G5Admission.Sample
{{
ClassMethod Revision() As %Integer
{{
    quit {revision}
}}
}}
"""
    package = archive({"module.xml": xml, "src/G5Admission/Sample.cls": code})
    package_layer = descriptor(package, "application/vnd.oci.image.layer.v1.tar", "package.tgz")
    config = b"{}"
    manifest = {
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": descriptor(config, "application/vnd.oci.empty.v1+json"),
        "layers": [package_layer],
        "annotations": {
            "org.opencontainers.image.description": f"revision-{revision}",
            "com.intersystems.ipm.module.v1+xml": xml,
        },
    }
    BLOBS[digest(package)] = package
    MANIFESTS[str(revision)] = json.dumps(manifest).encode()

directory = archive({"g5-iter68-extraction-marker.txt": "directory layer was unpacked"})
directory_layer = descriptor(directory, "application/vnd.oci.image.layer.v1.tar+gzip", "directory")
BLOBS[digest(directory)] = directory
mismatch = json.loads(MANIFESTS["2"])
# Distinguish the provider effect from IPM's later size check.
mismatch["layers"][0]["size"] += 1
mismatch["layers"].insert(0, directory_layer)
MANIFESTS["size-mismatch"] = json.dumps(mismatch).encode()


class Registry(BaseHTTPRequestHandler):
    retag_lookups = 0

    def do_GET(self):
        path = urlsplit(self.path).path
        mode = (ROOT / "mode").read_text().strip()
        prefix = "/v2/" + REPO
        status, media = 200, "application/json"

        if path in ("/v2", "/v2/"):
            body = b"{}"
        elif path == "/v2/_catalog":
            body = json.dumps({"repositories": [REPO]}).encode()
        elif path == f"{prefix}/tags/list":
            body = json.dumps({"name": REPO, "tags": [TAG]}).encode()
        elif path == f"{prefix}/manifests/{TAG}":
            if mode == "retag":
                type(self).retag_lookups += 1
                # First use: catalog. Second use: dependency manifest.
                # Later uses: package retrieval and post-pull hash lookup.
                revision = "1" if type(self).retag_lookups <= 2 else "2"
            else:
                revision = mode
            body = MANIFESTS[revision]
            media = "application/vnd.oci.image.manifest.v1+json"
        elif path.startswith(f"{prefix}/blobs/") and path.split("/")[-1] in BLOBS:
            body = BLOBS[path.split("/")[-1]]
            media = "application/octet-stream"
        else:
            status, body = 404, b"{}"

        print(f"GET|{mode}|{path}|{status}|{digest(body)}", flush=True)
        self.send_response(status)
        self.send_header("Content-Type", media)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Docker-Distribution-API-Version", "registry/2.0")
        self.send_header("Docker-Content-Digest", digest(body))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_args):
        pass


if __name__ == "__main__":
    HTTPServer(("127.0.0.1", 18292), Registry).serve_forever()
