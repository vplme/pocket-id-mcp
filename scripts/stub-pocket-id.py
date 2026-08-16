#!/usr/bin/env python3
"""Minimal stand-in for a Pocket ID instance.

`pocket-id-mcp` validates connectivity at startup by calling
GET /api/version/current, so booting it in CI needs *something* listening.
Tool definitions are built from the compiled catalog rather than fetched, so
tools/list is fully exercised against this stub without a real instance.
"""

import argparse
import http.server
import json


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = json.dumps({"current": "0.0.0-stub", "isUpToDate": True}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=8899)
    args = parser.parse_args()
    http.server.HTTPServer(("127.0.0.1", args.port), Handler).serve_forever()
