import asyncio
import os
import subprocess
import sys
import time
from typing import Optional
from fastapi import FastAPI, HTTPException, WebSocket, WebSocketDisconnect
from fastapi.responses import HTMLResponse, JSONResponse, PlainTextResponse
from pydantic import BaseModel
import uvicorn

from axiaccess_engine import (
    get_universal_engine,
    get_screen_reader_flag,
    set_screen_reader_flag,
)

app = FastAPI(
    title="AxiAccess: Universal Remote Desktop Accessibility Observer",
    description="AxiAccess: Cross-Platform Sidechannel OS Accessibility & Off-Screen Text Extractor for LLMs (Windows, macOS, Linux)",
    version="2.0.0",
)

engine = get_universal_engine()
event_history = []


class WakeupRequest(BaseModel):
    enabled: bool = True


class SpawnDemoRequest(BaseModel):
    lines: int = 250


@app.get("/api/windows")
def api_list_windows():
    return engine.list_windows()


@app.get("/api/llm")
def api_llm_consume(
    target: str = "all",
    verbosity: str = "compact",
    format: str = "json",
):
    """
    Primary LLM consumption endpoint with 5 explicit verbosity tiers:
    - verbosity=glance      (~50-150 tokens): Open windows, focus state, and high-level buffer summary.
    - verbosity=text_only   (~200-1500 tokens): Pure extracted text buffers (visible + unrendered off-screen) without UI chrome.
    - verbosity=interactive (~150-500 tokens): Actionable controls only (buttons, inputs, links, tabs).
    - verbosity=compact     (~400-1500 tokens): Pruned semantic YAML tree + 180-char text previews (default).
    - verbosity=full        (Unbounded): Pruned YAML tree + 100% of unrendered & visible DocumentRange buffers inline.
    Set `format=text` to get raw Markdown/YAML text directly for piping into an LLM prompt.
    """
    try:
        res = engine.format_for_llm(target_id=target, verbosity=verbosity)
        if format.lower() in ("text", "plain", "markdown", "yaml"):
            return PlainTextResponse(res["llm_text"])
        return res
    except ValueError as e:
        raise HTTPException(status_code=404, detail=str(e))
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@app.get("/api/inspect/{target_id}")
def api_inspect_window(
    target_id: str, max_depth: int = 9, max_nodes: int = 350
):
    try:
        return engine.inspect_window(
            target_id, max_depth=max_depth, max_nodes=max_nodes
        )
    except ValueError as e:
        raise HTTPException(status_code=404, detail=str(e))
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@app.post("/api/wakeup")
def api_set_wakeup(req: WakeupRequest):
    state = set_screen_reader_flag(req.enabled)
    return {"screen_reader_flag": state}


@app.post("/api/spawn-test-pad")
def api_spawn_test_pad(req: SpawnDemoRequest):
    """
    Creates a 250-line sample document and opens it in the native OS text editor
    (Notepad on Windows, TextEdit on macOS, gedit/xdg-open on Linux)
    so you can immediately test extracting lines 26..250 that are scrolled off-screen.
    """
    sample_path = os.path.join(os.path.dirname(__file__), "offscreen_test_doc.txt")
    lines = [
        "==========================================================================",
        " AXIACCESS UNRENDERED / OFF-SCREEN TEXT EXTRACTION VERIFICATION DOCUMENT",
        "==========================================================================",
        "Only the first ~25 lines of this file are painted in the editor's visible",
        "viewport. Lines 30 through 250 are completely scrolled off-screen and have",
        "NEVER been rendered into the desktop framebuffer.",
        "",
        "Yet native OS Accessibility Text Ranges retrieve all 250 lines instantly",
        "in read-only mode without scrolling or moving window focus!",
        "--------------------------------------------------------------------------",
    ]
    for i in range(11, req.lines + 1):
        zone = "VISIBLE VIEWPORT ZONE" if i <= 25 else "OFF-SCREEN / UNRENDERED BUFFER ZONE"
        secret = " [SECRET_MARKER_ALPHA_992]" if i == req.lines - 5 else ""
        lines.append(
            f"Line {i:03d}/{req.lines:03d} | [{zone}] | Semantic payload record #{i * 137}{secret}"
        )

    with open(sample_path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))

    if sys.platform == "win32":
        subprocess.Popen(["notepad.exe", sample_path])
    elif sys.platform == "darwin":
        subprocess.Popen(["open", "-a", "TextEdit", sample_path])
    else:
        subprocess.Popen(["xdg-open", sample_path])

    time.sleep(0.8)
    return {"status": "spawned", "file": sample_path, "lines": req.lines}


