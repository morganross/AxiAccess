# Shared sender, reader and MCP repairs

Date: October22,2026 (America/Los_Angeles). This record covers the shared
AxiAccess implementation; native adapters and installers have separate records.
No AxiOwl daemon, provider history, provider authentication or signing files are
changed by this implementation.

## Documents read before these repairs

These seven distinct documents were read in full before changing shared product
code. The repeated old plan in the two repositories counts as one document.

1. `README.md` at the original AxiAccess main: defines all/query/reader semantics,
   verbosity levels, Python launch dependency and legacy MCP tool contract.
2. `AXIOWL-ACCESS-LISTENER-INTEGRATION-PLAN.md` at original main: prior reader,
   diff, HTTP/MCP and isolation assumptions. Its STA/actions/implementation claims
   are historical proposals, not evidence that those features existed.
3. AxiOwl-Desktop `docs/AXIACCESS-SENDER-IDE-MCP-IMPLEMENTATION-PLAN-2026-10-22.md`:
   approved specific repair list, source identity, per-client capture scopes,
   text/transport budgets, native-session isolation and all-framework consumers.
4. AxiOwl-Desktop `docs/MCP-FRAMEWORK-CONFIGURATION.md`: actual eight-framework
   delivery, native schemas, preservation of unrelated settings, credential
   storage and reconnect boundaries. This informed the separate stable sender
   protocol rather than exposing remote pairing secrets to framework configs.
5. AxiOwl-Desktop `docs/IDE-ORCHESTRATION.md`: authenticated run-scoped bridge,
   cancellation, correlation, warm-worker lifecycle, bounded outputs and typed
   tool results. This informed per-client capture ownership and exact child
   worker cancellation rather than reusing a cancelled native response.
6. AxiOwl-Desktop `ide-ipc/README.md`: bounded framing before allocation,
   correlated errors, authenticated identity versus caller-asserted roles,
   epoch changes and explicit resynchronization. Its protocol/daemon was not
   modified or reused as the observation relay.
7. AxiOwl-Desktop `tools/remote-desktop/README.md`: app-owned process lifecycle,
   private bootstrap and offline native payload preservation. It informed exact
   child ownership rather than stopping independently installed senders.

Each repair below uses at least five of those documents for its specific
contract. The shared foundation is not a claim that reading one broad checklist
proves platform-native runtime behavior.

## Repair records

| Issue | Evidenced root cause | Code change | Direct reading references |
| --- | --- | --- | --- |
| AA-01 reader | `main.rs` executed relative Python reader scripts and relied on working directory/interpreter | Native Rust reader, typed records and deliberately configured TLS/SSH forwarding; Python compatibility sources retained but not a production runtime dependency |1,2,3,4,7|
| AA-02 all-target detail | Legacy `target=all` selected glance regardless of requested detail without explaining it | Inventory remains cheap; effectiveDetail=glance plus explicit detailNotice; selected-window reads use exact references |1,2,3,4,5|
| AA-03 reader authority | Legacy reader reconstructed structured facts from formatted `llm_text` | JSON is authoritative; XML/Markdown serialize those same records and do not infer active editor/shell ownership |1,2,3,4,6|
| AA-04 shared history | Legacy global home-directory state mixed machine/target observations | Bounded in-memory captures keyed by sender epoch, exact window/detail and authenticated clientScope; no legacy cache overwrite |1,2,3,5,6|
| AA-05 stale selection | Per-walk IDs and loose handle/title selection could identify another window | Opaque epoch/native-lifetime refs; strict nativeID required; unknown/ambiguous legacy target rejects; no fallback to a different app |1,2,3,5,6|
| AA-06 error masking | Defaults/empty inventories could conceal native failures | Result-based native worker, typed errors, partial inventory issues and actual capability state |1,2,3,4,6|
| AA-08 bounds/cancellation | Native blocking operations and input/output had no whole-operation containment | Bounded ingress, queue32, one isolated worker,3s native deadline, exact child kill-on-drop, correlated worker IDs, capped captures/pages/results |1,2,3,5,6,7|
| AA-09 changes | WebSocket clients each scanned all windows repeatedly; legacy diff lacked stable structural identity | Native event clock wait plus on-demand typed structural comparison, parent/order/state/native role, full captured text fingerprints; stale/partial baselines resynchronize |1,2,3,5,6|
| AA-11 network authority | Old HTTP router shared observation with wakeup/demo actions and had no authentication | TLS-only sender with paired per-client bearer credentials and exact trusted leaf; private stdio bootstrap for IDE-owned child; no mutation/demo routes; immediate config-based revocation |1,2,3,4,5,6,7|
| AA-13 text paging | Large inline text and preview-based dedup made truncation and identity ambiguous | Separate buffer refs, immutable Unicode-safe pages/cursors, retained content budget and explicit truncation; equal-prefix buffers retained separately |1,2,3,4,5,6|
| AA-14 serialization | Manual XML/YAML-like interpolation treated app strings as markup | Typed serde JSON plus escaped XML text; app content explicitly untrusted; no CDATA delimiter injection |1,2,3,4,6|
| AA-15 MCP | Handwritten tools/call ignored requested name and had minimal lifecycle handling | RMCP3.5.1, strict named tools/closed arguments, bounded stdio before parser allocation, cancellation and read-only annotations |1,2,3,4,5,6|
| Persistent remote startup | CLI-only listen override would be lost when installers restart sender at login | Defaulted private Config.listen and configure--listen atomically preserve selected address alongside identity; sender default loads it, explicit runtime override remains available |1,2,3,6,7|

