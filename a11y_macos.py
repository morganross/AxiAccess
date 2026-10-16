"""
macOS Native Accessibility Engine (AXUIElement / ApplicationServices / Quartz)
for A11y-SSH Universal Remote Desktop Accessibility Observer.

Features:
- Unlocks lazy Electron / Chromium / WebKit accessibility trees on macOS by setting
  `AXEnhancedUserInterface` and `AXManualAccessibility` on target application PIDs.
- Enumerates all desktop windows via Quartz `CGWindowListCopyWindowInfo` + `NSWorkspace`.
- Walks the native `AXUIElementRef` hierarchy, pruning empty layout wrappers (`AXGroup`, `AXSplitGroup`, `AXScrollArea`).
- Extracts full unrendered / off-screen text buffers via `kAXValueAttribute`, `kAXNumberOfCharactersAttribute`,
  `kAXVisibleCharacterRangeAttribute`, and `kAXStringForRangeParameterizedAttribute`.
"""

import time
import threading

try:
    import objc
    from CoreFoundation import kCFBooleanTrue, kCFBooleanFalse
    import Quartz
    from AppKit import NSWorkspace, NSApplicationActivationPolicyRegular
    import ApplicationServices as HIServices

    HAS_MACOS_AX = True
except ImportError:
    HAS_MACOS_AX = False


# Normalized role map from macOS AXRole strings to A11y-SSH semantic roles
AX_ROLE_MAP = {
    "AXWindow": "window",
    "AXApplication": "application",
    "AXButton": "button",
    "AXPopUpButton": "combobox",
    "AXComboBox": "combobox",
    "AXCheckBox": "checkbox",
    "AXRadioButton": "radiobutton",
    "AXTextField": "edit",
    "AXTextArea": "document",
    "AXWebArea": "document",
    "AXStaticText": "text",
    "AXLink": "link",
    "AXImage": "image",
    "AXList": "list",
    "AXOutline": "tree",
    "AXRow": "treeitem",
    "AXTable": "table",
    "AXTabGroup": "tab",
    "AXMenuBar": "menubar",
    "AXMenuBarItem": "menuitem",
    "AXMenu": "menu",
    "AXMenuItem": "menuitem",
    "AXToolbar": "toolbar",
    "AXScrollArea": "pane",
    "AXGroup": "group",
    "AXSplitGroup": "group",
    "AXLayoutArea": "pane",
    "AXSheet": "window",
    "AXDrawer": "pane",
    "AXValueIndicator": "slider",
    "AXSlider": "slider",
    "AXProgressIndicator": "progressbar",
    "AXScrollBar": "scrollbar",
    "AXSplitter": "separator",
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
}


