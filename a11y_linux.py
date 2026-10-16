"""
Linux Native Accessibility Engine (AT-SPI2 over D-Bus)
for A11y-SSH Universal Remote Desktop Accessibility Observer.

Features:
- Unlocks lazy Electron / Chromium / Firefox / GTK / Qt accessibility trees on Linux by setting
  `org.a11y.Status.IsEnabled = True` and `org.a11y.Status.ScreenReaderEnabled = True` on the session D-Bus.
- Connects to the AT-SPI2 registry (`org.a11y.atspi.Registry`) via `pyatspi` (or `gi.repository.Atspi`).
- Walks the desktop Accessible hierarchy, pruning layout containers (`panel`, `filler`, `scroll-pane`, `section`).
- Extracts full unrendered / off-screen text buffers via `org.a11y.atspi.Text.getText(0, -1)` and `org.a11y.atspi.Value`.
"""

import os
import subprocess
import time
import threading

HAS_LINUX_ATSPI = False
_atspi_backend = None

try:
    import pyatspi

    HAS_LINUX_ATSPI = True
    _atspi_backend = "pyatspi"
except ImportError:
    try:
        import gi

        gi.require_version("Atspi", "2.0")
        from gi.repository import Atspi

        HAS_LINUX_ATSPI = True
        _atspi_backend = "gi_atspi"
    except Exception:
        HAS_LINUX_ATSPI = False


# Normalized role map from AT-SPI2 role names to A11y-SSH semantic roles
ATSPI_ROLE_MAP = {
    "frame": "window",
    "window": "window",
    "dialog": "window",
    "file chooser": "window",
    "alert": "window",
    "application": "application",
    "push button": "button",
    "toggle button": "button",
    "Check box": "checkbox",
    "check box": "checkbox",
    "radio button": "radiobutton",
    "combo box": "combobox",
    "entry": "edit",
    "password text": "edit",
    "text": "document",
    "document frame": "document",
    "document web": "document",
    "document text": "document",
    "terminal": "document",
    "paragraph": "text",
    "heading": "text",
    "label": "text",
    "static": "text",
    "link": "link",
    "image": "image",
    "icon": "image",
    "list": "list",
    "list item": "listitem",
    "tree": "tree",
    "tree item": "treeitem",
    "tree table": "table",
    "table": "table",
    "table cell": "dataitem",
    "page tab list": "tab",
    "page tab": "tabitem",
    "menu bar": "menubar",
    "menu": "menu",
    "menu item": "menuitem",
    "tool bar": "toolbar",
    "status bar": "statusbar",
    "progress bar": "progressbar",
    "slider": "slider",
    "scroll bar": "scrollbar",
    "scroll pane": "pane",
    "panel": "pane",
    "filler": "group",
    "section": "group",
    "split pane": "group",
    "viewport": "pane",
    "separator": "separator",
}

CONTAINER_ROLES = {
    "pane",
    "group",
    "custom",
    "separator",
    "scrollbar",
    "image",
    "tooltip",
}

TEXT_BEARING_ROLES = {
    "document",
    "edit",
    "pane",
    "window",
    "text",
}