Native AA-07/10/12 are implemented by the platform owners. The shared layer
preserves nullable/native metadata, excludes protected buffers, and includes
native role/state in comparisons; those facts do not replace native extraction
verification.

## Authentication and cancellation details

Remote TLS authenticates the imported exact certificate, not a guessed hostname.
The fixed certificate SAN`axiaccess.local` remains subject to normal TLS
signature/validity/name checks while the configured machine address selects TCP
routing. No public roots, HTTP redirect, ambient proxy or invalid-cert bypass.
The IDE separately checks senderId and retains trust in its AxiAccess namespace.

Pair/revoke updates private config atomically; the TLS handler rereads it before
every request. Revoking the last client is allowed and disables network reads.
Unix permissions are0700/0600; Windows setup installs an owner-only protected
DACL and credential reads verify the owner-only file ACL. No keys are included
in installers or native provider configuration.

A native worker is removed from the reusable slot while a read is active.
Dropping a cancelled read drops/kills that exact child. Request IDs are checked
before accepting results. A restarted worker changes epoch and invalidates live
window refs rather than consuming leftover output as a new request's result.
Fresh change captures evicted by concurrent retention pressure return a typed
expired-observation result rather than panicking at an unchecked map lookup.

## Compatibility and OS boundaries

- Shared observer/sender/reader/MCP is Rust on Windows x86_64, Linux x86_64 and
  Mac arm64. OS calls remain in the relevant platform adapter.
- Existing `query`, `--cli`, `--mcp`, `mcp`, verbosity names and
  `observe_desktop` remain, with truthful bounded output.
- `serve` now requires setup and uses authenticated TLS: unauthenticated HTTP
  clients must deliberately pair. Mutating wakeup/demo endpoints are removed
  from the shipped native router; this is intentional read-only scope.
- Reader retains `--ssh`, `-p/--port`, `-i/--identity`, targeted observations and
  `--diff`. SSH forwards only to the existing selected graphical sender; it
  never starts an arbitrary remote shell observer. Remote reader additionally
  requires the paired certificate/token. Baselines are explicit/scope-bound;
  the old unscoped on-disk heuristic cache is not reused.
- Framework process configuration remains owned by the IDE's all-framework MCP
  integration. Standalone native MCP deliberately exposes only local; agents
  cannot pass arbitrary endpoints or credentials.
- Native events are invalidation hints, not proof of complete app event
  coverage. Changes reconcile selected bounded content after waiting; no idle
  whole-desktop polling.

## Verification actually performed

- Local Windows GNU`cargo check --all-targets` passed on the final shared2.1.0
  source including native Windows ACL APIs, lazy text hydration and state fields.
  This is compilation, not native MSVC installer acceptance.
- Local`cargo test --lib`:14 passed,0 failed. Covers Unicode boundary paging,
  closed MCP schemas/unknown tools, pre-parser size bounds, client/cursor capture
  isolation, partial frame rejection, structural changes/XML escaping and the
  Windows adapter's three focused identity/text tests. Additional checks prove
  equal-prefix/equal-length duplicate text buffers cannot exchange content
  unnoticed and native checked-state/order changes enter the structural delta.
- One intermediate unit run had11passes/1failure: an authorization test expected
  `invalid_cursor`, but lazy hydration now rejects an unauthorized owner earlier
  with`expired_observation`. The assertion now requires rejection without
  prescribing which private identifier is checked first. No access control was
  weakened; the final14-test rerun passed.
- Native Windows debug executable exercised actual private config setup/status,
  owner-only ACL verification, idempotent identity retention, quiet secret
  redaction, private stdio bootstrap, valid capability/invalid-token rejection,
  unknown action rejection, and real RMCP initialize/tool discovery/unknown-tool
  rejection. Sanitized operator receipt:
  `output/axiaccess-implementation-20261022/core-native-acceptance.json` in the
  parent workspace. This does not claim a real application text read.
- Final native binary also exercised`configure --listen` with a fresh loopback
  port, compared all identity/certificate/private-key/client fields before and
  after, reran quiet setup byte-for-byte, then launched`sender --config` without
  any listen override. It bound the stored port and completed an authenticated
  TLS capabilities request using hostname validation and only the paired root.
  Native private ACL reads and Windows atomic replacement passed. The same
  sanitized receipt contains these checks; no credentials or app text is saved.
- Root coordinator reports separate actual Windows TLS capability/inventory,
  rejection and revoke receipts. Those receipts are not reproduced here as
  three-OS installation or all-framework GUI proof.

Pending completion evidence: actual signed native installers, per-OS real accessibility reads,
IDE integration and actual all-framework GUI calls. Root delivery record must
keep source/configuration checks separate from those installed results.
