# Product documentation contract reconciliation

Date: October 22, 2026, America/Los_Angeles.

Scope: AxiAccess README and superseded historical integration-plan notices. This is a documentation correction, not proof of installed native reads, signed installers or framework acceptance.

## Documents read before editing

The actual full contents of these distinct repository documents were read before the correction. The nearly duplicated historical proposal in AxiAccess and AxiOwl-Desktop counts as one logical document, not two.

1. The previous `README.md`: documented Python-backed Mac/Linux engines, plaintext HTTP examples, old reader commands, estimated token costs and an unbounded/100% full-text claim. These are the precise claims that required replacement.
2. [Historical listener proposal](../../AXIOWL-ACCESS-LISTENER-INTEGRATION-PLAN.md): proposed a copied IDE engine, STA UIA, action/click/type tools, old endpoints and native-event assumptions. Its daemon-isolation intention remains valid; conflicting details are not current instructions.
3. [Approved final sender/IDE/MCP plan](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/AXIACCESS-SENDER-IDE-MCP-IMPLEMENTATION-PLAN-2026-10-22.md): defines engine ownership, native independent senders, remote/local/VM targets, no actions, unchanged daemon, bounded/partial results, scoped all-framework MCP and preservation boundaries. It remains an implementation plan, not rewritten as an acceptance report.
4. [Sender protocol](../SENDER-PROTOCOL.md): supplies current setup/private bootstrap/TLS/pairing semantics, selected-session requirements, explicit target/observation/buffer/cursor identities, actual operations, resource bounds and the local-only standalone MCP distinction.
5. [Windows packaging record](../WINDOWS-SENDER-PACKAGING-2026-10-22.md): establishes per-user MSI installation, owned startup/helper identity, configuration retention, real target/toolchain floor and the difference between precommit compilation and final signed installed proof.
6. [Linux native/packaging record](../LINUX-NATIVE-ATSPI-AND-SENDER-PACKAGING-2026-10-22.md) and [Linux package operation](../../packaging/linux/README.md): establish native AT-SPI, actual graphical bus/session, no Python runtime/global accessibility writes, explicit user startup, offline package contents and preservation.
7. [Mac native/packaging record](MACOS-NATIVE-AX-AND-SENDER-PACKAGING-2026-10-22.md): establishes native AX, arm64/macOS26, stable signed identity, explicit login startup and truthful observed permission denial. Its compilation receipts are not silently promoted to permission or text-read acceptance.
8. [IDE MCP framework configuration](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/MCP-FRAMEWORK-CONFIGURATION.md): establishes the eight launch consumers, per-run native overlays, separate credential storage, next-connection settings semantics and no claim that an enabled entry connected successfully.

The actual `src/main.rs`, `src/mcp.rs`, `src/transport/{client,config}.rs`, Cargo manifest and all canonical release entrypoints/session helpers were also inspected to verify flags, package names and behavior. Source listings are additional evidence, not document-reading credits.

## Evidenced cause

The previous product README described the earlier prototype rather than the current native 2.1.0 implementation. Its remote examples omitted the now-required certificate/token pairing, its Rust examples implied old plaintext HTTP/dashboard availability, and its full mode promised unbounded/100% text coverage. The historical proposal still read as an active instruction to copy the engine into the IDE, use STA on Windows and add actions, despite the human's later observation-only final plan.

This creates a concrete integration risk: a developer/operator following those documents would choose the wrong runtime dependencies, endpoint/authentication model, OS threading boundary or tool surface. The correction therefore reconciles written instructions with current code rather than adding new restrictions or asserting unperformed tests.

## Change

- Replaced `README.md` with the native sender/IDE listener boundary, supported initial OS targets, independent installer operation, identity/pairing preservation and explicit privacy/partial-coverage limitations.
- Documented current `setup`, `status`, `pair`, `sender`, authenticated `serve`, `query`, `reader` and `mcp` forms from the actual CLI. Remote reader examples require a public certificate file and a privately supplied token environment variable; no token literal is placed in command arguments.
- Distinguished the standalone operator CLI's SSH configuration support from the IDE listener's isolated explicit-configuration SSH tunnel. Both forward to an already-running graphical sender; neither tool issues a remote command or bypasses host trust.
- Documented scoped baselines/text pages, honest limits, no `full`-means-unbounded claim and connected transport versus native-read readiness.
- Documented all eight IDE MCP consumers as source wiring, not installed conversation proof. Standalone MCP remains local-only; no persistent pairing secret belongs in agent environment/configuration.
- Prepended a clearly superseded notice to the AxiAccess historical proposal while retaining its historical body. The matching IDE historical notice is supplied to the clean publication assembler from the verified GitHub-main document because the old local checkout lacks that file; its unrelated dirty Settings/source changes are not copied into the historical document.
- Did not modify the approved final plan, protocol implementation, runtime code, daemon, provider/chat data or credentials as part of this documentation task.

## Compatibility and OS boundaries

These docs describe Windows x86_64, Linux x86_64 and Mac arm64/native APIs. They do not add Intel-Mac, arbitrary Linux-distribution qualification, secure-desktop access, universal toolkit text coverage, global accessibility activation or remote UI control. Old Python/demo files are historical compatibility/reference material outside the production package, not deleted or silently run by these instructions.

Installer version/source/signature evidence, native accessibility success and GUI/framework observations remain independently recorded. Current Mac permission denial is an accurate state, not successful inventory. Deleting local IDE pairing is distinguished from sender-side revocation. Sender/private pairing stores, native provider credentials/chats and signing material retain their existing ownership.

## Actual verification

Performed static reconciliation against the current CLI/transport/MCP/release sources and the documents above. Checked README local Markdown links and historical-body preservation with a small read-only script. No network connection, package build, installation, observed-app action or model request was performed by this documentation task.

The related Settings source checks are recorded separately in the IDE's `docs/reports/AXIACCESS-SETTINGS-IMPLEMENTATION-2026-10-22.md`; they do not prove that these README workflows passed on an installed system.