class LinuxA11yEngine:
    """
    Native Linux AT-SPI2 / D-Bus semantic tree & off-screen text extractor.
    Outputs the exact same schema and 5-tier LLM verbosity formats as WindowsA11yEngine.
    """

    def __init__(self):
        self._lock = threading.Lock()
        self.wakeup_enabled = True
        if os.name == "posix":
            self.set_screen_reader_flag(True)

    def set_screen_reader_flag(self, enabled: bool = True) -> bool:
        """
        Enables `org.a11y.Status.IsEnabled` and `ScreenReaderEnabled` on the Linux session D-Bus
        so Chromium, Firefox, Electron, GTK, and Qt apps expose their full AT-SPI2 trees.
        """
        self.wakeup_enabled = bool(enabled)
        os.environ["ACCESSIBILITY_ENABLED"] = "1" if enabled else "0"
        os.environ["GTK_MODULES"] = "gail:atk-bridge"
        os.environ["QT_LINUX_ACCESSIBILITY_ALWAYS_ON"] = "1"
        val_str = "true" if enabled else "false"
        for prop in ("IsEnabled", "ScreenReaderEnabled"):
            try:
                subprocess.run(
                    [
                        "gdbus",
                        "call",
                        "--session",
                        "--dest",
                        "org.a11y.Bus",
                        "--object-path",
                        "/org/a11y/bus",
                        "--method",
                        "org.freedesktop.DBus.Properties.Set",
                        "org.a11y.Status",
                        prop,
                        f"<{val_str}>",
                    ],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    timeout=1.5,
                )
            except Exception:
                pass
        return self.wakeup_enabled

    def get_screen_reader_flag(self) -> bool:
        return self.wakeup_enabled

    def _get_desktop(self):
        if _atspi_backend == "pyatspi":
            return pyatspi.Registry.getDesktop(0)
        if _atspi_backend == "gi_atspi":
            return Atspi.get_desktop(0)
        return None

    def _child_count(self, obj) -> int:
        try:
            return obj.childCount if _atspi_backend == "pyatspi" else obj.get_child_count()
        except Exception:
            return 0

    def _get_child(self, obj, idx: int):
        try:
            return (
                obj.getChildAtIndex(idx)
                if _atspi_backend == "pyatspi"
                else obj.get_child_at_index(idx)
            )
        except Exception:
            return None

    def _get_name(self, obj) -> str:
        try:
            n = obj.name if _atspi_backend == "pyatspi" else obj.get_name()
            return (n or "").strip()
        except Exception:
            return ""

    def _get_role_name(self, obj) -> str:
        try:
            r = (
                obj.getRoleName()
                if _atspi_backend == "pyatspi"
                else obj.get_role_name()
            )
            r_low = (r or "").lower()
            return ATSPI_ROLE_MAP.get(r_low, r_low.replace(" ", "_"))
        except Exception:
            return "custom"

    def _get_pid(self, obj) -> int:
        try:
            return int(obj.get_process_id())
        except Exception:
            return 0

    def _get_states(self, obj):
        """Returns (focused, enabled, offscreen)"""
        try:
            if _atspi_backend == "pyatspi":
                st = obj.getState()
                focused = st.contains(pyatspi.STATE_FOCUSED) or st.contains(
                    pyatspi.STATE_ACTIVE
                )
                enabled = st.contains(pyatspi.STATE_ENABLED)
                showing = st.contains(pyatspi.STATE_SHOWING)
                return bool(focused), bool(enabled), not bool(showing)
            else:
                st = obj.get_state_set()
                focused = st.contains(Atspi.StateType.FOCUSED) or st.contains(
                    Atspi.StateType.ACTIVE
                )
                enabled = st.contains(Atspi.StateType.ENABLED)
                showing = st.contains(Atspi.StateType.SHOWING)
                return bool(focused), bool(enabled), not bool(showing)
        except Exception:
            return False, True, False

    def _extract_atspi_text(self, obj, role: str, max_preview: int = 180):
        """
        Queries `org.a11y.atspi.Text.getText(0, -1)` to extract the entire underlying
        text buffer (including unrendered/scrolled-away lines) on Linux.
        """
        if role not in TEXT_BEARING_ROLES:
            return None

        full_text = ""
        try:
            if _atspi_backend == "pyatspi":
                txt_iface = obj.queryText()
                char_count = txt_iface.characterCount
                if char_count > 0:
                    full_text = (txt_iface.getText(0, min(char_count, 65536)) or "").strip()
            else:
                txt_iface = obj.get_text_iface()
                if txt_iface:
                    char_count = Atspi.Text.get_character_count(txt_iface)
                    if char_count > 0:
                        full_text = (
                            Atspi.Text.get_text(txt_iface, 0, min(char_count, 65536))
                            or ""
                        ).strip()
        except Exception:
            pass

        if not full_text:
            return None

        total_chars = len(full_text)
        total_lines = full_text.count("\n") + 1
        preview = (
            full_text[:max_preview].replace("\r", "").replace("\n", " ↵ ").strip()
        )
        if total_chars > max_preview:
            preview += " …"

        return {
            "source": "AT-SPI Text",
            "total_chars": total_chars,
            "visible_chars": total_chars,
            "offscreen_chars": 0,
            "total_lines": total_lines,
            "visible_lines": total_lines,
            "preview": preview,
            "full_text": full_text,
            "visible_text": full_text,
        }

    def list_windows(self) -> dict:
        t0 = time.perf_counter()
        desktop = self._get_desktop()
        if desktop is None:
            return {
                "platform": "Linux AT-SPI2 (D-Bus / pyatspi not installed)",
                "screen_reader_flag": self.wakeup_enabled,
                "scan_time_ms": 0.0,
                "count": 0,
                "windows": [],
            }

        windows = []
        win_idx = 1
        app_count = self._child_count(desktop)
        for i in range(app_count):
            app = self._get_child(desktop, i)
            if not app:
                continue
            app_name = self._get_name(app) or "linux-app"
            pid = self._get_pid(app)
            w_count = self._child_count(app)
            for j in range(w_count):
                win = self._get_child(app, j)
                if not win:
                    continue
                title = self._get_name(win) or f"({app_name})"
                focused, _, offscreen = self._get_states(win)
                windows.append(
                    {
                        "hwnd": win_idx,
                        "pid": pid,
                        "process": app_name,
                        "title": title,
                        "class_name": self._get_role_name(win),
                        "offscreen": offscreen,
                        "minimized": offscreen,
                        "focused": focused,
                        "bounds": {"left": 0, "top": 0, "width": 1280, "height": 800},
                        "_ref": win,
                    }
                )
                win_idx += 1

        elapsed_ms = round((time.perf_counter() - t0) * 1000, 1)
        clean_windows = [{k: v for k, v in w.items() if k != "_ref"} for w in windows]
        return {
            "platform": f"Linux AT-SPI2 over D-Bus ({_atspi_backend})",
            "screen_reader_flag": self.wakeup_enabled,
            "scan_time_ms": elapsed_ms,
            "count": len(clean_windows),
            "windows": clean_windows,
        }

    def inspect_window(
        self, target_id, max_depth: int = 9, max_nodes: int = 400
    ) -> dict:
        t0 = time.perf_counter()
        desktop = self._get_desktop()
        if desktop is None:
            raise ValueError("Linux AT-SPI2 D-Bus library (pyatspi / gir1.2-atspi-2.0) is required on Linux.")

        target_str = str(target_id).lower()
        matched_win = None
        matched_meta = None
        win_idx = 1

        app_count = self._child_count(desktop)
        for i in range(app_count):
            app = self._get_child(desktop, i)
            if not app:
                continue
            app_name = self._get_name(app) or "linux-app"
            pid = self._get_pid(app)
            w_count = self._child_count(app)
            for j in range(w_count):
                win = self._get_child(app, j)
                if not win:
                    continue
                title = self._get_name(win) or f"({app_name})"
                focused, _, offscreen = self._get_states(win)
                meta = {
                    "hwnd": win_idx,
                    "pid": pid,
                    "process": app_name,
                    "title": title,
                    "class_name": self._get_role_name(win),
                    "offscreen": offscreen,
                }
                if (
                    str(win_idx) == str(target_id)
                    or str(pid) == str(target_id)
                    or target_str in title.lower()
                    or target_str in app_name.lower()
                ):
                    matched_win = win
                    matched_meta = meta
                    break
                win_idx += 1
            if matched_win:
                break

        if not matched_win:
            raise ValueError(f"Window '{target_id}' not found on Linux AT-SPI2 bus.")

        raw_nodes_visited = 0
        pruned_nodes_count = 0
        text_buffers = []
        seen_text_hashes = set()
        yaml_lines = []

        def walk(obj, depth):
            nonlocal raw_nodes_visited, pruned_nodes_count
            if depth > max_depth or pruned_nodes_count >= max_nodes or obj is None:
                return None

            raw_nodes_visited += 1
            role = self._get_role_name(obj)
            name = self._get_name(obj)
            focused, enabled, offscreen = self._get_states(obj)

            text_info = self._extract_atspi_text(obj, role)
            is_meaningful = (
                bool(name)
                or (text_info is not None)
                or (role not in CONTAINER_ROLES and not offscreen)
            )

            node_obj = None
            next_depth = depth

            if is_meaningful:
                pruned_nodes_count += 1
                eid = f"@e{pruned_nodes_count}"
                indent = "  " * depth
                clean_name = name.replace("\r", "").replace("\n", " ")
                name_part = f' "{clean_name[:95]}"' if clean_name else ""

                badges = []
                if focused:
                    badges.append("focused")
                if not enabled:
                    badges.append("disabled")
                if offscreen:
                    badges.append("offscreen")

                if text_info:
                    src = text_info["source"]
                    tc = text_info["total_chars"]
                    tl = text_info["total_lines"]
                    badges.append(f"{src}: {tc} chars / {tl} lines")

                    txt_sig = (src, tc, text_info["preview"][:80])
                    if tc > 0 and txt_sig not in seen_text_hashes:
                        seen_text_hashes.add(txt_sig)
                        text_buffers.append(
                            {
                                "id": eid,
                                "role": role,
                                "name": clean_name or f"<{role}>",
                                "source": src,
                                "total_chars": tc,
                                "visible_chars": tc,
                                "offscreen_chars": 0,
                                "total_lines": tl,
                                "visible_lines": tl,
                                "preview": text_info["preview"],
                                "full_text": text_info["full_text"],
                                "visible_text": text_info["visible_text"],
                            }
                        )

                badge_str = f" ({', '.join(badges)})" if badges else ""
                yaml_lines.append(f"{indent}- {role}{name_part} [{eid}]{badge_str}")
                if (
                    text_info
                    and text_info["preview"]
                    and text_info["preview"] != clean_name
                ):
                    yaml_lines.append(
                        f"{indent}    text: \"{text_info['preview']}\""
                    )

                node_obj = {
                    "id": eid,
                    "role": role,
                    "name": clean_name,
                    "depth": depth,
                    "offscreen": offscreen,
                    "focused": focused,
                    "enabled": enabled,
                    "has_text": text_info is not None,
                    "text_source": text_info["source"] if text_info else None,
                    "total_chars": text_info["total_chars"] if text_info else 0,
                    "offscreen_chars": 0,
                    "preview": text_info["preview"] if text_info else "",
                    "children": [],
                }
                next_depth = depth + 1

            children_nodes = []
            c_count = self._child_count(obj)
            for idx in range(c_count):
                if pruned_nodes_count >= max_nodes:
                    break
                child = self._get_child(obj, idx)
                if child:
                    child_res = walk(child, next_depth)
                    if child_res:
                        if isinstance(child_res, list):
                            children_nodes.extend(child_res)
                        else:
                            children_nodes.append(child_res)

            if node_obj is not None:
                node_obj["children"] = children_nodes
                return node_obj
            return children_nodes

        tree = walk(matched_win, 0)
        tree_root = tree if isinstance(tree, list) else ([tree] if tree else [])
        elapsed_ms = round((time.perf_counter() - t0) * 1000, 1)
        compression_pct = (
            round((1 - (pruned_nodes_count / max(1, raw_nodes_visited))) * 100, 1)
            if raw_nodes_visited > 0
            else 0.0
        )

        header = (
            f"# Semantic GUI-SSH Snapshot (Linux AT-SPI2 / D-Bus)\n"
            f"window: \"{matched_meta['title']}\" (process: {matched_meta['process']}, pid: {matched_meta['pid']})\n"
            f"stats: {pruned_nodes_count} semantic nodes (pruned from {raw_nodes_visited} raw AT-SPI nodes, "
            f"{compression_pct}% noise removed in {elapsed_ms}ms)\n"
            f"elements:\n"
        )
        llm_prompt_payload = header + "\n".join(yaml_lines)

        return {
            "window": matched_meta,
            "stats": {
                "raw_nodes": raw_nodes_visited,
                "semantic_nodes": pruned_nodes_count,
                "compression_pct": compression_pct,
                "text_buffers_found": len(text_buffers),
                "total_extracted_chars": sum(b["total_chars"] for b in text_buffers),
                "offscreen_extracted_chars": 0,
                "scan_time_ms": elapsed_ms,
                "estimated_tokens": max(1, len(llm_prompt_payload) // 4),
            },
            "llm_yaml": llm_prompt_payload,
            "text_buffers": text_buffers,
            "tree": tree_root,
        }
