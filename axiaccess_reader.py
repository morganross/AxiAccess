#!/usr/bin/env python3
"""
AxiAccess Reader-Converter (`axiaccess_reader.py`)
=================================================
Connects to an AxiAccess sender (over SSH, HTTP, or locally) and converts raw
accessibility trees & text buffers into token-budgeted, semantic representations
engineered specifically for Coding Agent LLMs (Claude Code, Cursor, Devin, Aider, etc.).

Connection Modes:
  1. SSH Direct:        python axiaccess_reader.py --ssh user@remote-host
  2. SSH Port-Forward:  python axiaccess_reader.py --url http://127.0.0.1:8765
  3. Local:             python axiaccess_reader.py (queries local host directly)

Output Formats:
  - `xml` (default):    Clean XML tags (<active_editor>, <terminal>, <errors_detected>)
  - `markdown`:         Human-readable Markdown with codeblocks & status lists
  - `diff`:             Only semantic changes since the agent's last check (ultra low-token)
  - `json`:             Structured JSON for programmatic agent loops
"""

import argparse
import hashlib
import json
import os
import re
import shlex
import subprocess
import sys
import time
import urllib.parse
import urllib.request
from typing import Any, Dict, List, Optional, Tuple

CACHE_FILE = os.path.expanduser("~/.axiaccess_state_cache.json")

# Code file extensions recognized in window titles and editor buffers
CODE_EXTENSIONS = (
    ".rs", ".py", ".js", ".ts", ".tsx", ".jsx", ".go", ".c", ".cpp", ".cc", ".h", ".hpp",
    ".java", ".cs", ".rb", ".php", ".swift", ".kt", ".scala", ".html", ".css", ".scss",
    ".json", ".toml", ".yaml", ".yml", ".md", ".sh", ".bash", ".zsh", ".ps1", ".sql",
    ".dockerfile", "dockerfile", ".vue", ".svelte", ".zig", ".lua", ".r", ".dart"
)

# Language syntax hint mapping
LANG_MAP = {
    ".rs": "rust", ".py": "python", ".js": "javascript", ".ts": "typescript",
    ".tsx": "tsx", ".jsx": "jsx", ".go": "go", ".c": "c", ".cpp": "cpp",
    ".java": "java", ".cs": "csharp", ".rb": "ruby", ".html": "html",
    ".css": "css", ".json": "json", ".toml": "toml", ".yaml": "yaml",
    ".yml": "yaml", ".md": "markdown", ".sh": "bash", ".ps1": "powershell",
    ".sql": "sql", ".zig": "zig", ".lua": "lua", ".dart": "dart"
}

# Processes recognized as IDEs or text editors
EDITOR_PROCS = {
    "code", "code.exe", "cursor", "cursor.exe", "idea", "idea64.exe",
    "pycharm", "pycharm64.exe", "rustrover", "rustrover64.exe", "clion", "clion64.exe",
    "webstorm", "webstorm64.exe", "devenv.exe", "sublime_text", "sublime_text.exe",
    "notepad++.exe", "notepad.exe", "gedit", "kate", "textedit", "antigravity.exe",
    "windsurf", "windsurf.exe", "zed", "zed.exe", "emacs", "nvim", "vim"
}

# Processes recognized as terminals or shells
TERMINAL_PROCS = {
    "windowsterminal.exe", "powershell.exe", "pwsh.exe", "cmd.exe", "conhost.exe",
    "bash", "zsh", "alacritty", "alacritty.exe", "wezterm", "wezterm-gui.exe",
    "kitty", "gnome-terminal", "xterm", "konsole", "iterm2", "terminal"
}

# Processes recognized as web browsers (for web previews / localhost testing)
BROWSER_PROCS = {
    "chrome.exe", "chrome", "firefox.exe", "firefox", "msedge.exe", "msedge",
    "brave.exe", "brave", "safari", "chromium.exe", "chromium", "opera.exe"
}


# ============================================================================
# REMOTE SSH & HTTP FETCHING LAYER
# ============================================================================

