"""Serve generated npm tarballs through a temporary loopback registry fixture."""

import argparse
import base64
import hashlib
import json
import tarfile
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import unquote, urlsplit

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("directory", type=Path)
parser.add_argument("port_file", type=Path)
args = parser.parse_args()
packages, tarballs = {}, {}
for path in args.directory.glob("*.tgz"):
    with tarfile.open(path, "r:gz") as archive:
        with archive.extractfile("package/package.json") as source:
            manifest = json.load(source)
    data = path.read_bytes()
    tarballs[path.name] = data
    manifest["dist"] = {
        "integrity": "sha512-"
        + base64.b64encode(hashlib.sha512(data).digest()).decode(),
        "shasum": hashlib.sha1(data).hexdigest(),
        "filename": path.name,
    }
    packages[manifest["name"]] = manifest


class Registry(BaseHTTPRequestHandler):
    def log_message(self, *arguments):
        pass

    def do_GET(self):
        path = unquote(urlsplit(self.path).path)
        if path.startswith("/tarballs/") and path[len("/tarballs/") :] in tarballs:
            body = tarballs[path[len("/tarballs/") :]]
            content_type = "application/octet-stream"
        else:
            name = path[1:]
            direct = name not in packages
            if direct:
                name = name.rsplit("/", 1)[0]
            if name not in packages:
                self.send_error(404)
                return
            manifest = dict(packages[name])
            manifest["dist"] = {
                **manifest["dist"],
                "tarball": f"http://127.0.0.1:{self.server.server_port}/tarballs/{manifest['dist']['filename']}",
            }
            metadata = (
                manifest
                if direct
                else {
                    "name": name,
                    "dist-tags": {"latest": manifest["version"]},
                    "versions": {manifest["version"]: manifest},
                    "time": {manifest["version"]: "2020-01-01T00:00:00.000Z"},
                }
            )
            body = json.dumps(metadata).encode()
            content_type = "application/json"
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


server = ThreadingHTTPServer(("127.0.0.1", 0), Registry)
args.port_file.write_text(str(server.server_port))
server.serve_forever()
