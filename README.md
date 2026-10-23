# AxiAccess

**Read-only information about apps and windows, locally and on remote computers or VM guests.**

AxiAccess reads window inventories, accessibility hierarchies, readable control
states and text exposed by Windows UI Automation, macOS AXUIElement and Linux
AT-SPI. The production sender uses native Rust adapters. It needs no screenshots,
Python, Node, provider CLI, browser runtime or full IDE.

It does **not** click, type, press keys, focus windows, scroll, launch applications,
execute agent-provided commands or read arbitrary files. Observation does not
change global accessibility settings. Treat captured app text as untrusted data,
not instructions.

Version **2.1.0** adds the native sender/protocol and independent installers.
Source wiring, deterministic tests, native compilation, signed packages and real
installed observations are different evidence levels. See the
[verification records](#verification); this README does not establish acceptance.

## Architecture

| Component | Where | Responsibility |
| --- | --- | --- |
| Sender | Selected graphical user on a computer or VM guest | Native reads, bounded captures, identity, pairing and encrypted transport |
| IDE listener | Computer running AxiOwl IDE | Configured targets, trust, connections, Settings previews and agent authorization |
| `axiowl_access` MCP | Permitted IDE frameworks | Explicit target-scoped observation tools |
| Standalone `axiaccess mcp` | Selected local graphical session | Local-only MCP for deliberate external configuration |

Install the sender **inside a VM guest** to observe guest apps; reading the host's
VM-viewer window is different. The independent AxiOwl daemon is not an AxiAccess
host or relay. The engine remains in this repository, not a copied IDE engine.

## Native targets and coverage

| Target | API | Requirements and limits |
| --- | --- | --- |
| Windows x86_64 | UI Automation, cached properties, TextPattern | Windows 10 / Server 2016 toolchain floor; interactive user desktop. No secure-desktop, other-user or protected/elevated-app access promise. |
| Linux x86_64 | AT-SPI2 over D-Bus, native Rust `zbus` | Selected graphical session and accessibility bus. X11/Wayland toolkit coverage varies; ordinary system libraries remain prerequisites. |
| Mac arm64 | ApplicationServices/AXUIElement, CoreFoundation, AppKit | Current package targets macOS 26. Accessibility permission belongs to the installed signed sender. No Intel-Mac package claim. |

Off-screen text is readable only when an application exposes it. Virtualized,
unloaded, custom-drawn or inaccessible content may be absent. No operation
realizes controls, scrolls or focuses an app to manufacture completeness.
Unavailable geometry/visibility/state remains unknown.

Adapters filter native protected fields before value/text extraction. Ordinary
readable documents can still contain secrets, and apps can mislabel content.
No universal secret-removal or complete-text claim is made.

Old Python/demo files remain compatibility/reference material in the repository;
they are not dependencies of native sender installers. Release scripts can use
development tooling such as Python, PowerShell or Swift without shipping those
runtimes in the sender.

## Install and operate the sender

| OS | Native offline artifact | Installed entrypoint/session controls |
| --- | --- | --- |
| Windows | `AxiAccess-Sender-Windows-x64-VERSION.msi` | `%LOCALAPPDATA%\Programs\AxiAccess\axiaccess.exe`; owned `session.ps1` Start/Stop/Status/EnableStartup/DisableStartup |
| Linux | `AxiAccess_VERSION_amd64.deb` | `/usr/bin/axiaccess`; `axiaccess-session start|stop|enable|disable|status` |
| Mac | `AxiAccess-Sender-macOS-arm64-VERSION.pkg` | `/Applications/AxiAccess Sender.app`; native status/setup window, opt-in user login startup |

Windows installation is per-user, initializes configuration idempotently and
registers owned user startup. Mac login startup is opt-in. Linux does not start a
root observer; start/enable the selected graphical user's service explicitly.
See [Linux package operation](packaging/linux/README.md). Use actual release
signature/checksum/provenance receipts, not just a filename.

Upgrade/reinstall preserve sender identity and pairing. Uninstall removes owned
program/startup files and deliberately retains private user configuration. No
provider credentials, chats, signing material or daemon is installed or modified.
VM templates must generate distinct sender identities instead of copying paired
private state into several active guests.

The default encrypted endpoint is loopback. Installation does not create global
firewall rules or expose a public port. Direct remote access requires an explicit
reachable interface; VM NAT requires a deliberate forward or SSH tunnel.

## Identity, pairing and startup

Run as the selected graphical user using the installed executable, or
`./target/release/axiaccess` for a source build:

```sh
# Idempotent setup; preserve an existing identity.
axiaccess setup --quiet

# Public configuration metadata, not proof that a sender is running.
axiaccess status

# Issue/revoke an individual client credential.
axiaccess pair ide-laptop
axiaccess pair ide-laptop --revoke

# Authenticated TLS on loopback in this graphical session.
axiaccess sender

# Deliberate LAN/VM interface; substitute this machine's real address.
axiaccess sender --listen 192.168.56.10:8765

# Compatibility alias: TLS, never the old plaintext demo API.
axiaccess serve --port 8765
```

`setup` without `--quiet` and `pair NAME` deliberately print pairing secrets to
the operator. Do not publish that output or put it into evidence, prompts or
provider configuration. A pairing supplies sender ID, public certificate and
fingerprint, client ID and private token. Use separate clients for independent
consumers; rotating an existing ID requires explicit revocation first.

| OS | Private configuration |
| --- | --- |
| Windows | `%LOCALAPPDATA%/AxiAccess/sender.json` |
| Mac | `~/Library/Application Support/AxiAccess/sender.json` |
| Linux | `${XDG_CONFIG_HOME:-~/.config}/axiaccess/sender.json` |

The file contains private key and client credentials. Unix directory/file
permissions are 0700/0600; Windows uses an owner-only product directory.
`--config PATH` on management/sender commands selects another private location.

Stop the owned sender before changing its bind address. SSH access alone does not
identify or grant access to the graphical desktop: start the sender in that
desktop session, then connect to it.

HTTPS `POST /v1/read` uses per-client authentication and the paired certificate.
Clients route TCP to the configured address while validating the certificate's
fixed name `axiaccess.local` against the imported certificate. There is no
plaintext fallback, redirect following or accept-invalid-certificate option.
Changed certificates require a trust change; no demo/wakeup/action endpoints ship.

IDE-owned local observation uses private stdio bootstrap and an ephemeral
capability, not remote pairing tokens. The IDE closes only its owned child.
Disconnecting an independent sender does not stop or uninstall it.
[Full sender protocol](docs/SENDER-PROTOCOL.md).

## CLI observations and native reader

```sh
axiaccess query all --verbosity glance --json
axiaccess query "Exact unique window title" --verbosity text_only --json
axiaccess reader "Exact unique window title" --format xml
axiaccess reader "Exact unique window title" --format markdown

# AXIACCESS_TOKEN is supplied privately, never a literal argument/history value.
axiaccess reader "Exact unique window title" --url https://192.168.56.10:8765 \
  --certificate sender-cert.pem --client-id ide-laptop --token-env AXIACCESS_TOKEN \
  --format markdown

# Tunnel to an already-running graphical-session sender's loopback TLS endpoint.
axiaccess reader "Exact unique window title" --ssh user@remote-host \
  --certificate sender-cert.pem --client-id ide-laptop --token-env AXIACCESS_TOKEN \
  --sender-port 8765 --format xml
```

The PEM file is the sender's public pairing certificate. Prefer exact returned
`windowRef` values: ambiguous title/process/PID matches reject instead of choosing
another window. `all`/`desktop` returns inventory, even when detailed legacy
verbosity was requested; inspect an explicit window for text.

CLI SSH is an operator tool: it can use existing aliases, ProxyJump, identities
and SSH configuration, while forcing batch mode, strict host checking and
forward-failure handling. No remote command runs. `--port PORT` and
`--identity PATH` select SSH settings explicitly. The IDE listener's separately
scoped tunnel intentionally ignores user SSH configuration/commands/extra
forwards and uses explicit host/user/identity or the SSH agent/default identity.
Neither bypasses host trust.

`reader TARGET --diff --scope SAME_SCOPE` returns structured changes. A subsequent
remote call can resume `--baseline BASELINE` only with the same explicit scope;
expired baselines require resynchronization. Legacy unscoped cache files are not
reused. `--format diff` currently emits JSON changes, not an unrestricted file diff.

## MCP and AxiOwl IDE

A deliberate external local MCP configuration can run the native command with
`args: ["mcp"]` and an absolute executable path. Use that client's native schema.
No network pairing token is embedded. Standalone MCP exposes `targetId=local`
only, scoped to its stdio lifetime; it also retains compatibility
`observe_desktop(target, verbosity)`.

The IDE supplies **`axiowl_access`** through its existing framework launchers:
AxiCode, Codex, Claude Code, Copilot, Cursor, OpenCode, Gemini and Antigravity.
Enable the feature in Settings, configure/pair/allow machines, then connect the
framework. This is wiring coverage, not a claim that each installed framework
has passed a real read. Saved changes apply on the next connection without
interrupting an active turn or rewriting independent provider-app settings.

| Tool | Observation |
| --- | --- |
| `access_targets` | Configured permitted machines and actual last-known status |
| `access_capabilities` | Native permission/session/event coverage and limits |
| `access_windows` | Bounded inventory and lifetime-bound references |
| `access_inspect` | Selected window/subtree and text-buffer descriptors |
| `access_read_text` | Observation/buffer-bound immutable text page and continuation |
| `access_changes` | Scoped added/removed/modified records or resynchronization |

`interactive` means readable-controls detail, never an action. IDE agent tools
name configured target IDs, not arbitrary URLs/credentials/commands. Run authority
is separate from pairing; delegation cannot expand allowed machines. Deleting one
IDE's saved pairing is not remote revocation: revoke on the sender explicitly.

## Resource and failure contract

| Resource | Current starting bound |
| --- | --- |
| Native execution | One isolated worker, 32 queued reads, 3-second native deadline |
| Transport | 256 KiB frames; oversized results require smaller pages |
| Text page | Up to 16 KiB UTF-8, no split inside a Unicode character |
| Retention | 16 MiB / 32 bounded captures; evicted cursors fail explicitly |
| Captured text | 2 MiB aggregate; starting 64 KiB per-buffer cap |
| Event wait | Up to 30 seconds, cancellable, followed by scoped reconciliation |

Adapters also bound native calls, raw traversal/depth and retained output.
`full` is still bounded and may be incomplete: no fixed token cost, 100% extraction
or universal native-event claim. Compact inspection currently captures bounded
text alongside its hierarchy; fully lazy native range reads remain an optimization.

No full-desktop scanning loop runs without an observation request. Native events
invalidate state; actual scoped comparisons determine changes. Lost events,
partial trees, expired captures and changed desktop/worker epochs may require
resynchronization. Stuck native work is isolated/replaced without restarting apps.

Permission needed, absent graphical session, stale/ambiguous references, timeout
and transport failures remain explicit, not successful empty desktops. Connected
TLS is not native-read readiness. Diagnostics should retain IDs, timing, counts
and typed errors, not captured private app text or pairing secrets.

## Build and release

```sh
cargo build --locked --release
```

Canonical standalone release entrypoints, each run on its native target with the
existing approved toolchain/signing configuration:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release-windows.ps1
```

```sh
python3 scripts/release-linux.py
bash scripts/release-macos.sh
```

Receipts bind source/tree, Cargo lock, target/toolchain/recipe, dependencies,
payload size/hash and native signing/installer results. Coordinated publication
uses coherent committed source. IDE builds acquire this repo's main once per
release, cache by actual commit/target/toolchain/recipe and record that commit:
no permanent source pin. IDE offline packages include the local sender and reuse
the existing Rust MCP bridge.

## Verification

- [Protocol and bounds](docs/SENDER-PROTOCOL.md)
- [Shared sender/reader/MCP repair](docs/repairs/SHARED-SENDER-PROTOCOL-MCP-2026-10-22.md)
- [Windows native observation](docs/repairs/WINDOWS-NATIVE-OBSERVATION-2026-10-22.md)
- [Windows packaging](docs/WINDOWS-SENDER-PACKAGING-2026-10-22.md)
- [Linux native observation/packaging](docs/LINUX-NATIVE-ATSPI-AND-SENDER-PACKAGING-2026-10-22.md)
- [Mac native observation/packaging](docs/repairs/MACOS-NATIVE-AX-AND-SENDER-PACKAGING-2026-10-22.md)
- [Approved integration plan](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/AXIACCESS-SENDER-IDE-MCP-IMPLEMENTATION-PLAN-2026-10-22.md)

The [older integration proposal](AXIOWL-ACCESS-LISTENER-INTEGRATION-PLAN.md)
is historical. Its copied engine, STA UIA, action tools, unauthenticated APIs and
assumed event coverage are not current implementation instructions.