class RemoteAxiAccessClient:
    """Handles communication with the AxiAccess host over SSH, HTTP, or locally."""

    def __init__(
        self,
        ssh_host: Optional[str] = None,
        ssh_port: Optional[int] = None,
        ssh_key: Optional[str] = None,
        base_url: Optional[str] = None,
        remote_cmd: Optional[str] = None,
    ):
        self.ssh_host = ssh_host
        self.ssh_port = ssh_port
        self.ssh_key = ssh_key
        self.base_url = base_url.rstrip("/") if base_url else None
        self.remote_cmd = remote_cmd

    def fetch_desktop_snapshot(self, target: str = "all", verbosity: str = "full") -> Dict[str, Any]:
        """Fetches raw desktop inspection data from remote SSH or HTTP daemon."""
        if self.base_url:
            return self._fetch_via_http(target, verbosity)
        if self.ssh_host:
            return self._fetch_via_ssh(target, verbosity)
        return self._fetch_locally(target, verbosity)

    def _fetch_via_http(self, target: str, verbosity: str) -> Dict[str, Any]:
        url = f"{self.base_url}/api/llm?target={urllib.parse.quote(target)}&verbosity={urllib.parse.quote(verbosity)}&format=json"
        req = urllib.request.Request(url, headers={"Accept": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=12.0) as resp:
                return json.loads(resp.read().decode("utf-8", errors="replace"))
        except Exception as e:
            raise ConnectionError(f"Failed to connect to AxiAccess HTTP endpoint at {url}: {e}")

    def _fetch_via_ssh(self, target: str, verbosity: str) -> Dict[str, Any]:
        """Executes the AxiAccess binary or Python CLI on the remote host over standard SSH."""
        # Candidate binary commands on remote machine
        cmd_candidates = []
        if self.remote_cmd:
            cmd_candidates.append(f"{self.remote_cmd} query {shlex.quote(target)} -v {shlex.quote(verbosity)} --json")
        else:
            cmd_candidates.append(f"axiaccess query {shlex.quote(target)} -v {shlex.quote(verbosity)} --json")
            cmd_candidates.append(f"./axiaccess query {shlex.quote(target)} -v {shlex.quote(verbosity)} --json")
            cmd_candidates.append(f"python3 axiaccess_cli.py {shlex.quote(target)} -v {shlex.quote(verbosity)} --json")
            cmd_candidates.append(f"python axiaccess_cli.py {shlex.quote(target)} -v {shlex.quote(verbosity)} --json")

        combined_remote_cmd = " || ".join(cmd_candidates)

        ssh_args = ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10"]
        if self.ssh_port:
            ssh_args.extend(["-p", str(self.ssh_port)])
        if self.ssh_key:
            ssh_args.extend(["-i", self.ssh_key])
        ssh_args.append(self.ssh_host)
        ssh_args.append(combined_remote_cmd)

        try:
            proc = subprocess.run(
                ssh_args,
                capture_output=True,
                text=True,
                timeout=20,
            )
        except subprocess.TimeoutExpired:
            raise TimeoutError(f"SSH connection to {self.ssh_host} timed out after 20 seconds.")
        except Exception as e:
            raise ConnectionError(f"Failed to spawn SSH process: {e}")

        if proc.returncode != 0 and not proc.stdout.strip():
            err_msg = proc.stderr.strip() or f"SSH exited with code {proc.returncode}"
            raise ConnectionError(f"SSH execution on {self.ssh_host} failed: {err_msg}")

        # Extract JSON from output (in case remote shell printed login banners or MOTD)
        stdout_text = proc.stdout.strip()
        json_start = stdout_text.find("{")
        json_end = stdout_text.rfind("}")
        if json_start != -1 and json_end != -1 and json_end > json_start:
            clean_json_str = stdout_text[json_start : json_end + 1]
            try:
                return json.loads(clean_json_str)
            except json.JSONDecodeError as e:
                raise ValueError(f"Failed to parse JSON from remote SSH output: {e}\nRaw output: {stdout_text[:400]}")
        raise ValueError(f"No JSON object detected in SSH output: {stdout_text[:400]}")

    def _fetch_locally(self, target: str, verbosity: str) -> Dict[str, Any]:
        """Tries local running daemon first, then falls back to in-process engine."""
        # 1. Try local daemon port 8765
        try:
            url = f"http://127.0.0.1:8765/api/llm?target={urllib.parse.quote(target)}&verbosity={urllib.parse.quote(verbosity)}&format=json"
            req = urllib.request.Request(url, headers={"Accept": "application/json"})
            with urllib.request.urlopen(req, timeout=1.5) as resp:
                return json.loads(resp.read().decode("utf-8", errors="replace"))
        except Exception:
            pass

        # 2. In-process engine fallback
        try:
            from axiaccess_engine import get_universal_engine
            engine = get_universal_engine()
            return engine.format_for_llm(target_id=target, verbosity=verbosity)
        except Exception as e:
            raise RuntimeError(f"Local AxiAccess engine failed: {e}")


# ============================================================================
# SEMANTIC CODING AGENT SYNTHESIZER
# ============================================================================

class CodingAgentSynthesizer:
    """
    Transforms raw window trees, text buffers, and control hierarchies into
    dense, high-signal coding context blocks.
    """

    def __init__(self, raw_data: Dict[str, Any], host_label: str = "remote-host"):
        self.raw_data = raw_data
        self.host_label = host_label

    def synthesize(self) -> Dict[str, Any]:
        target = self.raw_data.get("target", "all")
        verbosity = self.raw_data.get("verbosity", "compact")
        llm_text = self.raw_data.get("llm_text", "")

        windows_meta = self._parse_glance_windows(llm_text)
        focused_window = next((w for w in windows_meta if w.get("focused")), None)

        active_editor = self._extract_active_editor(llm_text, windows_meta)
        terminal_state = self._extract_terminal_state(llm_text, windows_meta)
        browser_preview = self._extract_browser_preview(llm_text, windows_meta)
        dialogs = self._extract_dialogs(llm_text, windows_meta)
        errors = self._extract_compiler_errors(terminal_state, active_editor, llm_text)

        return {
            "host": self.host_label,
            "timestamp": time.strftime("%H:%M:%S"),
            "target": target,
            "verbosity": verbosity,
            "focused_window": focused_window,
            "active_editor": active_editor,
            "terminal": terminal_state,
            "browser_preview": browser_preview,
            "dialogs": dialogs,
            "errors_detected": errors,
            "open_windows_count": len(windows_meta),
            "open_windows": windows_meta,
        }

    def _parse_glance_windows(self, text: str) -> List[Dict[str, Any]]:
        """Parses lines like `- [FOCUSED] hwnd:123 pid:456 (code.exe) "main.rs - AxiAccess"`"""
        windows = []
        pattern = re.compile(r"-\s*\[(FOCUSED|VISIBLE|MINIMIZED|OFFSCREEN)\]\s*hwnd:(\d+)\s*pid:(\d+)\s*\(([^)]+)\)\s*\"([^\"]*)\"")
        for line in text.splitlines():
            m = pattern.search(line)
            if m:
                state, hwnd, pid, proc, title = m.groups()
                windows.append({
                    "state": state,
                    "focused": state == "FOCUSED",
                    "hwnd": int(hwnd),
                    "pid": int(pid),
                    "process": proc.strip(),
                    "title": title.strip(),
                    "is_editor": proc.lower() in EDITOR_PROCS or any(title.lower().endswith(ext) for ext in CODE_EXTENSIONS),
                    "is_terminal": proc.lower() in TERMINAL_PROCS,
                    "is_browser": proc.lower() in BROWSER_PROCS,
                })
        return windows

    def _extract_active_editor(self, text: str, windows: List[Dict[str, Any]]) -> Optional[Dict[str, Any]]:
        """Identifies code editors and extracts open file, language, and buffer lines."""
        editor_win = next((w for w in windows if w["focused"] and w["is_editor"]), None)
        if not editor_win:
            editor_win = next((w for w in windows if w["is_editor"] and w["state"] != "MINIMIZED"), None)

        if not editor_win:
            return None

        title = editor_win["title"]
        file_name = "unknown"
        detected_lang = "text"
        is_dirty = any(mark in title for mark in ("*", "●", "[Modified]"))

        # Try to find file extension in title
        for part in title.split():
            clean_part = part.strip().strip("-|•*●[]()")
            for ext in CODE_EXTENSIONS:
                if clean_part.lower().endswith(ext):
                    file_name = clean_part
                    detected_lang = LANG_MAP.get(ext, ext.lstrip("."))
                    break
            if file_name != "unknown":
                break

        # Extract code buffer snippet if present in llm_text
        code_lines = []
        in_buffer_section = False
        for line in text.splitlines():
            if "FULL UNRENDERED" in line or "Extracted Text Only" in line or "Buffer @" in line:
                in_buffer_section = True
                continue
            if in_buffer_section:
                if line.startswith("```"):
                    continue
                code_lines.append(line)

        code_snippet = "\n".join(code_lines).strip()
        if not code_snippet:
            # Fallback preview from tree text
            preview_matches = re.findall(r"text:\s*\"([^\"]+)\"", text)
            if preview_matches:
                code_snippet = "\n".join(preview_matches[:8])

        total_lines = code_snippet.count("\n") + (1 if code_snippet else 0)

        return {
            "app": editor_win["process"],
            "window_title": title,
            "file": file_name,
            "language": detected_lang,
            "unsaved_changes": is_dirty,
            "total_lines": total_lines,
            "code_buffer": code_snippet,
        }

    def _extract_terminal_state(self, text: str, windows: List[Dict[str, Any]]) -> Optional[Dict[str, Any]]:
        """Identifies terminal emulators and extracts last command and output buffer."""
        term_win = next((w for w in windows if w["is_terminal"]), None)
        if not term_win:
            # Check if there is an integrated terminal node in the tree
            if "terminal" in text.lower():
                term_win = {"process": "integrated-terminal", "title": "Integrated Terminal"}
            else:
                return None

        # Look for shell prompt markers
        lines = text.splitlines()
        last_command = ""
        captured_lines = []
        for line in lines:
            trimmed = line.strip()
            # Detect PowerShell or Bash prompts
            if re.search(r"(PS\s+[A-Za-z]:\\[^>]*>|[$#❯]\s+)", trimmed):
                last_command = trimmed
            captured_lines.append(trimmed)

        term_output = "\n".join(captured_lines[-40:]).strip() if captured_lines else ""

        return {
            "app": term_win["process"],
            "title": term_win.get("title", "Terminal"),
            "last_command": last_command or "(interactive shell)",
            "output_buffer": term_output,
        }

    def _extract_browser_preview(self, text: str, windows: List[Dict[str, Any]]) -> Optional[Dict[str, Any]]:
        """Identifies browser windows for localhost / web app testing."""
        browser_win = next((w for w in windows if w["is_browser"] and w["state"] != "MINIMIZED"), None)
        if not browser_win:
            return None

        title = browser_win["title"]
        url_match = re.search(r"(https?://[^\s\"']+)", text)
        url = url_match.group(1) if url_match else None

        is_localhost = bool(url and ("localhost" in url or "127.0.0.1" in url or "::1" in url))

        return {
            "app": browser_win["process"],
            "title": title,
            "url": url,
            "is_localhost_preview": is_localhost,
        }

    def _extract_dialogs(self, text: str, windows: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
        """Detects modal dialogs or alert popups that might block an agent."""
        dialogs = []
        for w in windows:
            title_lower = w["title"].lower()
            if any(k in title_lower for k in ("confirm", "warning", "error", "dialog", "save changes", "alert")):
                dialogs.append({
                    "title": w["title"],
                    "app": w["process"],
                    "state": w["state"],
                })
        return dialogs

    def _extract_compiler_errors(
        self,
        terminal: Optional[Dict[str, Any]],
        editor: Optional[Dict[str, Any]],
        full_text: str
    ) -> List[str]:
        """Detects compiler errors, test failures, and stack traces."""
        errors = []
        haystack = ""
        if terminal and terminal.get("output_buffer"):
            haystack += terminal["output_buffer"] + "\n"
        if editor and editor.get("code_buffer"):
            haystack += editor["code_buffer"] + "\n"
        haystack += full_text

        error_patterns = [
            r"error\[E\d+\]:[^\n]+",                    # Rust compiler errors
            r"Traceback \(most recent call last\):[\s\S]{1,300}?(?=\n\S|$)", # Python tracebacks
            r"[A-Za-z]+Error:[^\n]+",                   # Python/JS Error types
            r"FAILED\s+tests/[^\n]+",                   # pytest failures
            r"FAIL:[^\n]+",                             # test failures
            r"SyntaxError:[^\n]+",                      # Syntax errors
            r"cannot find symbol[^\n]+",                # Java errors
            r"fatal error:[^\n]+",                      # C/C++ errors
        ]

        for pat in error_patterns:
            matches = re.findall(pat, haystack)
            for m in matches:
                clean_m = m.strip()
                if clean_m and clean_m not in errors:
                    errors.append(clean_m)

        return errors[:8]


# ============================================================================
# STATE DIFF & CACHING ENGINE
# ============================================================================

class StateDiffEngine:
    """Maintains snapshot cache and computes delta for low-token agent loops."""

    @staticmethod
    def compute_diff(current_state: Dict[str, Any]) -> Tuple[Dict[str, Any], bool]:
        prev_state = StateDiffEngine.load_cache()
        StateDiffEngine.save_cache(current_state)

        if not prev_state:
            return {
                "type": "INITIAL_SNAPSHOT",
                "message": "First inspection recorded. No previous state to diff against.",
                "state": current_state,
            }, True

        deltas = {}
        # 1. Focus changes
        curr_focus = current_state.get("focused_window") or {}
        prev_focus = prev_state.get("focused_window") or {}
        if curr_focus.get("title") != prev_focus.get("title"):
            deltas["focus_change"] = {
                "from": prev_focus.get("title", "(none)"),
                "to": curr_focus.get("title", "(none)"),
                "app": curr_focus.get("process", "unknown"),
            }

        # 2. Editor changes
        curr_ed = current_state.get("active_editor") or {}
        prev_ed = prev_state.get("active_editor") or {}
        if curr_ed.get("file") != prev_ed.get("file"):
            deltas["editor_tab_switched"] = {
                "from": prev_ed.get("file", "(none)"),
                "to": curr_ed.get("file", "(none)"),
            }
        elif curr_ed.get("code_buffer") != prev_ed.get("code_buffer"):
            deltas["editor_content_modified"] = {
                "file": curr_ed.get("file"),
                "lines_before": prev_ed.get("total_lines", 0),
                "lines_after": curr_ed.get("total_lines", 0),
            }

        # 3. Terminal changes
        curr_term = current_state.get("terminal") or {}
        prev_term = prev_state.get("terminal") or {}
        if curr_term.get("output_buffer") != prev_term.get("output_buffer"):
            curr_lines = (curr_term.get("output_buffer") or "").splitlines()
            prev_lines = (prev_term.get("output_buffer") or "").splitlines()
            new_lines = [l for l in curr_lines if l not in prev_lines]
            deltas["terminal_appended"] = {
                "new_lines_count": len(new_lines),
                "snippet": "\n".join(new_lines[-15:]) if new_lines else "(buffer refreshed)",
            }

        # 4. New errors
        curr_errs = set(current_state.get("errors_detected") or [])
        prev_errs = set(prev_state.get("errors_detected") or [])
        new_errs = list(curr_errs - prev_errs)
        if new_errs:
            deltas["new_errors_detected"] = new_errs

        # 5. New dialogs
        curr_diag = [d["title"] for d in current_state.get("dialogs", [])]
        prev_diag = [d["title"] for d in prev_state.get("dialogs", [])]
        new_dialogs = [d for d in curr_diag if d not in prev_diag]
        if new_dialogs:
            deltas["new_dialog_popped_up"] = new_dialogs

        has_changes = bool(deltas)
        return {
            "type": "DELTA",
            "has_changes": has_changes,
            "host": current_state.get("host"),
            "timestamp": current_state.get("timestamp"),
            "deltas": deltas if has_changes else "No significant screen changes detected since last check.",
        }, has_changes

    @staticmethod
    def load_cache() -> Optional[Dict[str, Any]]:
        if os.path.exists(CACHE_FILE):
            try:
                with open(CACHE_FILE, "r", encoding="utf-8") as f:
                    return json.load(f)
            except Exception:
                pass
        return None

    @staticmethod
    def save_cache(state: Dict[str, Any]):
        try:
            with open(CACHE_FILE, "w", encoding="utf-8") as f:
                json.dump(state, f, indent=2, ensure_ascii=False)
        except Exception:
            pass


# ============================================================================
# FORMATTERS FOR CODING AGENT LLMS
# ============================================================================

def format_as_coding_xml(data: Dict[str, Any]) -> str:
    """Formats desktop state as clean, structured XML tags for coding agent prompts."""
    lines = [f'<remote_desktop host="{data.get("host", "remote")}" timestamp="{data.get("timestamp", "")}">']

    # Focus
    f = data.get("focused_window")
    if f:
        lines.append(f'  <active_focus app="{f["process"]}" hwnd="{f["hwnd"]}" pid="{f["pid"]}">{escape_xml(f["title"])}</active_focus>')
    else:
        lines.append('  <active_focus state="none" />')

    # Active Editor
    ed = data.get("active_editor")
    if ed:
        dirty_attr = ' unsaved_changes="true"' if ed.get("unsaved_changes") else ''
        lines.append(f'  <active_editor file="{ed["file"]}" language="{ed["language"]}" app="{ed["app"]}" total_lines="{ed["total_lines"]}"{dirty_attr}>')
        code = ed.get("code_buffer", "")
        if code:
            lines.append(f"<![CDATA[\n{code}\n]]>")
        else:
            lines.append("    <!-- (Empty or virtualized buffer) -->")
        lines.append('  </active_editor>')

    # Terminal
    term = data.get("terminal")
    if term:
        lines.append(f'  <terminal app="{term["app"]}">')
        if term.get("last_command"):
            lines.append(f'    <last_command>{escape_xml(term["last_command"])}</last_command>')
        out = term.get("output_buffer", "")
        if out:
            lines.append(f"    <output><![CDATA[\n{out}\n    ]]></output>")
        lines.append('  </terminal>')

    # Browser Preview
    br = data.get("browser_preview")
    if br:
        lines.append(f'  <browser_preview app="{br["app"]}" url="{br.get("url") or "unknown"}">')
        lines.append(f'    <page_title>{escape_xml(br["title"])}</page_title>')
        lines.append('  </browser_preview>')

    # Errors
    errs = data.get("errors_detected", [])
    if errs:
        lines.append('  <errors_detected>')
        for err in errs:
            lines.append(f'    <error>{escape_xml(err)}</error>')
        lines.append('  </errors_detected>')

    # Dialogs
    diags = data.get("dialogs", [])
    if diags:
        lines.append('  <active_dialogs>')
        for d in diags:
            lines.append(f'    <dialog app="{d["app"]}">{escape_xml(d["title"])}</dialog>')
        lines.append('  </active_dialogs>')

    # Other open windows summary
    other_wins = [w for w in data.get("open_windows", []) if not w.get("focused")]
    if other_wins:
        lines.append('  <background_windows>')
        for w in other_wins[:6]:
            lines.append(f'    <window app="{w["process"]}">{escape_xml(w["title"])}</window>')
        lines.append('  </background_windows>')

    lines.append('</remote_desktop>')
    return "\n".join(lines)


def format_as_markdown(data: Dict[str, Any]) -> str:
    """Formats desktop state as rich Markdown with fenced code blocks."""
    lines = [
        f"# Remote Desktop Context (`{data.get('host', 'remote')}` at {data.get('timestamp', '')})",
        "",
    ]
    f = data.get("focused_window")
    if f:
        lines.append(f"**Active Focus:** `{f['process']}` — *{f['title']}*")
        lines.append("")

    ed = data.get("active_editor")
    if ed:
        dirty_flag = " *(Unsaved Changes)*" if ed.get("unsaved_changes") else ""
        lines.append(f"## Active Editor: `{ed['file']}` ({ed['language']}){dirty_flag}")
        lines.append(f"```{ed['language']}")
        lines.append(ed.get("code_buffer", "") or "// (empty buffer)")
        lines.append("```")
        lines.append("")

    term = data.get("terminal")
    if term:
        lines.append(f"## Terminal: `{term['app']}`")
        if term.get("last_command"):
            lines.append(f"**Last Command:** `{term['last_command']}`")
        lines.append("```text")
        lines.append(term.get("output_buffer", ""))
        lines.append("```")
        lines.append("")

    errs = data.get("errors_detected", [])
    if errs:
        lines.append("## Detected Errors / Diagnostic Traces")
        for err in errs:
            lines.append(f"- ⚠️ `{err}`")
        lines.append("")

    br = data.get("browser_preview")
    if br:
        lines.append(f"## Web Preview: `{br['app']}`")
        lines.append(f"- Title: {br['title']}")
        if br.get("url"):
            lines.append(f"- URL: {br['url']}")
        lines.append("")

    return "\n".join(lines)


def format_as_diff_text(diff_data: Dict[str, Any]) -> str:
    """Formats state delta for minimal token consumption."""
    if not diff_data.get("has_changes", True):
        return f"# Desktop Delta ({diff_data.get('timestamp')}): No screen changes detected."

    lines = [f"# Desktop Delta ({diff_data.get('timestamp')} on {diff_data.get('host')})"]
    deltas = diff_data.get("deltas", {})
    if isinstance(deltas, dict):
        for key, val in deltas.items():
            lines.append(f"## {key.replace('_', ' ').title()}")
            if isinstance(val, dict):
                for k, v in val.items():
                    if k == "snippet":
                        lines.append(f"```\n{v}\n```")
                    else:
                        lines.append(f"- **{k}**: {v}")
            elif isinstance(val, list):
                for item in val:
                    lines.append(f"- {item}")
            else:
                lines.append(str(val))
    else:
        lines.append(str(deltas))

    return "\n".join(lines)


def escape_xml(s: Any) -> str:
    return str(s or "").replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace('"', "&quot;")


# ============================================================================
# CLI MAIN ENTRYPOINT
# ============================================================================

def main():
    sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser(
        description="AxiAccess Reader-Converter: Bridges remote OS accessibility into clean Coding Agent LLM prompts."
    )
    # Connection args
    g_conn = parser.add_argument_group("Remote Connection Options")
    g_conn.add_argument("--ssh", metavar="[USER@]HOST", help="Remote SSH host to connect to (e.g. dev-box or user@192.168.1.50)")
    g_conn.add_argument("-p", "--port", type=int, help="SSH port (if non-standard 22)")
    g_conn.add_argument("-i", "--identity", help="Path to SSH private key file")
    g_conn.add_argument("--url", help="HTTP URL of remote AxiAccess daemon (e.g. http://127.0.0.1:8765)")
    g_conn.add_argument("--remote-cmd", help="Custom command or path to run axiaccess on remote host")

    # Scope & Format args
    parser.add_argument("target", nargs="?", default="all", help="Target window (title, process, PID, or 'all')")
    parser.add_argument("-f", "--format", choices=["xml", "markdown", "diff", "json", "raw"], default="xml", help="Output format for coding agent (default: xml)")
    parser.add_argument("-v", "--verbosity", choices=["glance", "interactive", "compact", "text_only", "full"], default="full", help="Raw fetch verbosity level (default: full)")
    parser.add_argument("--diff", action="store_true", help="Compare with previous snapshot and return only what changed")

    args = parser.parse_args()

    host_label = args.ssh or (args.url or "localhost")
    client = RemoteAxiAccessClient(
        ssh_host=args.ssh,
        ssh_port=args.port,
        ssh_key=args.identity,
        base_url=args.url,
        remote_cmd=args.remote_cmd,
    )

    try:
        raw_snapshot = client.fetch_desktop_snapshot(target=args.target, verbosity=args.verbosity)
    except Exception as e:
        sys.stderr.write(f"Error fetching from AxiAccess ({host_label}): {e}\n")
        sys.exit(1)

    if args.format == "raw":
        print(json.dumps(raw_snapshot, indent=2, ensure_ascii=False))
        return

    synthesizer = CodingAgentSynthesizer(raw_snapshot, host_label=host_label)
    semantic_state = synthesizer.synthesize()

    # Handle diff mode
    if args.diff or args.format == "diff":
        diff_res, has_changes = StateDiffEngine.compute_diff(semantic_state)
        if args.format == "json":
            print(json.dumps(diff_res, indent=2, ensure_ascii=False))
        else:
            print(format_as_diff_text(diff_res))
        return

    # Update state cache silently
    StateDiffEngine.save_cache(semantic_state)

    if args.format == "xml":
        print(format_as_coding_xml(semantic_state))
    elif args.format == "markdown":
        print(format_as_markdown(semantic_state))
    elif args.format == "json":
        print(json.dumps(semantic_state, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
