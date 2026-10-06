#!/usr/bin/env python3
"""Dependency-free MCP stdio adapter to an explicitly enabled localhost bridge."""
import argparse
import json
import socket
import sys

TOOLS = [
    {"name": "get_state", "description": "Read Spike 4 authority, ships, cabin frame and origin shifts.",
     "inputSchema": {"type": "object", "properties": {}, "additionalProperties": False}},
    {"name": "do_action", "description": "Perform one allowed Spike 4 test action; no eval or script execution.",
     "inputSchema": {"type": "object", "properties": {
         "action": {"type": "string", "enum": ["pilot", "board_remote", "walk_own", "walk", "shift", "observe", "place_ship"]},
         "args": {"type": "object"}}, "required": ["action"], "additionalProperties": False}},
]


def request(port, payload):
    with socket.create_connection(("127.0.0.1", port), timeout=3) as peer:
        peer.sendall((json.dumps(payload) + "\n").encode())
        with peer.makefile("rb") as stream:
            line = stream.readline(65537)
            if not line or len(line) > 65536:
                raise ValueError("missing/oversized bridge response")
            return json.loads(line)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=18441)
    args = parser.parse_args()
    for line in sys.stdin:
        message = {}
        try:
            message = json.loads(line)
            method = message.get("method")
            if "id" not in message:
                continue
            if method == "initialize":
                result = {"protocolVersion": "2024-11-05", "capabilities": {"tools": {}},
                          "serverInfo": {"name": "exo-spike4", "version": "0.1"}}
            elif method == "ping":
                result = {}
            elif method == "tools/list":
                result = {"tools": TOOLS}
            elif method == "tools/call":
                params = message.get("params", {})
                name = params.get("name")
                if name == "get_state":
                    reply = request(args.port, {"op": "get_state"})
                elif name == "do_action":
                    arguments = params.get("arguments", {})
                    if arguments.get("action") not in TOOLS[1]["inputSchema"]["properties"]["action"]["enum"]:
                        raise ValueError("unknown action")
                    reply = request(args.port, {"op": "do_action", **arguments})
                else:
                    raise ValueError("unknown tool")
                result = {"content": [{"type": "text", "text": json.dumps(reply)}],
                          "isError": not reply.get("ok", False)}
            else:
                raise ValueError("unsupported method")
            response = {"jsonrpc": "2.0", "id": message["id"], "result": result}
        except Exception as error:
            response = {"jsonrpc": "2.0", "id": message.get("id"),
                        "error": {"code": -32603, "message": str(error)}}
        print(json.dumps(response), flush=True)


if __name__ == "__main__":
    main()