class MacOSA11yEngine:
    """
    Native macOS AXUIElement semantic tree & off-screen text extractor.
    Outputs the exact same schema and 5-tier LLM verbosity formats as WindowsA11yEngine.
    """

    def __init__(self):
        self._lock = threading.Lock()
        self.wakeup_enabled = True

    def set_screen_reader_flag(self, enabled: bool = True) -> bool:
        self.wakeup_enabled = bool(enabled)
        if HAS_MACOS_AX:
            # Broadcast AXEnhancedUserInterface + AXManualAccessibility to running GUI apps
            ws = NSWorkspace.sharedWorkspace()
            for app in ws.runningApplications():
                if app.activationPolicy() == NSApplicationActivationPolicyRegular:
                    self._unlock_app_pid(int(app.processIdentifier()), enabled)
        return self.wakeup_enabled

    def get_screen_reader_flag(self) -> bool:
        return self.wakeup_enabled

    def _unlock_app_pid(self, pid: int, enabled: bool = True):
        """
        Sets AXEnhancedUserInterface and AXManualAccessibility on a target macOS PID.
        This is the secret handshake required on macOS for Electron (VS Code, ChatGPT, Slack)
        and Chromium browsers to build and expose their internal DOM accessibility trees.
        """
        if not HAS_MACOS_AX or pid <= 0:
            return
        try:
            app_ref = HIServices.AXUIElementCreateApplication(pid)
            val = kCFBooleanTrue if enabled else kCFBooleanFalse
            HIServices.AXUIElementSetAttributeValue(
                app_ref, "AXEnhancedUserInterface", val
            )
            HIServices.AXUIElementSetAttributeValue(
                app_ref, "AXManualAccessibility", val
            )
        except Exception:
            pass

    def _ax_attr(self, el, attr_name: str, default=None):
        if not HAS_MACOS_AX or el is None:
            return default
        try:
            err, val = HIServices.AXUIElementCopyAttributeValue(el, attr_name, None)
            if err == 0 and val is not None:
                return val
        except Exception:
            pass
        return default

    def _ax_param_attr(self, el, attr_name: str, param, default=None):
        if not HAS_MACOS_AX or el is None:
            return default
        try:
            err, val = HIServices.AXUIElementCopyParameterizedAttributeValue(
                el, attr_name, param, None
            )
            if err == 0 and val is not None:
                return val
        except Exception:
            pass
        return default

    def list_windows(self) -> dict:
        t0 = time.perf_counter()
        if not HAS_MACOS_AX:
            return {
                "platform": "macOS AXUIElement (PyObjC ApplicationServices not installed)",
                "screen_reader_flag": self.wakeup_enabled,
                "scan_time_ms": 0.0,
                "count": 0,
                "windows": [],
            }

        ws = NSWorkspace.sharedWorkspace()
        front_app = ws.frontmostApplication()
        front_pid = int(front_app.processIdentifier()) if front_app else -1

        cg_options = (
            Quartz.kCGWindowListOptionOnScreenOnly
            | Quartz.kCGWindowListExcludeDesktopElements
        )
        cg_wins = Quartz.CGWindowListCopyWindowInfo(
            cg_options, Quartz.kCGNullWindowID
        )

        windows = []
        seen_pids = set()

        for w in cg_wins or []:
            layer = int(w.get(Quartz.kCGWindowLayer, 0))
            if layer != 0:
                continue
            pid = int(w.get(Quartz.kCGWindowOwnerPID, 0))
            owner = str(w.get(Quartz.kCGWindowOwnerName, "unknown"))
            win_id = int(w.get(Quartz.kCGWindowNumber, 0))
            title = str(w.get(Quartz.kCGWindowName, "") or "").strip()
            bounds_dict = w.get(Quartz.kCGWindowBounds, {})
            width = int(bounds_dict.get("Width", 0))
            height = int(bounds_dict.get("Height", 0))
            if width < 40 and height < 40:
                continue

            if self.wakeup_enabled and pid not in seen_pids:
                seen_pids.add(pid)
                self._unlock_app_pid(pid, True)

            # If CGWindowName is blank (due to macOS screen recording privacy), query AXTitle
            if not title:
                app_ref = HIServices.AXUIElementCreateApplication(pid)
                ax_wins = self._ax_attr(app_ref, "AXWindows", [])
                if ax_wins and len(ax_wins) > 0:
                    title = str(self._ax_attr(ax_wins[0], "AXTitle", "") or "").strip()

            windows.append(
                {
                    "hwnd": win_id or pid,
                    "pid": pid,
                    "process": owner,
                    "title": title or f"({owner})",
                    "class_name": "AXWindow",
                    "offscreen": False,
                    "minimized": False,
                    "focused": pid == front_pid,
                    "bounds": {
                        "left": int(bounds_dict.get("X", 0)),
                        "top": int(bounds_dict.get("Y", 0)),
                        "width": width,
                        "height": height,
                    },
                }
            )

        elapsed_ms = round((time.perf_counter() - t0) * 1000, 1)
        return {
            "platform": "macOS (AXUIElement / ApplicationServices)",
            "screen_reader_flag": self.wakeup_enabled,
            "scan_time_ms": elapsed_ms,
            "count": len(windows),
            "windows": windows,
        }

    def _find_window_ax(self, target_id: str):
        win_list = self.list_windows()["windows"]
        target_str = str(target_id).lower()

        matched_win = None
        for w in win_list:
            if str(w["hwnd"]) == str(target_id) or str(w["pid"]) == str(target_id):
                matched_win = w
                break
        if not matched_win:
            for w in win_list:
                if (
                    target_str in w["title"].lower()
                    or target_str in w["process"].lower()
                ):
                    matched_win = w
                    break

        if not matched_win:
            return None, None

        pid = matched_win["pid"]
        self._unlock_app_pid(pid, self.wakeup_enabled)
        app_ref = HIServices.AXUIElementCreateApplication(pid)
        ax_wins = self._ax_attr(app_ref, "AXWindows", [])
        if ax_wins and len(ax_wins) > 0:
            for ax_w in ax_wins:
                t = str(self._ax_attr(ax_w, "AXTitle", "") or "")
                if matched_win["title"].lower() in t.lower():
                    return matched_win, ax_w
            return matched_win, ax_wins[0]
        return matched_win, app_ref

    def _extract_ax_text(self, el, role: str, max_preview: int = 180):
        """
        Extracts both full document text (`kAXValueAttribute` / `kAXStringForRangeParameterizedAttribute`)
        and visible character range (`kAXVisibleCharacterRangeAttribute`) on macOS.
        """
        val = self._ax_attr(el, "AXValue")
        full_text = str(val).strip() if isinstance(val, str) else ""

        # If AXValue is empty on a document/text area, try parameterized range extraction
        if not full_text and role in TEXT_BEARING_ROLES:
            num_chars = self._ax_attr(el, "AXNumberOfCharacters", 0)
            if isinstance(num_chars, int) and num_chars > 0:
                try:
                    cf_range = HIServices.AXValueCreate(
                        HIServices.kAXValueCFRangeType,
                        Quartz.CFRange(0, min(num_chars, 65536)),
                    )
                    rng_str = self._ax_param_attr(
                        el, "AXStringForRange", cf_range, ""
                    )
                    if rng_str:
                        full_text = str(rng_str).strip()
                except Exception:
                    pass

        if not full_text:
            return None

        # Check visible character range to compute unrendered/off-screen character count
        vis_text = full_text
        vis_range_val = self._ax_attr(el, "AXVisibleCharacterRange")
        if vis_range_val is not None:
            try:
                vis_str = self._ax_param_attr(
                    el, "AXStringForRange", vis_range_val, ""
                )
                if vis_str:
                    vis_text = str(vis_str).strip()
            except Exception:
                pass

        total_chars = len(full_text)
        vis_chars = len(vis_text)
        offscreen_chars = max(0, total_chars - vis_chars) if vis_chars > 0 else 0
        total_lines = full_text.count("\n") + 1
        vis_lines = vis_text.count("\n") + (1 if vis_text else 0)

        preview = (
            full_text[:max_preview].replace("\r", "").replace("\n", " ↵ ").strip()
        )
        if total_chars > max_preview:
            preview += " …"

        return {
            "source": "AXTextRange" if offscreen_chars > 0 else "AXValue",
            "total_chars": total_chars,
            "visible_chars": vis_chars,
            "offscreen_chars": offscreen_chars,
            "total_lines": total_lines,
            "visible_lines": vis_lines,
            "preview": preview,
            "full_text": full_text,
            "visible_text": vis_text,
        }

    def inspect_window(
        self, target_id, max_depth: int = 9, max_nodes: int = 400
    ) -> dict:
        t0 = time.perf_counter()
        if not HAS_MACOS_AX:
            raise ValueError("macOS ApplicationServices (PyObjC) is only available on macOS hosts.")

        win_meta, ax_root = self._find_window_ax(target_id)
        if not win_meta or ax_root is None:
            raise ValueError(f"Window '{target_id}' not found on macOS desktop.")

        raw_nodes_visited = 0
        pruned_nodes_count = 0
        text_buffers = []
        seen_text_hashes = set()
        yaml_lines = []

        def walk(el, depth):
            nonlocal raw_nodes_visited, pruned_nodes_count
            if depth > max_depth or pruned_nodes_count >= max_nodes or el is None:
                return None

            raw_nodes_visited += 1
            ax_role = str(self._ax_attr(el, "AXRole", "AXGroup") or "AXGroup")
            role = AX_ROLE_MAP.get(ax_role, ax_role.replace("AX", "").lower())
            title = str(self._ax_attr(el, "AXTitle", "") or "").strip()
            desc = str(self._ax_attr(el, "AXDescription", "") or "").strip()
            name = title or desc

            enabled = bool(self._ax_attr(el, "AXEnabled", True))
            focused = bool(self._ax_attr(el, "AXFocused", False))

            text_info = self._extract_ax_text(el, role)
            is_meaningful = (
                bool(name)
                or (text_info is not None)
                or (role not in CONTAINER_ROLES)
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

                if text_info:
                    src = text_info["source"]
                    tc = text_info["total_chars"]
                    tl = text_info["total_lines"]
                    oc = text_info["offscreen_chars"]
                    badges.append(
                        f"{src}: {tc} chars / {tl} lines"
                        + (f" [{oc} off-screen]" if oc > 0 else "")
                    )
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
                                "visible_chars": text_info["visible_chars"],
                                "offscreen_chars": oc,
                                "total_lines": tl,
                                "visible_lines": text_info["visible_lines"],
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
                    "offscreen": False,
                    "focused": focused,
                    "enabled": enabled,
                    "has_text": text_info is not None,
                    "text_source": text_info["source"] if text_info else None,
                    "total_chars": text_info["total_chars"] if text_info else 0,
                    "offscreen_chars": text_info["offscreen_chars"] if text_info else 0,
                    "preview": text_info["preview"] if text_info else "",
                    "children": [],
                }
                next_depth = depth + 1

            children_nodes = []
            ax_children = self._ax_attr(el, "AXChildren", []) or []
            for child in ax_children:
                if pruned_nodes_count >= max_nodes:
                    break
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

        tree = walk(ax_root, 0)
        tree_root = tree if isinstance(tree, list) else ([tree] if tree else [])
        elapsed_ms = round((time.perf_counter() - t0) * 1000, 1)
        compression_pct = (
            round((1 - (pruned_nodes_count / max(1, raw_nodes_visited))) * 100, 1)
            if raw_nodes_visited > 0
            else 0.0
        )

        header = (
            f"# Semantic GUI-SSH Snapshot (macOS AXUIElement)\n"
            f"window: \"{win_meta['title']}\" (process: {win_meta['process']}, pid: {win_meta['pid']})\n"
            f"stats: {pruned_nodes_count} semantic nodes (pruned from {raw_nodes_visited} raw AX nodes, "
            f"{compression_pct}% noise removed in {elapsed_ms}ms)\n"
            f"elements:\n"
        )
        llm_prompt_payload = header + "\n".join(yaml_lines)

        return {
            "window": win_meta,
            "stats": {
                "raw_nodes": raw_nodes_visited,
                "semantic_nodes": pruned_nodes_count,
                "compression_pct": compression_pct,
                "text_buffers_found": len(text_buffers),
                "total_extracted_chars": sum(b["total_chars"] for b in text_buffers),
                "offscreen_extracted_chars": sum(
                    b["offscreen_chars"] for b in text_buffers
                ),
                "scan_time_ms": elapsed_ms,
                "estimated_tokens": max(1, len(llm_prompt_payload) // 4),
            },
            "llm_yaml": llm_prompt_payload,
            "text_buffers": text_buffers,
            "tree": tree_root,
        }
