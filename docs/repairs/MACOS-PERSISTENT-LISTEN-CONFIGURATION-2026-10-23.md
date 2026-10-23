# Mac sender: one authoritative persisted listen address

Date: October 23, 2026, America/Los_Angeles. Narrow correction requested during the coordinated release follow-up. No app/OS input automation or TCC modification is part of this correction.

## Five documents read before the correction

Actual full contents were read, not just filenames or search excerpts:

1. [Approved sender/IDE/MCP implementation plan](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/AXIACCESS-SENDER-IDE-MCP-IMPLEMENTATION-PLAN-2026-10-22.md): sender settings and identity must survive upgrade/reinstall and graphical-session startup; explicit configuration must not be silently replaced by a different binding.
2. [AxiAccess README](../../README.md): private per-user configuration is separate from provider credentials; normal sender startup and explicit one-run CLI overrides are different operations.
3. [Sender protocol](../SENDER-PROTOCOL.md): the new Config.listen contract makes `configure --listen` persistent, while `sender --listen` is intentionally temporary; ordinary login startup must use stored config.
4. [Previous Mac implementation/installation record](MACOS-NATIVE-AX-AND-SENDER-PACKAGING-2026-10-22.md): established stable signed application identity, private sender namespace, opt-in ServiceManagement startup, no provider/daemon changes and exact prior b55 artifact/evidence. It prevents replacing that proven artifact's source claims with later work.
5. [Linux native sender/packaging record](../LINUX-NATIVE-ATSPI-AND-SENDER-PACKAGING-2026-10-22.md): the same standalone sender config serves another OS's graphical-session startup. The cause is in the Mac UI/login adapter, so shared auth/config/identity behavior should not be rewritten to compensate.

The existing app-owned [remote-runtime contract](https://github.com/morganross/AxiOwl-Desktop/blob/main/tools/remote-desktop/README.md) was also read: stop only an owned child and preserve an independently managed process/configuration boundary.

## Evidenced cause

In published source b55df997, SenderApp.swift stored its chosen endpoint in UserDefaults and always launched `sender --listen <UI value>`. LoginHelper.swift independently read a UserDefaults endpoint, defaulted to127.0.0.1:8765, and passed another explicit `--listen`. That was a separate store from the newly implemented Config.listen. A CLI `configure --listen` could therefore be overridden on the next GUI/login start, and a GUI choice did not become the CLI's stored address. This is an actual source-contract contradiction; no GUI click or observed runtime failure is invented as evidence.

## Code change

Only the Mac setup/control adapter changes:

- UI renders immediately and reads the public `axiaccess status --config ...` metadata asynchronously to seed the endpoint. It does not read/deserialize the secret-bearing config itself. User edits are not overwritten by a late response.
- Before GUI start, idempotent quiet setup runs only when needed, then the native `configure --listen` command persists the chosen address. GUI sender launch passes only `sender --config ...` and therefore uses the same Config.listen as the CLI.
- Before enabling user login startup, the UI stops only its exact owned child, persists the address through the same native command, and registers the existing ServiceManagement agent.
- LoginHelper passes no address override and does not read UserDefaults. Normal login startup uses the shared private config.
- CLI command output that could contain setup credentials is discarded; only public status JSON is captured transiently to obtain listen. Native configuration code retains its ownership/0600 validation and atomic replacement without changing the existing key, sender identity, paired clients or certificate.

The b55 signed/notarized installer and its install/reinstall receipts remain valid for that exact historical source. This follow-up is not retroactively described as shipped in b55. No daemon, provider credential/history, native AX read engine or other OS's configuration implementation is changed.

## Verification

Both changed Swift files passed actual `swiftc -typecheck -target arm64-apple-macos13.0` on the Mac, without diagnostics. The check used an isolated owned `.cache/port-persistence-check` folder; the clean b55 producer checkout and its signed artifact were not modified/relabelled. The coordinated updated-source canonical release remains pending. No GUI click/type test, user login-item registration, Accessibility grant or TCC workaround was performed. Native persistent-configuration runtime behavior and installed GUI acceptance will be reported separately, after actual execution with the updated binary.
