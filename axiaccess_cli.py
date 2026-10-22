#!/usr/bin/env python3
"""
AxiAccess Remote CLI & SSH Command Entrypoint (`a11y_cli.py` / `axiaccess_cli.py`).
Allows an LLM agent or script to SSH into a Windows, macOS, or Linux machine
and immediately extract GUI state and off-screen text with a single command:

Examples over SSH:
  ssh user@host "python /path/to/axiaccess_cli.py"                        # L1 Glance of all windows
  ssh user@host "python /path/to/axiaccess_cli.py ChatGPT -v text_only"   # L4 Full off-screen text buffer
  ssh user@host "python /path/to/axiaccess_cli.py firefox -v interactive" # L2 Actionable buttons/links only
  ssh user@host "python /path/to/axiaccess_cli.py --json ChatGPT"         # Structured JSON output
"""

import argparse
import json
import sys
import urllib.parse
import urllib.request
from axiaccess_engine import get_universal_engine


def try_fast_local_daemon(target: str, verbosity: str, as_json: bool, port: int = 8765):
    """
    If `server.py` is already running on the host (127.0.0.1:8765), query it first
    for instant response; otherwise fall back to direct in-process OS extraction.
    """
    fmt = "json" if as_json else "text"
    url = (
        f"http://127.0.0.1:{port}/api/llm?"
        f"target={urllib.parse.quote(target)}&verbosity={urllib.parse.quote(verbosity)}&format={fmt}"
    )
    try:
        req = urllib.request.Request(url, headers={"Accept": "application/json, text/plain"})
        with urllib.request.urlopen(req, timeout=2.5) as resp:
            return resp.read().decode("utf-8", errors="replace")
    except Exception:
        return None


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser(
        description="AxiAccess: One-shot CLI for LLM agents over SSH."
    )
    parser.add_argument(
        "target",
        nargs="?",
        default="all",
        help="Window title substring, process name, PID, HWND, or 'all' (default: 'all')",
    )
    parser.add_argument(
        "-v",
        "--verbosity",
        choices=["glance", "interactive", "compact", "text_only", "full"],
        default=None,
        help="LLM verbosity tier (default: 'glance' when target='all', else 'compact')",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Output structured JSON with token estimates instead of raw Markdown/YAML",
    )
    parser.add_argument(
        "--direct",
        action="store_true",
        help="Bypass running daemon and query OS accessibility APIs directly in-process",
    )

    args = parser.parse_args()
    verbosity = args.verbosity or ("glance" if args.target.lower() == "all" else "compact")

    if not args.direct:
        cached = try_fast_local_daemon(args.target, verbosity, args.json)
        if cached is not None:
            print(cached)
            return

    eng = get_universal_engine()
    res = eng.format_for_llm(target_id=args.target, verbosity=verbosity)
    if args.json:
        print(json.dumps(res, indent=2, ensure_ascii=False))
    else:
        print(res["llm_text"])


if __name__ == "__main__":
    main()
