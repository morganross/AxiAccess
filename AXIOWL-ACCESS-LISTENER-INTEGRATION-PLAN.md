> **Superseded historical proposal — October 22, 2026.**
> Retained for history, not current implementation instructions. Follow [the approved sender/IDE/MCP plan](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/AXIACCESS-SENDER-IDE-MCP-IMPLEMENTATION-PLAN-2026-10-22.md) and [the current AxiAccess sender protocol](docs/SENDER-PROTOCOL.md).
> The old copied IDE engine crate, STA requirement, action/click/type tools, unauthenticated HTTP endpoints and assumed universal native-event claims below are superseded. The sender engine remains in AxiAccess; Windows UIA uses MTA; native event availability/coverage must be reported from actual capabilities. The IDE owns listener/MCP delivery; the daemon remains unchanged. Source wiring is not installed acceptance.

# AxiOwl Access Listener Integration Plan
**Document Version:** 1.0.0  
**Date:** October 2026  
**AxiAccess Repository:** [morganross/AxiAccess](https://github.com/morganross/AxiAccess)  
**AxiOwl-Desktop Repository:** [morganross/AxiOwl-Desktop](https://github.com/morganross/AxiOwl-Desktop)  

---

## 1. Executive Summary

This document specifies the architectural plan and implementation roadmap for integrating the **Access Listener** into the **AxiOwl-Desktop** ecosystem.

The Access Listener brings deep accessibility-driven operating system observation, real-time UI change tracking, tree diffing, and LLM-optimized reader/converter capabilities—originally pioneered in **AxiAccess**—directly into the AxiOwl developer environment.

### Core Architectural Mandate
> **CRITICAL DIRECTIVE:** The Access Listener **MUST NOT** be added to the existing `ide-daemon` (`AxiOwlIdeDaemon.exe`).
>
> The existing `ide-daemon` is a dedicated, minimal, single-instance supervisor whose sole responsibility is to monitor `axiowl-desktop.exe` over a private Windows named pipe and facilitate mobile pairing / Goose relay grants. Conflating accessibility observation with this supervisor would violate isolation guarantees, introduce COM/threading hazards, and jeopardize IDE stability.
>
> Instead, the Access Listener will be built as a **first-class dedicated workspace crate (`crates/axiowl-access-listener`)**, supervised either as an app-owned sidecar service or an isolated background worker within the AxiOwl runtime, complete with an **MCP (Model Context Protocol) server**, a **token-optimized Reader-Converter**, and a **React Settings UI card**.

---

## 2. Background & Provenance: The AxiAccess Engine

The Access Listener derives its capabilities from **AxiAccess** (`https://github.com/morganross/AxiAccess`), a native Rust accessibility observer developed to give coding agents and LLMs real-time awareness of GUI applications without visual screenshot latency or context-window bloat.

### 2.1 AxiAccess Architecture & Capabilities
1. **Native Windows UI Automation (UIA) Engine (`engine.rs`):**
   - **COM STA Threading:** Operates on dedicated Single-Threaded Apartment (`COINIT_APARTMENTTHREADED`) threads with proper Windows message pumps to ensure COM reentrancy safety.
   - **Cache Requests (`IUIAutomationCacheRequest`):** Pre-fetches element properties (`BoundingRectangle`, `Name`, `ControlType`, `AutomationId`, `Value`, `IsEnabled`, `HasKeyboardFocus`) in batch COM calls, eliminating chatty round-trips.
   - **Tree Walker & Filtering:** Traverses the desktop UIA element tree hierarchically, identifying parent-child relationships, off-screen states, and control boundaries.
   - **Real-Time COM Event Listeners:**
     - Focus Change Handler (`IUIAutomationFocusChangedEventHandler`)
     - Property Change Handler (`IUIAutomationPropertyChangedEventHandler`)
     - Structure Change Handler (`IUIAutomationStructureChangedEventHandler`)
     - Notification Handler (`IUIAutomationNotificationEventHandler`)

2. **Five-Tier LLM Verbosity System:**
   - **Tier 1: `glance` (< 500 tokens):** Identifies the currently active window, focused control, and top-level menu bar.
   - **Tier 2: `interactive`:** Emits only actionable controls (buttons, text inputs, list items, checkboxes, menus) with automation IDs and bounding coordinates.
   - **Tier 3: `compact`:** Prunes decorative layout containers, groupings, and empty panes, preserving semantic hierarchy.
   - **Tier 4: `text_only`:** Extracts visible text, labels, and status messages sequentially.
   - **Tier 5: `full`:** Complete raw accessibility tree with all COM attributes and pattern states.

3. **Structural Tree Diffing Engine:**
   - Computes symmetric tree deltas between consecutive observations (`added`, `removed`, `modified`).
   - Emits compact delta events so LLMs only process what changed (e.g., a modal appeared, a build completed, or a test status updated) rather than re-reading the entire UI.

4. **Coding Agent Reader-Converter:**
   - Formats snapshots and diffs into structured formats optimized for LLM comprehension:
     - **XML:** Semantic tag tree (`<Window title="AxiOwl"><Button id="build-btn" state="enabled">Build</Button></Window>`).
     - **Markdown:** Hierarchical bullet lists with state tags.
     - **Diff:** Unified diff blocks (`+ [Button] "Run Test"`) ideal for multi-turn agent turns.
   - Connects locally, over HTTP REST (`/api/snapshot`, `/api/diff`, `/api/events`, `/api/action`), WebSocket streaming (`/ws`), or remotely over **SSH tunnels**.

5. **Model Context Protocol (MCP) Server:**
   - Native JSON-RPC 2.0 stdio server providing tool definitions for coding frameworks (Claude Code, Copilot, Goose, Cursor, Antigravity).

---

## 3. AxiOwl-Desktop Codebase Audit & Architectural Boundary

### 3.1 AxiOwl-Desktop Repository Overview
`AxiOwl-Desktop` is an enterprise AI IDE desktop application structured as follows:

```
AxiOwl-Desktop/
├── branding/                       # Application assets and icon sets
├── crates/                         # Modular Rust workspace crates
│   ├── browser-automation/         # DOM automation & webview accessibility
│   ├── browser-host/               # Webview host abstraction & CEF accessibility handlers
│   ├── browser-mcp/                # MCP server exposing browser automation to agents
│   ├── cef-host-shared/            # Shared CEF definitions
│   ├── cef-locales/                # CEF localization resources
│   ├── codex-history/              # Session history store
│   ├── desktop-command-codegen/    # Proc macro code generation for desktop commands
│   ├── gui-state-export/           # Preference and recovery state migration helper
│   ├── hosted-gateway/             # Gateway connectivity
│   ├── ide-application/            # Provider configurations, routes, and sessions
│   ├── ide-orchestration/          # Task scheduler, threads, and host runs
│   ├── mcp-config/                 # MCP configuration management & tool catalog
│   ├── project-recovery/           # Editor recovery and project state
│   ├── provider-installer/         # Provider binaries installer
│   ├── tool-mcp/                   # MCP server exposing RTK and CLI tools
│   ├── webview2-host/              # Windows WebView2 backend
│   └── windows-gui-client/         # Signed CEF bootstrap cdylib
├── ide-daemon/                     # AxiOwlIdeDaemon.exe supervisor (Windows named pipe watchdog)
├── ide-ipc/                        # Private 1 MiB framed LE length-prefixed IPC contract
├── release/                        # Release packaging, signing policy, locks
├── scripts/                        # Build, signing, and release automation
├── src/                            # React + Vite frontend GUI
│   ├── components/                 # UI components (SettingsPage, EditorPane, etc.)
│   └── lib/                        # Runtime bridges and state stores
├── src-tauri/                      # axiowl-desktop.exe core Tauri v2 backend
│   ├── src/                        # Core backend Rust modules
│   └── Cargo.toml                  # Tauri dependencies and feature flags
├── tools/                          # Helper build scripts and stage tools
└── vendor/                         # Submodule trees (axiowl-remote-desktop, etc.)
```

### 3.2 The Existing Daemon: `AxiOwlIdeDaemon`
Located in `ide-daemon/src/main.rs`:
- **Instance Management:** Acquires `Global\AxiOwlIdeDaemon-{sid}` mutex.
- **IPC Endpoint:** Connects to the core via named pipe `axiowl-ide-core-daemon-v2-{sidHash16}`.
- **Heartbeat & Capabilities:** Sends `ReadOperation::CapabilitiesGet` every 15 seconds.
- **Purpose:** Acts strictly as an external supervisor, ensuring single-instance integrity and bridging to mobile pairing.

### 3.3 Why the Access Listener Must NOT be in `ide-daemon`
1. **Separation of Concerns:** `ide-daemon` is designed to be minimal (<200 lines in `main.rs`) to minimize crash surface. It must survive IDE crashes to report or recover state.
2. **COM STA & Message Loop Requirements:** Windows UI Automation requires an initialized Single-Threaded Apartment and an active message pump. Placing COM event sinks inside the named-pipe supervisor loop introduces thread blocking and deadlocks.
3. **Heavy Dependency Footprint:** The Access Listener requires HTTP/WebSocket serving (`axum`, `tokio-tungstenite`), token generation, and tree formatting. Injecting these into `ide-daemon` breaks the minimalist design contract pinned in `release/shared-daemon/protocol.lock.json`.
4. **Security & Privilege Boundaries:** `ide-daemon` runs with user SID validation and strict ACLs. An accessibility listener interacts with interactive desktop windows and may expose loopback APIs; keeping them separate prevents privilege leakage.

### 3.4 Established Sidecar Patterns in AxiOwl-Desktop
AxiOwl already implements clean patterns for auxiliary services:
- **`RemoteDesktopManager` (`src-tauri/src/remote_desktop.rs`):**
  - Manages `axiowl-remote-desktop.exe` as an app-owned child process on a loopback TCP port.
  - Controls lifecycle via stdin/stdout JSON handshake.
  - Controlled via `remote_desktop_status` and `remote_desktop_set_enabled` commands.
  - Dedicated React settings panel in `src/components/SettingsPage/RemoteDesktopSettings.jsx`.
- **`tool-mcp` and `browser-mcp`:**
  - Dedicated standalone crates in `crates/` compiled with `rmcp` to expose MCP tools to external agents without bloating the main executable.

---

## 4. Architectural Plan for the Access Listener

The Access Listener will follow AxiOwl's modular architecture:

```
+-----------------------------------------------------------------------------------+
|                                  AxiOwl-Desktop                                   |
|                                                                                   |
|  +--------------------+         +--------------------+     +-------------------+  |
|  |   React Frontend   |         |  Tauri IDE Core    |     |   Existing Daemon |  |
|  |  AccessListener    |<=======>|  AccessManager     |     |  AxiOwlIdeDaemon  |  |
|  |    Settings        |  IPC    |  (src-tauri)       |     | (SUPERVISOR ONLY) |  |
|  +--------------------+         +---------+----------+     +---------+---------+  |
|                                           |                          |            |
|                                           | (Spawns / Manages)       | Named Pipe |
|                                           v                          v            |
|                       +-----------------------------------+  [axiowl-ide-core-    |
|                       |  NEW: AxiOwl Access Listener      |   daemon-v2-...]      |
|                       |  (crates/axiowl-access-listener)  |                       |
|                       +-----------------+-----------------+                       |
|                                         |                                         |
+-----------------------------------------|-----------------------------------------+
                                          |
          +-------------------------------+-------------------------------+
          |                               |                               |
          v                               v                               v
  +---------------+               +---------------+               +---------------+
  |  HTTP & WS    |               |  MCP Server   |               | Reader-       |
  |  REST API     |               | (stdio / IPC) |               | Converter     |
  | (Port 8765)   |               |  axiowl_access|               | (SSH / CLI)   |
  +---------------+               +---------------+               +---------------+
          |                               |                               |
          +-------------------------------+-------------------------------+
                                          |
                                          v
                              +-----------------------+
                              | Coding Agents / LLMs  |
                              | (Goose, Claude Code,  |
                              |  Copilot, AxiAccess)  |
                              +-----------------------+
```

### 4.1 Component Breakdown

#### Component 1: `crates/axiowl-access-listener` (New Crate)
- **Role:** The core engine library and standalone binary.
- **Key Modules:**
  - `engine/windows.rs`: Native Windows UI Automation COM tree walker, cache requests, event sinks.
  - `engine/macos.rs`: macOS Accessibility (AXUIElement) provider.
  - `engine/linux.rs`: Linux AT-SPI D-Bus provider.
  - `diff.rs`: Structural tree differ computing added/removed/modified nodes.
  - `reader.rs`: Formatters producing XML, Markdown, and unified diffs.
  - `mcp.rs`: Model Context Protocol stdio server handler.
  - `server.rs`: Axum-based HTTP REST (`/api/snapshot`, `/api/diff`, `/api/events`, `/api/action`) and WebSocket server (`/ws`).
  - `main.rs`: Multi-mode CLI entrypoint (`serve`, `mcp`, `query`, `reader`).

#### Component 2: `src-tauri/src/access_listener.rs` (App-Managed Service)
- **Role:** Manages the listener process from inside the AxiOwl IDE desktop runtime.
- **Structure:**
  ```rust
  #[derive(Default)]
  pub struct AccessListenerManager {
      inner: Mutex<ListenerState>,
      operation: tokio::sync::Mutex<()>,
      shutting_down: AtomicBool,
  }
  ```
- **Lifecycle Integration (`desktop_bootstrap.rs`):**
  - Registered in `AppContext`: `app.manage(AccessListenerManager::default())`.
  - Spawns background supervisor in `services()`: checks user preferences, starts listener if enabled.
  - Clean shutdown hook in `shutdown()`: terminates listener process safely.

#### Component 3: Tauri Commands & Command Surface
- **New Commands (`src-tauri/src/commands.rs`):**
  - `access_listener_status`: Returns `{ enabled, running, port, version, error }`.
  - `access_listener_set_enabled`: Toggles state and saves preference to `app_config_dir/access-listener.json`.
  - `access_listener_snapshot`: Returns on-demand accessibility snapshot for a window or full screen.
  - `access_listener_diff`: Returns changes since last snapshot.
- **Registration:** Added to `src-tauri/src/desktop_command_surface.txt` and dispatched via `src-tauri/src/commands.rs`.

#### Component 4: Built-in MCP Tool Integration (`axiowl_access`)
- **Integration in `src-tauri/src/mcp_management/launch.rs`:**
  - Auto-configures `axiowl_access` in MCP runtime manifests alongside `axiowl_browser` and `axiowl_ide`.
  - Tools exposed:
    1. `access_snapshot(target, tier)`: Capture live window or desktop UI.
    2. `access_diff()`: Return delta changes since previous query.
    3. `access_query(selector)`: Find element by role, name, or automation ID.
    4. `access_action(element_id, action)`: Click or type into an accessible element.

#### Component 5: React Settings UI (`AccessListenerSettings.jsx`)
- **Location:** `src/components/SettingsPage/AccessListenerSettings.jsx`
- **Features:**
  - Real-time status badge (`Running`, `Starting`, `Stopped`).
  - Active loopback port display (`127.0.0.1:8765`).
  - One-click Start/Stop toggle button.
  - Live inspection test preview for developers.

---

## 5. Detailed Implementation Phases

### Phase 1: Workspace Crate Scaffolding (`crates/axiowl-access-listener`)
1. Create `crates/axiowl-access-listener/Cargo.toml` with dependencies:
   - `windows` (Win32_UI_Accessibility, Win32_System_Com, Win32_Foundation)
   - `tokio`, `axum`, `tokio-tungstenite`, `rmcp`, `serde`, `serde_json`, `clap`.
2. Port and adapt the native UIA COM engine from `AxiAccess` (`engine.rs`).
3. Add the 5-tier serialization system and tree diff algorithm.
4. Add the Axum HTTP + WebSocket streaming server.
5. Add the stdio MCP server mode.

### Phase 2: Core IDE Backend Integration
1. Implement `src-tauri/src/access_listener.rs` modeling `RemoteDesktopManager`.
2. Add commands to `src-tauri/src/commands.rs` and update `desktop_command_surface.txt`.
3. Hook `AccessListenerManager` into `desktop_bootstrap.rs` (start in `services()`, clean up in `shutdown()`).
4. Update `src-tauri/Cargo.toml` to reference the new crate.

### Phase 3: MCP Server Registration
1. In `src-tauri/src/mcp_management/launch.rs`, register `axiowl_access` as an available MCP tool provider.
2. Expose tools to Claude, Copilot, Goose, and Cursor agent profiles.

### Phase 4: Frontend UI
1. Create `src/components/SettingsPage/AccessListenerSettings.jsx`.
2. Register the component in `SettingsPage.jsx` and `Sidebar`.

### Phase 5: Verification & Packaging
1. Write focused tests for UIA element extraction, diff calculation, and serialization.
2. Verify that `ide-daemon` remains completely isolated and untouched.
3. Update `release-windows-ide.ps1` to include `axiowl-access-listener.exe` in packaging targets.

---

## 6. Verification & Quality Gates

| Check | Requirement | Verification Method |
|---|---|---|
| **Daemon Isolation** | Zero modifications or references to `ide-daemon/` | `git diff -- ide-daemon/` must be completely empty |
| **COM Safety** | All UIA calls execute on STA threads with message pumps | Rust unit test running concurrent snapshot calls |
| **Token Efficiency** | `glance` tier returns < 500 tokens; `diff` returns empty on no change | Token count benchmark on IDE main window |
| **MCP Compliance** | Tool list and call responses conform to JSON-RPC 2.0 / MCP spec | Verification with MCP inspector and Goose/Claude clients |
| **Clean Shutdown** | Terminating IDE cleans up listener process without orphaned handles | Process inspection post-exit (`Get-Process`) |

---

*Plan formulated for review and execution.*
