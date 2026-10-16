# A11y-SSH: Universal Remote Desktop Accessibility Observer

**A semantic "GUI-SSH" sidechannel for LLMs.**  
Extracts live desktop UI hierarchies and **unrendered / off-screen document text buffers** directly from OS-native accessibility services (`Windows UI Automation`, `macOS AXUIElement`, `Linux AT-SPI2`)—independently of the pixel framebuffer, without taking screenshots, and without touching window focus or mouse input.

---

## Why This Exists

Standard **SSH / PowerShell** is blind to what happens inside GUI applications, while **RDP / VNC + Vision LLMs** rely on slow, token-expensive screenshots that can only see the pixels currently painted in the viewport.

**A11y-SSH** sits in the middle:
1. **Interactive Desktop Bridge (`WinSta0\Default`):** Bypasses Windows session/desktop isolation (`OpenDesktopW("Default") + SetThreadDesktop`) so background daemons and SSH sessions can inspect the live interactive GUI desktop.
2. **Screen Reader Wakeup Handshake (`SPI_SETSCREENREADER`):** Broadcasts the native OS assistive-technology flag so lazy apps (Chrome, Firefox, Electron, VS Code, ChatGPT, Slack) unlock their internal DOM and text buffers.
3. **Unrendered / Off-Screen Text Extraction (`IUIAutomationTextPattern`):** Queries `DocumentRange` alongside `GetVisibleRanges()` to retrieve thousands of lines of scrolled-away or unrendered document text straight from application memory.
4. **5-Tier LLM Verbosity API:** Compresses noisy accessibility trees (`CacheRequest`-batched COM traversal) into token-budgeted formats tailored for LLM agents.

---

## 5-Tier LLM Verbosity Levels (`/api/llm`)

| Tier | Query Parameter | Typical Token Cost | Description |
| :--- | :--- | :--- | :--- |
| **L1: Glance** | `verbosity=glance` | `~80–250 tokens` | Lists all open desktop windows, active focus state, and a high-level summary of available text buffers. |
| **L2: Interactive** | `verbosity=interactive` | `~300–1,300 tokens` | Actionable controls only (`button`, `edit`, `link`, `tabitem`, `menuitem`) + buffer pointers. Strips layout wrappers. |
| **L3: Compact** *(Default)* | `verbosity=compact` | `~500–6,000 tokens` | Noise-pruned YAML/ARIA hierarchy (`@e1`, `@e2`...) with 180-character text previews. |
| **L4: Text Only** | `verbosity=text_only` | `~500–7,000 tokens` | Zero UI chrome. Pure extracted document and conversation text (including all unrendered off-screen text). |
| **L5: Full** | `verbosity=full` | `Unbounded` | Complete pruned YAML hierarchy **plus** 100% of full `DocumentRange` text buffers inline. |

---

## Quick Start

### 1. Install Dependencies
```bash
pip install -r requirements.txt
```

### 2. Launch the Web Dashboard & HTTP/WebSocket Server
```bash
python server.py
```
Then open **[http://127.0.0.1:8765](http://127.0.0.1:8765)** in your browser.

### 3. Query from an LLM or CLI
```bash
# Desktop-wide glance (plain text for direct LLM prompt injection)
curl "http://127.0.0.1:8765/api/llm?target=all&verbosity=glance&format=text"

# Extract pure text (including off-screen/unrendered lines) from an open app
curl "http://127.0.0.1:8765/api/llm?target=ChatGPT&verbosity=text_only&format=text"

# Extract only actionable buttons/inputs/links
curl "http://127.0.0.1:8765/api/llm?target=firefox&verbosity=interactive&format=text"
```

### 4. Use as an MCP Server (Claude / Gemini / Cursor / SSH)
```bash
python mcp_server.py
```
Exposes the `observe_desktop(target, verbosity)` tool over JSON-RPC 2.0 `stdio`.

---

## Project Structure

- [`a11y_engine.py`](a11y_engine.py) — Core Windows UI Automation (`UIAutomationCore.dll`) engine, `WinSta0\Default` desktop bridge, `CacheRequest` batcher, `TextPattern` off-screen extractor, and 5-tier LLM formatter.
- [`server.py`](server.py) — FastAPI REST & WebSocket sidechannel server (`/api/windows`, `/api/llm`, `/api/inspect/{target}`, `/ws/events`).
- [`mcp_server.py`](mcp_server.py) — Model Context Protocol (MCP) `stdio` server for native LLM tool integration.
- [`index.html`](index.html) — Live interactive Dark-Mode Web Dashboard with one-click 250-line off-screen Notepad verification test.
