import sys
import time
import threading

if sys.platform == "win32":
    import ctypes
    from ctypes import wintypes
    import comtypes
    import comtypes.client

    # Load native Windows UI Automation Type Library
    comtypes.client.GetModule("UIAutomationCore.dll")
    from comtypes.gen.UIAutomationClient import (
        CUIAutomation,
        IUIAutomation,
        IUIAutomationElement,
        IUIAutomationTextPattern,
        IUIAutomationValuePattern,
        IUIAutomationLegacyIAccessiblePattern,
        TreeScope_Children,
    )
else:
    ctypes = None
    wintypes = None
    comtypes = None
    IUIAutomation = object
    IUIAutomationElement = object

# UIA Property IDs for high-speed CacheRequest batching
UIA_ControlTypePropertyId = 30003
UIA_NamePropertyId = 30005
UIA_HasKeyboardFocusPropertyId = 30008
UIA_IsEnabledPropertyId = 30010
UIA_AutomationIdPropertyId = 30011
UIA_ClassNamePropertyId = 30012
UIA_NativeWindowHandlePropertyId = 30020
UIA_IsOffscreenPropertyId = 30022
UIA_IsTextPatternAvailablePropertyId = 30040
UIA_IsValuePatternAvailablePropertyId = 30043
UIA_ValueValuePropertyId = 30045

# UIA Pattern IDs
UIA_ValuePatternId = 10002
UIA_TextPatternId = 10014
UIA_LegacyIAccessiblePatternId = 10018

# Control Type mapping
CONTROL_TYPES = {
    50000: "button",
    50001: "calendar",
    50002: "checkbox",
    50003: "combobox",
    50004: "edit",
    50005: "link",
    50006: "image",
    50007: "listitem",
    50008: "list",
    50009: "menu",
    50010: "menubar",
    50011: "menuitem",
    50012: "progressbar",
    50013: "radiobutton",
    50014: "scrollbar",
    50015: "slider",
    50016: "spinner",
    50017: "statusbar",
    50018: "tab",
    50019: "tabitem",
    50020: "text",
    50021: "toolbar",
    50022: "tooltip",
    50023: "tree",
    50024: "treeitem",
    50025: "custom",
    50026: "group",
    50027: "thumb",
    50028: "datagrid",
    50029: "dataitem",
    50030: "document",
    50031: "splitbutton",
    50032: "window",
    50033: "pane",
    50034: "header",
    50035: "headeritem",
    50036: "table",
    50037: "titlebar",
    50038: "separator",
}

# Roles that are pure layout wrappers unless they have a name or text pattern
CONTAINER_ROLES = {
    "pane",
    "group",
    "custom",
    "separator",
    "scrollbar",
    "thumb",
    "image",
    "tooltip",
}

# Roles eligible for expensive TextPattern DocumentRange extraction
# (Leaf 'text' and 'listitem' nodes already expose their string in CachedName,
#  and their parent 'document'/'pane' TextPattern already contains the full concatenated text)
TEXT_BEARING_ROLES = {
    "document",
    "edit",
    "pane",
    "window",
}

PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
kernel32 = ctypes.windll.kernel32 if sys.platform == "win32" else None
user32 = ctypes.windll.user32 if sys.platform == "win32" else None


def get_process_name(pid: int) -> str:
    if not pid or sys.platform != "win32":
        return "system"
    h_proc = kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
    if not h_proc:
        return f"pid:{pid}"
    try:
        buf = ctypes.create_unicode_buffer(512)
        size = wintypes.DWORD(512)
        if kernel32.QueryFullProcessImageNameW(h_proc, 0, buf, ctypes.byref(size)):
            return buf.value.split("\\")[-1]
    finally:
        kernel32.CloseHandle(h_proc)
    return f"pid:{pid}"


def set_screen_reader_flag(enabled: bool = True) -> bool:
    """
    Sets the OS-native screen reader flag across Windows (SPI_SETSCREENREADER),
    macOS (AXManualAccessibility / AXEnhancedUserInterface), and Linux (org.a11y.Status).
    """
    if sys.platform == "win32":
        SPI_SETSCREENREADER = 0x0047
        SPIF_SENDCHANGE = 0x0002
        user32.SystemParametersInfoW(
            SPI_SETSCREENREADER, 1 if enabled else 0, None, SPIF_SENDCHANGE
        )
        return get_screen_reader_flag()
    return get_universal_engine().set_screen_reader_flag(enabled)