@app.get("/api/architecture")
def api_cross_platform_matrix():
    """Returns the cross-platform OS Accessibility translation matrix & MCP spec."""
    return {
        "platforms": [
            {
                "os": "Windows",
                "status": "Implemented (axiaccess_engine.py)" + (" — Active Host" if sys.platform == "win32" else ""),
                "api": "UI Automation (UIAutomationCore.dll / IUIAutomation)",
                "tree_walker": "ControlViewWalker + CreateCacheRequest (Batched COM)",
                "deep_text_api": "IUIAutomationTextPattern::DocumentRange & GetVisibleRanges()",
                "wakeup_signal": "SystemParametersInfoW(SPI_SETSCREENREADER, 1)",
                "session_bridge": "OpenDesktopW('Default') + SetThreadDesktop (Bypasses SSH/Service isolation)",
            },
            {
                "os": "macOS",
                "status": "Implemented (axiaccess_macos.py)" + (" — Active Host" if sys.platform == "darwin" else ""),
                "api": "ApplicationServices / HIServices (AXUIElementRef)",
                "tree_walker": "AXUIElementCopyAttributeValue(kAXChildrenAttribute)",
                "deep_text_api": "kAXValueAttribute + kAXStringForRangeParameterizedAttribute + kAXVisibleCharacterRangeAttribute",
                "wakeup_signal": "AXUIElementSetAttributeValue(app, 'AXManualAccessibility', kCFBooleanTrue)",
                "session_bridge": "Quartz CGWindowList + Aqua GUI Session over SSH stdio",
            },
            {
                "os": "Linux",
                "status": "Implemented (axiaccess_linux.py)" + (" — Active Host" if sys.platform.startswith("linux") else ""),
                "api": "AT-SPI2 over D-Bus (org.a11y.atspi / pyatspi)",
                "tree_walker": "org.a11y.atspi.Accessible.GetChildren / GetRoleName",
                "deep_text_api": "org.a11y.atspi.Text.getText(0, -1)",
                "wakeup_signal": "org.a11y.Status.IsEnabled = true & ScreenReaderEnabled = true",
                "session_bridge": "Native D-Bus session bus forwarding over SSH",
            },
        ]
    }


@app.websocket("/ws/events")
async def websocket_events(ws: WebSocket):
    """
    Live sidechannel event stream (the `tail -f` of the desktop).
    Monitors window creations, focus switches, title changes, and text buffer growth
    without touching the user's focus or cursor.
    """
    await ws.accept()
    prev_windows = {}
    try:
        init_scan = await asyncio.to_thread(engine.list_windows)
        for w in init_scan["windows"]:
            prev_windows[w["hwnd"]] = w

        await ws.send_json(
            {
                "timestamp": time.strftime("%H:%M:%S"),
                "type": "CONNECTED",
                "process": "AxiAccess",
                "message": f"AxiAccess sidechannel attached to WinSta0\\Default ({len(prev_windows)} windows monitored, read-only mode).",
            }
        )

        while True:
            await asyncio.sleep(1.5)
            scan = await asyncio.to_thread(engine.list_windows)
            curr_map = {w["hwnd"]: w for w in scan["windows"]}
            now_str = time.strftime("%H:%M:%S")

            # Detect new windows
            for hwnd, w in curr_map.items():
                if hwnd not in prev_windows:
                    await ws.send_json(
                        {
                            "timestamp": now_str,
                            "type": "WINDOW_OPENED",
                            "hwnd": hwnd,
                            "process": w["process"],
                            "message": f"New window opened: \"{w['title']}\" (PID {w['pid']})",
                        }
                    )
                else:
                    old_w = prev_windows[hwnd]
                    if w["title"] != old_w["title"]:
                        await ws.send_json(
                            {
                                "timestamp": now_str,
                                "type": "TITLE_CHANGED",
                                "hwnd": hwnd,
                                "process": w["process"],
                                "message": f"Title updated: \"{old_w['title']}\" → \"{w['title']}\"",
                            }
                        )
                    if w["focused"] and not old_w["focused"]:
                        await ws.send_json(
                            {
                                "timestamp": now_str,
                                "type": "FOCUS_CHANGED",
                                "hwnd": hwnd,
                                "process": w["process"],
                                "message": f"Active focus moved to \"{w['title']}\" ({w['process']})",
                            }
                        )

            # Detect closed windows
            for hwnd, old_w in prev_windows.items():
                if hwnd not in curr_map:
                    await ws.send_json(
                        {
                            "timestamp": now_str,
                            "type": "WINDOW_CLOSED",
                            "hwnd": hwnd,
                            "process": old_w["process"],
                            "message": f"Window closed: \"{old_w['title']}\" ({old_w['process']})",
                        }
                    )

            prev_windows = curr_map
    except WebSocketDisconnect:
        return
    except Exception:
        return


@app.get("/", response_class=HTMLResponse)
def serve_dashboard():
    with open("index.html", "r", encoding="utf-8") as f:
        return f.read()


if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=8765, log_level="info")
