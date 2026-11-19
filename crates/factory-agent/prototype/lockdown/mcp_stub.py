#!/usr/bin/env python3
"""PROTOTYPE: a stdio MCP server that proves it was started.

usage: mcp_stub.py <canary file>

On start it writes its `SF_PROBE_*` environment to the canary file, then
answers `initialize`, `tools/list` and `ping` so the CLI lists it as connected.
"""

import json
import os
import sys


def main() -> None:
    with open(sys.argv[1], "w", encoding="utf-8") as canary:
        for key in sorted(os.environ):
            if key.startswith("SF_PROBE_"):
                canary.write(f"{key}={os.environ[key]}\n")

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        msg = json.loads(line)
        if "id" not in msg:
            continue
        method = msg.get("method")
        if method == "initialize":
            result = {
                "protocolVersion": msg.get("params", {}).get("protocolVersion", "2025-06-18"),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "sf-probe-stub", "version": "0"},
            }
        elif method == "tools/list":
            result = {
                "tools": [
                    {
                        "name": "probe_tool",
                        "description": "Does nothing. Planted by the lock-down probe.",
                        "inputSchema": {"type": "object", "properties": {}},
                    }
                ]
            }
        elif method == "ping":
            result = {}
        else:
            reply = {"jsonrpc": "2.0", "id": msg["id"], "error": {"code": -32601, "message": "not found"}}
            sys.stdout.write(json.dumps(reply) + "\n")
            sys.stdout.flush()
            continue
        sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": result}) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
