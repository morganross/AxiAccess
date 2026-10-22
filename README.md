# AxiAccess: Universal Remote Desktop Accessibility Observer

**AxiAccess is a cross-platform semantic "GUI-SSH" sidechannel for LLMs (`Windows` · `macOS` · `Linux`).**  
Extracts live desktop UI hierarchies and **unrendered / off-screen document text buffers** directly from OS-native accessibility services (`Windows UI Automation`, `macOS AXUIElement`, `Linux AT-SPI2`)—independently of the pixel framebuffer, without taking screenshots, and without touching window focus or mouse input.

GitHub Repository: **[https://github.com/morganross/AxiAccess](https://github.com/morganross/AxiAccess)**

---

## Cross-Platform Native Engines

AxiAccess automatically selects the native OS Accessibility backend for the host machine and maps all three OSes into a unified schema and 5-tier LLM verbosity format:

| OS | Engine Module | Native Accessibility API | Unrendered / Off-Screen Text API | Electron / Browser Wakeup Handshake |
| :--- | :--- | :--- | :--- | :--- |
| **Windows** | Rust (`src/engine.rs`) / [`axiaccess_engine.py`](axiaccess_engine.py) | `UIAutomationCore.dll` (`IUIAutomation` + `CacheRequest` COM batching + `WinSta0\Default` bridge) | `IUIAutomationTextPattern::DocumentRange` & `GetVisibleRanges()` | `SystemParametersInfoW(SPI_SETSCREENREADER, 1)` |
| **macOS** | [`axiaccess_macos.py`](axiaccess_macos.py) | `ApplicationServices` / `HIServices` (`AXUIElementRef`) + `Quartz.CGWindowList` | `kAXValueAttribute` + `kAXStringForRangeParameterizedAttribute` + `kAXVisibleCharacterRangeAttribute` | `AXUIElementSetAttributeValue(app, "AXManualAccessibility", kCFBooleanTrue)` |
| **Linux** | [`axiaccess_linux.py`](axiaccess_linux.py) | `AT-SPI2` over D-Bus (`pyatspi` / `gi.repository.Atspi`) | `org.a11y.atspi.Text.getText(0, -1)` | `org.a11y.Status.IsEnabled = true` & `ScreenReaderEnabled = true` |

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

## Quick Start (Rust Binary & Python)

### Option A: Native Rust Binary (Recommended)

```bash
cargo build --release
```

Run AxiAccess in any of three modes:

```bash
# 1. Start the HTTP REST API, WebSocket event stream, and Web Dashboard:
./target/release/axiaccess serve --port 8765

# 2. Run one-shot query from terminal / SSH:
./target/release/axiaccess query ChatGPT -v glance
./target/release/axiaccess query ChatGPT -v text_only

# 3. Run as an MCP JSON-RPC stdio server:
./target/release/axiaccess mcp
```

### Option B: Python Server

```bash
pip install -r requirements.txt
python server.py
```
* **Linux system package note:** Ensure the system AT-SPI2 Python bindings are installed (`sudo apt install python3-pyatspi gir1.2-atspi-2.0` on Debian/Ubuntu, or `sudo dnf install python3-pyatspi` on Fedora).
* **macOS permission note:** Grant Accessibility permission to your terminal/Python binary in `System Settings -> Privacy & Security -> Accessibility`.

Then open **[http://127.0.0.1:8765](http://127.0.0.1:8765)** in your browser.

---

## Querying from an LLM or CLI

```bash
# Desktop-wide glance (plain text for direct LLM prompt injection)
curl "http://127.0.0.1:8765/api/llm?target=all&verbosity=glance&format=text"

# Extract pure text (including off-screen/unrendered lines) from an open app
curl "http://127.0.0.1:8765/api/llm?target=ChatGPT&verbosity=text_only&format=text"

# Extract only actionable buttons/inputs/links
curl "http://127.0.0.1:8765/api/llm?target=firefox&verbosity=interactive&format=text"
```

---

## Reader-Converter for Coding Agent LLMs (`axiaccess_reader.py` / `axiaccess reader`)

While the **Sender** runs on the remote host capturing the raw accessibility tree, the **Reader-Converter** connects over SSH (or HTTP/locally) and converts the screen into clean, token-efficient semantic context blocks specifically for coding agents:

- `<active_editor>` — Open file name, language, visible code, and off-screen buffers.
- `<terminal>` — Shell prompt, last executed command, and output traces.
- `<errors_detected>` — Isolated compiler errors, tracebacks, and test failures.
- `<browser_preview>` — Active web preview tab & URL for localhost testing.
- `<active_dialogs>` — Popups and modal alerts blocking interaction.

### Remote Execution Over SSH

```bash
# 1. Connect over SSH and get an XML coding-agent context prompt:
./target/release/axiaccess reader --ssh user@remote-box -f xml
# or using Python:
python axiaccess_reader.py --ssh user@remote-box -f xml

# 2. Get Markdown with fenced code blocks:
python axiaccess_reader.py --ssh user@remote-box -f markdown

# 3. Low-token Delta / Diff Mode ("What changed since my last step?"):
python axiaccess_reader.py --ssh user@remote-box --diff

# 4. Target a specific application:
python axiaccess_reader.py --ssh user@remote-box "Visual Studio Code" -f xml
```

### Python SDK Example for Agent Loops

```python
from axiaccess_reader import RemoteAxiAccessClient, CodingAgentSynthesizer, format_as_coding_xml

# Connect to remote desktop over SSH
client = RemoteAxiAccessClient(ssh_host="user@dev-server")
raw_snapshot = client.fetch_desktop_snapshot(target="all")

# Synthesize into coding-agent prompt
synthesizer = CodingAgentSynthesizer(raw_snapshot, host_label="dev-server")
agent_context = synthesizer.synthesize()

# Inject into LLM prompt
prompt = format_as_coding_xml(agent_context)
print(prompt)
```

---

## MCP Server (Claude / Gemini / Cursor / SSH)

```bash
./target/release/axiaccess mcp
# or:
python mcp_server.py
```
Exposes the `observe_desktop(target, verbosity)` tool over JSON-RPC 2.0 `stdio`.
