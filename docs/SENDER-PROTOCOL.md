# AxiAccess sender protocol v1

Product direction: sender -> IDE listener -> agent. Agent tools can observe only;
no input, focus, scrolling, app launch, arbitrary file or command operations exist.

## Process modes

`axiaccess setup --quiet` creates an idempotent per-user identity and certificate.
`axiaccess sender` serves TLS at the stored address (initial127.0.0.1:8765).
`axiaccess configure --listen IP:PORT` persists an explicit LAN/VM interface
without changing keys/clients; restart is required for a bind-address change.
`sender --listen IP:PORT` is an explicit one-run override. Normal login startup
uses the stored address. `serve --port PORT` remains an authenticated TLS alias.
`axiaccess status` describes configuration, not a claim that a service is running.
`axiaccess pair NAME` issues a client; `pair NAME --revoke` revokes it. The sender
reloads private pairing configuration before each request. Empty client lists
disable network reads without destroying the sender identity.

Default configuration:
- Windows `%LOCALAPPDATA%/AxiAccess/sender.json`.
- Mac `~/Library/Application Support/AxiAccess/sender.json`.
- Linux `${XDG_CONFIG_HOME:-~/.config}/axiaccess/sender.json`.

The config contains private key and bearer credentials; Unix directory0700/file0600,
Windows owner-only product directory. Never put it in evidence, provider config,
agent environment, installer or repository. `setup` without `--quiet` and `pair`
print the deliberate pairing secret only to the operator's console. Quiet setup
returns only public metadata. Reinstall never regenerates an existing identity.

## IDE-owned private sender

Launch the selected installed `axiaccess sender --stdio` executable with private
stdin/stdout handles. Within10seconds send one bounded JSON line:

```json
{"type":"bootstrap","protocolVersion":1,"token":"<48..256-byte random secret>","clientId":"<IDE-instance-id>"}
```

The first response is:

```json
{"type":"ready","protocolVersion":1,"senderId":"...","epoch":"...","version":"2.1.0","sourceRevision":"...","platform":"windows|linux|macos","pid":123}
```

This sender is scoped to the owned pipe lifetime. It does not reuse another
independently installed sender or long-lived remote credentials. EOF stops it;
stuck native reads are in an exact child worker with kill-on-drop ownership.

## Shared request/response

```json
{"id":"request-id","token":"<stdio capability only>","clientScope":"run-scope","operation":"windows","params":{}}
```

`clientScope` is optional (default`default`), bounded1..128bytes; IDE must supply
a distinct value for each authorized agent/run. Retained captures, text cursors
and change baselines cannot be read across authenticated client/scope boundaries.
The agent never chooses its long-lived credential or destination URL.

```json
{"id":"request-id","result":{}}
{"id":"request-id","error":{"code":"stale_reference","message":"...","retryable":false}}
```

Exactly one result/error. Frames are UTF-8 JSON with newline delimiters over
stdio, maximum256KiB including newline. Unsupported fields/operations reject.

## TLS transport

POST`/v1/read`, same JSON request/response, headers`Authorization: Bearer TOKEN`
and`X-AxiAccess-Client: NAME`. The sender rejects Origin-bearing browser requests.
No plaintext fallback, redirect, demo/wakeup, action or arbitrary URL endpoint.

Pairing conveys senderId, the exact certificate PEM and SHA256fingerprint,
clientId and token. The certificate includes the fixed SAN`axiaccess.local`.
The IDE connects TCP to the configured hostname/IP, but validates TLS using
serverName`axiaccess.local`, trusting only the imported certificate. For reqwest,
resolve that fixed name to the configured address and disable proxies and public
roots. Retain signature/validity/hostname checks; never accept-invalid-certs.
Verify the returned senderId as well. This supports public/private/LAN/VM
addresses without pretending that a DNS name is the authenticated identity.

SSH is an optional tunnel to the already-running graphical-session sender's TLS
endpoint. It does not launch observation in a random SSH desktop or issue agent-
supplied shell commands. Host authentication remains SSH's responsibility.

## Operations

| Operation | Params | Result |
| --- | --- | --- |
| `capabilities` | `{}` | ready identity plus native permission/capability state and applied limits |
| `windows` | `{maxWindows?:1..250}` | sender/epoch/observation/time, windows`[{windowRef,native}]`, completeness/issues |
| `inspect` | `{windowRef,detail?,maxNodes?:1..1000,maxDepth?:1..64}` | retained observation/tree and text-buffer descriptors |
| `read_text` | `{observationId,bufferRef,cursor?,maxBytes?:4..16384}` | immutable text page, UTF-8 byte range, nextCursor, capturedBytes/truncation |
| `changes` | `{windowRef,detail?,baseline?,waitMs?:0..30000}` | new baseline/observation, added/removed/modified, textChanged or resyncRequired |

Detail is`compact|full|interactive|text_only`; interactive lists controls only.
Window references are deterministic within one sender/native-worker epoch and
bind native process/window lifetime. Repeated inventories do not manufacture new
references. Worker restart invalidates live references. Native platform IDs are
not agent-callable target selectors. Ambiguous legacy title/PID queries reject.

Compact/interactive inspection requests native previews/metadata only and sets
`textDeferred:true`. First explicit read_text captures bounded full text from
the exact retained native buffer identity and records`textCapturedAtMs`, which
can be later than the original hierarchy observation. It does not pretend the
deferred text was captured earlier. Subsequent pages refer to that immutable
capture, not a live document assembled across revisions. A disappeared source
returns resync_required rather than selecting a similar control.
Distinct equal-length/same-prefix buffers remain separate.
Captured full text fingerprints participate in changes, not just previews.
Changing structural parent/order participates in modified records.
Partial or unidentified trees cannot establish a complete no-change result.

## Current resource boundaries

One active isolated native worker,32queued reads,3-second native deadline;
worker replacement never restarts observed apps. Native traversal bounded by
OS adapter; shared retained capture budget16MiB/32captures with explicit eviction.
Text page16KiB; total capture text2MiB with per-buffer64KiB starting cap;
transport256KiB. Compact inspection asks the native adapter for bounded
preview/metadata; explicit text reads hydrate only the selected bounded window's
matching native buffers. Changes explicitly captures bounded comparison text.
No whole-desktop polling when idle. Changes can wait for native-event invalidation
using backend condition variables (250ms maximum worker lease per wait segment),
then capture bounded actual state. A native event outside the target may wake the
wait; the final scoped comparison decides changes. Unsupported/lost native events
remain possible; bounded wait expiration performs reconciliation rather than
claiming that no desktop activity occurred. Backend event readiness is reported,
not invented from polling.

Off-screen text is application-dependent. No operation realizes virtualized
controls, changes focus, scrolls or activates accessibility globally. Protected
native fields are filtered by adapters before extraction; ordinary documents
can still contain secrets. Routine output/logs must not contain pairing secrets
or captured private app text.

## MCP

Native standalone`axiaccess mcp` uses RMCP3.5.1 with bounded stdio frames,
initialize/notification/cancellation handling and strict named tool dispatch.
Its six access tools plus compatibility`observe_desktop` expose local only.
The IDE's `axiowl_access` bridge exposes configured permitted remote/local/VM
targets across all frameworks; agent capability is separate from pairing token.
Source wiring and deterministic tests are not installed GUI/provider acceptance.
