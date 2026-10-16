"""
MCP (Model Context Protocol) JSON-RPC 2.0 Server over stdio for A11y-SSH.
Allows Claude, Gemini, Cursor, or any MCP-compatible LLM agent (locally or over SSH)
to inspect the desktop with explicit verbosity levels.
"""

import json
import sys
from a11y_engine import WindowsA11yEngine

engine = WindowsA11yEngine()

TOOLS = [
    {
        "name": "observe_desktop",
        "description": (
            "Read-only semantic GUI-SSH observer. Extracts OS accessibility hierarchy and "
            "unrendered off-screen document text at 5 selectable verbosity levels."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "Window HWND, PID, process name, or title substring. Use 'all' for a desktop-wide overview.",
                    "default": "all",
                },
                "verbosity": {
                    "type": "string",
                    "enum": ["glance", "text_only", "interactive", "compact", "full"],
                    "description": (
                        "Token budget / detail level:\n"
                        "- 'glance' (~80 tokens): Open windows, focus state, and buffer counts.\n"
                        "- 'text_only' (~500 tokens): Pure extracted text buffers (including off-screen text), zero UI chrome.\n"
                        "- 'interactive' (~300 tokens): Actionable controls only (buttons, inputs, links, tabs).\n"
                        "- 'compact' (~900 tokens): Pruned semantic YAML tree + 180-char text previews.\n"
                        "- 'full' (unbounded): Complete pruned YAML tree + 100% of unrendered & visible DocumentRange text inline."
                    ),
                    "default": "compact",
                },
            },
            "required": ["target", "verbosity"],
        },
    }
]


def handle_request(req: dict):
    method = req.get("method")
    req_id = req.get("id")

    if method == "initialize":
        return {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "a11y-ssh-observer", "version": "1.1.0"},
            },
        }

    if method == "tools/list":
        return {"jsonrpc": "2.0", "id": req_id, "result": {"tools": TOOLS}}

    if method == "tools/call":
        params = req.get("params", {})
        name = params.get("name")
        args = params.get("arguments", {})
        if name == "observe_desktop":
            target = args.get("target", "all")
            verbosity = args.get("verbosity", "compact")
            try:
                res = engine.format_for_llm(target_id=target, verbosity=verbosity)
                return {
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "result": {
                        "content": [{"type": "text", "text": res["llm_text"]}],
                        "isError": False,
                    },
                }
            except Exception as e:
                return {
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "result": {
                        "content": [{"type": "text", "text": f"Error: {e}"}],
                        "isError": True,
                    },
                }

    if req_id is not None:
        return {
            "jsonrpc": "2.0",
            "id": req_id,
            "error": {"code": -32601, "message": f"Method '{method}' not found"},
        }
    return None


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            resp = handle_request(req)
            if resp is not None:
                sys.stdout.write(json.dumps(resp) + "\n")
                sys.stdout.flush()
        except Exception as e:
            err = {
                "jsonrpc": "2.0",
                "id": None,
                "error": {"code": -32700, "message": str(e)},
            }
            sys.stdout.write(json.dumps(err) + "\n")
            sys.stdout.flush()


if __name__ == "__main__":
    main()