def get_screen_reader_flag() -> bool:
    if sys.platform == "win32":
        SPI_GETSCREENREADER = 0x0046
        flag = wintypes.BOOL()
        user32.SystemParametersInfoW(SPI_GETSCREENREADER, 0, ctypes.byref(flag), 0)
        return bool(flag.value)
    return get_universal_engine().get_screen_reader_flag()


class WindowsA11yEngine:
    """
    High-speed, read-only Windows UI Automation semantic extractor.
    - Bridges across Windows Desktops (WinSta0\\Default) so it works from SSH/services/sandboxes.
    - Uses UIA CacheRequest to batch COM calls (10x faster tree walking).
    - Queries IUIAutomationTextPattern (DocumentRange vs GetVisibleRanges) for unrendered off-screen text.
    """

    def __init__(self):
        self._lock = threading.Lock()
        self._hdesk = user32.OpenDesktopW("Default", 0, False, 0x01FF)
        # Ensure screen reader flag is active so Electron/Browsers expose full trees
        set_screen_reader_flag(True)

    def _attach_interactive_desktop(self):
        if self._hdesk:
            user32.SetThreadDesktop(self._hdesk)

    def _get_uia(self):
        self._attach_interactive_desktop()
        comtypes.CoInitialize()
        uia = comtypes.client.CreateObject(CUIAutomation, interface=IUIAutomation)
        return uia

    def _build_cache_request(self, uia: IUIAutomation):
        cache_req = uia.CreateCacheRequest()
        for prop_id in (
            UIA_ControlTypePropertyId,
            UIA_NamePropertyId,
            UIA_HasKeyboardFocusPropertyId,
            UIA_IsEnabledPropertyId,
            UIA_ClassNamePropertyId,
            UIA_IsOffscreenPropertyId,
            UIA_IsTextPatternAvailablePropertyId,
            UIA_IsValuePatternAvailablePropertyId,
            UIA_ValueValuePropertyId,
        ):
            cache_req.AddProperty(prop_id)
        return cache_req

    def _enum_interactive_hwnds(self):
        self._attach_interactive_desktop()
        hwnds = []
        WNDENUMPROC = ctypes.WINFUNCTYPE(
            ctypes.c_bool, wintypes.HWND, wintypes.LPARAM
        )

        def _cb(hwnd, lparam):
            if user32.IsWindowVisible(hwnd):
                length = user32.GetWindowTextLengthW(hwnd)
                if length > 0:
                    hwnds.append(int(hwnd))
            return True

        if self._hdesk:
            user32.EnumDesktopWindows(self._hdesk, WNDENUMPROC(_cb), 0)
        else:
            user32.EnumWindows(WNDENUMPROC(_cb), 0)
        return hwnds

    def list_windows(self):
        t0 = time.perf_counter()
        uia = self._get_uia()
        hwnds = self._enum_interactive_hwnds()

        focused_hwnd = user32.GetForegroundWindow()
        windows = []

        for hwnd in hwnds:
            try:
                el = uia.ElementFromHandle(hwnd)
                name = (el.CurrentName or "").strip()
                cls = el.CurrentClassName or ""
                pid = int(el.CurrentProcessId)
                offscreen = bool(el.CurrentIsOffscreen)

                if cls in ("Progman", "WorkerW", "IME", "MSCTFIME UI"):
                    continue
                if cls in ("Windows.UI.Core.CoreWindow",) and (
                    not name or name == "Windows Input Experience"
                ):
                    continue
                if name == "Program Manager":
                    continue

                rect = el.CurrentBoundingRectangle
                w = max(0, rect.right - rect.left)
                h = max(0, rect.bottom - rect.top)
                if w == 0 and h == 0 and not user32.IsIconic(hwnd):
                    continue

                bounds = {
                    "left": rect.left,
                    "top": rect.top,
                    "width": w,
                    "height": h,
                }

                proc_name = get_process_name(pid)
                is_minimized = bool(user32.IsIconic(hwnd))

                windows.append(
                    {
                        "hwnd": int(hwnd),
                        "pid": pid,
                        "process": proc_name,
                        "title": name or f"({proc_name} - {cls})",
                        "class_name": cls,
                        "offscreen": offscreen,
                        "minimized": is_minimized,
                        "focused": bool(hwnd == focused_hwnd),
                        "bounds": bounds,
                    }
                )
            except comtypes.COMError:
                continue

        windows.sort(
            key=lambda w: (
                not w["focused"],
                w["minimized"],
                w["offscreen"],
                w["process"].lower(),
            )
        )
        elapsed_ms = round((time.perf_counter() - t0) * 1000, 1)
        return {
            "platform": "Windows UIA (Default Desktop Bridge)",
            "screen_reader_flag": get_screen_reader_flag(),
            "scan_time_ms": elapsed_ms,
            "count": len(windows),
            "windows": windows,
        }

    def _find_window_element(self, uia, target_id):
        hwnds = self._enum_interactive_hwnds()
        target_str = str(target_id).lower()

        for hwnd in hwnds:
            if str(hwnd) == str(target_id):
                try:
                    return uia.ElementFromHandle(hwnd)
                except comtypes.COMError:
                    pass

        for hwnd in hwnds:
            try:
                el = uia.ElementFromHandle(hwnd)
                if str(el.CurrentProcessId) == str(target_id):
                    return el
            except comtypes.COMError:
                continue

        for hwnd in hwnds:
            try:
                el = uia.ElementFromHandle(hwnd)
                name = (el.CurrentName or "").lower()
                cls = (el.CurrentClassName or "").lower()
                proc = get_process_name(int(el.CurrentProcessId)).lower()
                if target_str in name or target_str in cls or target_str in proc:
                    return el
            except comtypes.COMError:
                continue

        return None

    def _extract_cached_text(
        self,
        el: IUIAutomationElement,
        role: str,
        has_tp: bool,
        has_vp: bool,
        cached_val: str,
        max_preview: int = 180,
    ):
        """
        Uses pre-cached pattern availability flags so we only make cross-process TextPattern calls
        when an element genuinely supports IUIAutomationTextPattern.
        """
        if has_tp and role in TEXT_BEARING_ROLES:
            try:
                unk = el.GetCurrentPattern(UIA_TextPatternId)
                if unk:
                    tp = unk.QueryInterface(IUIAutomationTextPattern)
                    doc_range = tp.DocumentRange
                    full_text = (doc_range.GetText(65536) or "").strip()
                    if full_text:
                        vis_text = ""
                        try:
                            vis_ranges = tp.GetVisibleRanges()
                            if vis_ranges and vis_ranges.Length > 0:
                                parts = []
                                for i in range(min(vis_ranges.Length, 40)):
                                    r_txt = vis_ranges.GetElement(i).GetText(16384) or ""
                                    if r_txt:
                                        parts.append(r_txt)
                                vis_text = "\n".join(parts).strip()
                        except comtypes.COMError:
                            pass

                        total_chars = len(full_text)
                        vis_chars = len(vis_text)
                        offscreen_chars = (
                            max(0, total_chars - vis_chars) if vis_chars > 0 else 0
                        )
                        total_lines = full_text.count("\n") + 1
                        vis_lines = vis_text.count("\n") + (1 if vis_text else 0)

                        preview = (
                            full_text[:max_preview]
                            .replace("\r", "")
                            .replace("\n", " ↵ ")
                            .strip()
                        )
                        if total_chars > max_preview:
                            preview += " …"

                        return {
                            "source": "TextPattern",
                            "total_chars": total_chars,
                            "visible_chars": vis_chars,
                            "offscreen_chars": offscreen_chars,
                            "total_lines": total_lines,
                            "visible_lines": vis_lines,
                            "preview": preview,
                            "full_text": full_text,
                            "visible_text": vis_text,
                        }
            except comtypes.COMError:
                pass

        if has_vp and cached_val:
            val = cached_val.strip()
            if val:
                preview = (
                    val[:max_preview].replace("\r", "").replace("\n", " ↵ ").strip()
                )
                if len(val) > max_preview:
                    preview += " …"
                return {
                    "source": "ValuePattern",
                    "total_chars": len(val),
                    "visible_chars": len(val),
                    "offscreen_chars": 0,
                    "total_lines": val.count("\n") + 1,
                    "visible_lines": val.count("\n") + 1,
                    "preview": preview,
                    "full_text": val,
                    "visible_text": val,
                }

        return None

    def inspect_window(
        self, target_id, max_depth: int = 9, max_nodes: int = 400
    ) -> dict:
        t0 = time.perf_counter()
        uia = self._get_uia()
        walker = uia.ControlViewWalker
        cache_req = self._build_cache_request(uia)

        win_el_raw = self._find_window_element(uia, target_id)
        if not win_el_raw:
            raise ValueError(f"Window '{target_id}' not found.")

        win_el = win_el_raw.BuildUpdatedCache(cache_req)
        win_title = (win_el.CachedName or "").strip()
        win_pid = int(win_el_raw.CurrentProcessId)
        win_hwnd = int(win_el_raw.CurrentNativeWindowHandle or 0)
        win_cls = win_el.CachedClassName or ""
        win_proc = get_process_name(win_pid)
        win_offscreen = bool(win_el.CachedIsOffscreen)

        raw_nodes_visited = 0
        pruned_nodes_count = 0
        text_buffers = []
        seen_text_hashes = set()
        yaml_lines = []

        def walk(el, depth):
            nonlocal raw_nodes_visited, pruned_nodes_count
            if depth > max_depth or pruned_nodes_count >= max_nodes:
                return None

            raw_nodes_visited += 1
            try:
                ctrl_id = el.CachedControlType
                role = CONTROL_TYPES.get(ctrl_id, f"role_{ctrl_id}")
                name = (el.CachedName or "").strip()
                offscreen = bool(el.CachedIsOffscreen)
                enabled = bool(el.CachedIsEnabled)
                has_focus = bool(el.CachedHasKeyboardFocus)
                has_tp = bool(
                    el.GetCachedPropertyValue(UIA_IsTextPatternAvailablePropertyId)
                )
                has_vp = bool(
                    el.GetCachedPropertyValue(UIA_IsValuePatternAvailablePropertyId)
                )
                cached_val = ""
                if has_vp:
                    v = el.GetCachedPropertyValue(UIA_ValueValuePropertyId)
                    if isinstance(v, str):
                        cached_val = v
            except comtypes.COMError:
                return None

            # Skip unnamed off-screen leaf clutter (menus, tooltips, images, scrollbars)
            if offscreen and not name and not has_tp and not has_vp:
                if role in ("menu", "menuitem", "tooltip", "image", "scrollbar", "thumb", "separator", "button", "listitem"):
                    return None

            text_info = self._extract_cached_text(
                el, role, has_tp, has_vp, cached_val
            )

            # Collapse pure structural containers if they have no name and no unique text
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
                if has_focus:
                    badges.append("focused")
                if not enabled:
                    badges.append("disabled")
                if offscreen:
                    badges.append("offscreen")

                if text_info:
                    src = text_info["source"]
                    tc = text_info["total_chars"]
                    tl = text_info["total_lines"]
                    oc = text_info["offscreen_chars"]
                    if src == "TextPattern":
                        badges.append(
                            f"TextPattern: {tc} chars / {tl} lines"
                            + (f" [{oc} off-screen]" if oc > 0 else "")
                        )
                    else:
                        badges.append(f"{src}: {tc} chars")

                    # Deduplicate identical parent/child text buffers
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
                    "offscreen": offscreen,
                    "focused": has_focus,
                    "enabled": enabled,
                    "has_text": text_info is not None,
                    "text_source": text_info["source"] if text_info else None,
                    "total_chars": text_info["total_chars"] if text_info else 0,
                    "offscreen_chars": text_info["offscreen_chars"] if text_info else 0,
                    "preview": text_info["preview"] if text_info else "",
                    "children": [],
                }
                next_depth = depth + 1

            # Walk children using CacheRequest for batch COM retrieval
            children_nodes = []
            try:
                child = walker.GetFirstChildElementBuildCache(el, cache_req)
                while child and pruned_nodes_count < max_nodes:
                    child_res = walk(child, next_depth)
                    if child_res:
                        if isinstance(child_res, list):
                            children_nodes.extend(child_res)
                        else:
                            children_nodes.append(child_res)
                    child = walker.GetNextSiblingElementBuildCache(child, cache_req)
            except comtypes.COMError:
                pass

            if node_obj is not None:
                node_obj["children"] = children_nodes
                return node_obj
            return children_nodes

        tree = walk(win_el, 0)
        tree_root = tree if isinstance(tree, list) else ([tree] if tree else [])

        elapsed_ms = round((time.perf_counter() - t0) * 1000, 1)
        compression_pct = (
            round((1 - (pruned_nodes_count / max(1, raw_nodes_visited))) * 100, 1)
            if raw_nodes_visited > 0
            else 0.0
        )

        header = (
            f"# AxiAccess Snapshot\n"
            f"window: \"{win_title}\" (process: {win_proc}, pid: {win_pid}, hwnd: {win_hwnd})\n"
            f"stats: {pruned_nodes_count} semantic nodes (pruned from {raw_nodes_visited} raw UIA nodes, "
            f"{compression_pct}% noise removed in {elapsed_ms}ms)\n"
            f"elements:\n"
        )
        llm_prompt_payload = header + "\n".join(yaml_lines)

        return {
            "window": {
                "hwnd": win_hwnd,
                "pid": win_pid,
                "process": win_proc,
                "title": win_title,
                "class_name": win_cls,
                "offscreen": win_offscreen,
            },
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

    def format_for_llm(self, target_id: str = "all", verbosity: str = "compact") -> dict:
        """
        Renders the desktop or a target window at 5 explicitly tuned LLM verbosity tiers:
        - L1 'glance'      (~50-150 tokens): Window list, active focus, and buffer counts only.
        - L2 'text_only'   (~200-1,500 tokens): Pure extracted text buffers (visible + offscreen) without UI chrome.
        - L3 'interactive' (~150-500 tokens): Actionable controls only (buttons, inputs, links, tabs) + text summaries.
        - L4 'compact'     (~400-1,500 tokens): Pruned semantic YAML hierarchy + 180-char text previews (Default).
        - L5 'full'        (Unbounded): Complete semantic hierarchy + 100% of full unrendered DocumentRange text inline.
        """
        verbosity = verbosity.lower().strip()
        if verbosity not in ("glance", "text_only", "interactive", "compact", "full"):
            verbosity = "compact"

        # Desktop-wide glance if target_id is 'all' or 'desktop'
        if target_id.lower() in ("all", "desktop") or verbosity == "glance":
            win_data = self.list_windows()
            lines = [
                f"# Desktop Glance ({win_data['count']} windows, scanned in {win_data['scan_time_ms']}ms)",
            ]
            for w in win_data["windows"]:
                state = (
                    "FOCUSED"
                    if w["focused"]
                    else ("MINIMIZED" if w["minimized"] else ("OFFSCREEN" if w["offscreen"] else "VISIBLE"))
                )
                lines.append(
                    f"- [{state}] hwnd:{w['hwnd']} pid:{w['pid']} ({w['process']}) \"{w['title']}\""
                )

            if target_id.lower() not in ("all", "desktop"):
                # Also include quick summary stats of the requested target window
                snap = self.inspect_window(target_id, max_depth=6, max_nodes=150)
                s = snap["stats"]
                w = snap["window"]
                lines.append(f"\n## Target Window Summary: \"{w['title']}\" ({w['process']})")
                lines.append(
                    f"- Semantic Elements: {s['semantic_nodes']} (pruned from {s['raw_nodes']} raw nodes)"
                )
                lines.append(
                    f"- Text Buffers: {s['text_buffers_found']} ({s['total_extracted_chars']} total chars, "
                    f"{s['offscreen_extracted_chars']} off-screen chars)"
                )
                for b in snap["text_buffers"]:
                    lines.append(
                        f"  * [{b['id']}] {b['role']} \"{b['name']}\" ({b['source']}): "
                        f"{b['total_chars']} chars ({b['offscreen_chars']} offscreen) -> \"{b['preview'][:100]}\""
                    )

            text_out = "\n".join(lines)
            return {
                "verbosity": "glance",
                "target": target_id,
                "char_count": len(text_out),
                "estimated_tokens": max(1, len(text_out) // 4),
                "llm_text": text_out,
            }

        # For L2..L5, inspect the target window
        max_depth = 8 if verbosity in ("interactive", "compact") else 11
        max_nodes = 250 if verbosity == "interactive" else 400
        snap = self.inspect_window(target_id, max_depth=max_depth, max_nodes=max_nodes)
        w = snap["window"]
        s = snap["stats"]

        if verbosity == "text_only":
            lines = [
                f"# Extracted Text Only — \"{w['title']}\" ({w['process']}, pid:{w['pid']})",
                f"# Buffers: {s['text_buffers_found']} | Total Chars: {s['total_extracted_chars']} | Off-Screen Chars: {s['offscreen_extracted_chars']}",
                "",
            ]
            if snap["text_buffers"]:
                for b in snap["text_buffers"]:
                    lines.append(
                        f"--- [{b['id']}] {b['role']} \"{b['name']}\" ({b['source']} | "
                        f"Total: {b['total_chars']} chars, {b['total_lines']} lines | "
                        f"Off-screen: {b['offscreen_chars']} chars) ---"
                    )
                    lines.append(b["full_text"])
                    lines.append("")
            else:
                # Fallback: collect all visible text/document/heading names from the tree
                lines.append("# (No TextPattern container buffers; falling back to readable text nodes)")
                def collect_names(nodes):
                    for n in nodes:
                        if n["name"] and n["role"] in ("text", "document", "edit", "listitem", "link", "heading", "tabitem"):
                            lines.append(f"- [{n['role']}] {n['name']}")
                        if n.get("children"):
                            collect_names(n["children"])
                collect_names(snap["tree"])

            text_out = "\n".join(lines)

        elif verbosity == "interactive":
            ACTION_ROLES = {
                "button", "splitbutton", "checkbox", "radiobutton", "combobox",
                "edit", "link", "menuitem", "tabitem", "treeitem", "listitem", "slider", "document"
            }
            lines = [
                f"# Interactive Controls & Buffers — \"{w['title']}\" ({w['process']}, pid:{w['pid']})",
                "controls:",
            ]
            def collect_interactive(nodes):
                for n in nodes:
                    if n["role"] in ACTION_ROLES or n["has_text"]:
                        if not n["offscreen"] or n["has_text"]:
                            flags = []
                            if n["focused"]:
                                flags.append("focused")
                            if n["has_text"]:
                                flags.append(f"{n['text_source']}:{n['total_chars']}c")
                            f_str = f" ({', '.join(flags)})" if flags else ""
                            name_str = f" \"{n['name'][:80]}\"" if n["name"] else ""
                            lines.append(f"  - {n['role']}{name_str} [{n['id']}]{f_str}")
                    if n.get("children"):
                        collect_interactive(n["children"])
            collect_interactive(snap["tree"])
            text_out = "\n".join(lines)

        elif verbosity == "compact":
            text_out = snap["llm_yaml"]

        else:  # verbosity == "full"
            lines = [
                snap["llm_yaml"],
                "\n# ==========================================================================",
                "# FULL UNRENDERED & VISIBLE DOCUMENT TEXT BUFFERS (INLINE)",
                "# ==========================================================================",
            ]
            for b in snap["text_buffers"]:
                lines.append(
                    f"\n## Buffer {b['id']} ({b['role']} \"{b['name']}\") — "
                    f"{b['total_chars']} chars ({b['offscreen_chars']} off-screen)"
                )
                lines.append("```text")
                lines.append(b["full_text"])
                lines.append("```")
            text_out = "\n".join(lines)

        return {
            "verbosity": verbosity,
            "target": w["title"],
            "hwnd": w["hwnd"],
            "process": w["process"],
            "char_count": len(text_out),
            "estimated_tokens": max(1, len(text_out) // 4),
            "stats": s,
            "llm_text": text_out,
        }


_UNIVERSAL_ENGINE_INSTANCE = None


def get_universal_engine():
    """
    Auto-detects the host OS (Windows, macOS, or Linux) and returns the corresponding
    native OS Accessibility Engine equipped with the 5-tier LLM verbosity formatter.
    """
    global _UNIVERSAL_ENGINE_INSTANCE
    if _UNIVERSAL_ENGINE_INSTANCE is not None:
        return _UNIVERSAL_ENGINE_INSTANCE

    if sys.platform == "win32":
        _UNIVERSAL_ENGINE_INSTANCE = WindowsA11yEngine()
    elif sys.platform == "darwin":
        from axiaccess_macos import MacOSA11yEngine

        # Bind the shared 5-tier format_for_llm method onto MacOSA11yEngine
        MacOSA11yEngine.format_for_llm = WindowsA11yEngine.format_for_llm
        _UNIVERSAL_ENGINE_INSTANCE = MacOSA11yEngine()
    else:
        from axiaccess_linux import LinuxA11yEngine

        # Bind the shared 5-tier format_for_llm method onto LinuxA11yEngine
        LinuxA11yEngine.format_for_llm = WindowsA11yEngine.format_for_llm
        _UNIVERSAL_ENGINE_INSTANCE = LinuxA11yEngine()

    return _UNIVERSAL_ENGINE_INSTANCE


