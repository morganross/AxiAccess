# Linux native observation and standalone sender packaging

Date: October 22, 2026, America/Los_Angeles. Status: implementation in progress;
actual compilation/installer/desktop results are recorded separately below.

## Documents read before implementation

The actual contents of these five distinct repository documents were read before
the Linux adapter or packaging edits. The duplicated earlier plan counts once.

1. `../AxiOwl-Desktop/docs/AXIACCESS-SENDER-IDE-MCP-IMPLEMENTATION-PLAN-2026-10-22.md`:
   defines native AT-SPI, identity/unknown-state handling, hard bounds, standalone
   DEB and graphical-user context, preservation and no-daemon/no-actions boundaries.
2. `README.md`: establishes the legacy schema/CLI and identifies the existing
   Linux Python and AT-SPI dependency claims that must not become hidden runtime
   requirements of the new native sender.
3. `AXIOWL-ACCESS-LISTENER-INTEGRATION-PLAN.md`: supplies historical isolation and
   sidecar intent; its click/type endpoints, duplicate engine and Windows-only
   packaging are superseded by the final plan, not copied into this adapter.
4. `../AxiOwl-Desktop/ide-ipc/README.md`: incomplete reads must be errors or explicit
   partial observations, identity/authentication is distinct from protocol hello,
   and resource/framing limits must be enforced before unbounded work.
5. `../AxiOwl-Desktop/tools/remote-desktop/README.md`: isolates owned child lifecycle
   and program packaging from independent runtime/credential state; source wiring
   alone does not establish installed cross-platform acceptance.

Additional actual source reviewed: `axiaccess_linux.py`, original Rust dispatch,
current shared engine/domain types and worker, Linux release signer invocation and
signing policy. Official GNOME architecture, Accessible/Component/Text D-Bus XML,
Role/StateType and Component.GetExtents documentation were consulted directly.

The additional repository document
`../AxiOwl-Desktop/docs/INSTALLER-APPLICATION-AND-DAEMON-LIFECYCLE-2026-09-30.md`
was read in full before the package lifecycle implementation: it establishes
exact owned-image/service selection and retaining previous activity on upgrade.
Its historical IDE daemon shutdown requirements do not apply to AxiAccess; this
new package never touches that service.

## Evidenced causes and changes

The original Linux Rust path launches Python; the Python adapter launches gdbus
to change accessibility flags on each construction. Its window IDs are enumeration
counters, bounds are fabricated 1280x800, offscreen is reused as minimized, all
extracted text is labelled visible, and native password role40 becomes ordinary
edit before extraction. Raw traversal is bounded by retained semantic nodes, not
actual work. These are source findings, not invented installed failures.

`src/platform/linux.rs` now talks directly to the AT-SPI accessibility bus using
Rust zbus. The adapter has no Python/gdbus/action/global-setting write. It requires
an explicitly selected graphical environment, discovers its accessibility bus,
reads actual screen-reader status rather than setting it, and separates session,
backend, stale-target and timeout errors. Native references include bus identity,
unique connection owner and object path; a legacy numeric alias is never an
enumeration index or substring/PID-based fallback.

Bounds are read only where VISIBLE and SHOWING make them meaningful; unknown
geometry is marked `bounds_known=false`. Actual ICONIFIED15 means minimized;
FOCUSED12 differs from FOCUSABLE11. Text visibility remains unknown rather than
claiming all content is visible. Native password role40 is checked before Name,
Text, attributes or child extraction. This does not promise detection of secrets
that applications expose in ordinary non-password content.

Nodes retain their native AT-SPI role and nullable native state. Checkbox,
selection and expansion state is populated only where applicable; an unavailable
state read yields null, not optimistic enabled/visible values. Compact hierarchy
reads request only bounded180-character text previews and native buffer identity;
full/text-only reads capture bounded text for explicit retained-page requests.
The semantic role mapping retains the established Linux button/edit/document
vocabulary used by legacy interactive/text formatting; the original AT-SPI role
remains separate. Password controls remain protected before normalization.

Each native D-Bus request has a350ms deadline inside a2.5s native observation,
leaving framing/recovery margin within the coordinator's3s hard deadline; raw nodes,
actual depth, text bytes, window counts and retained output are bounded. An
isolated shared observation process can be replaced by the coordinator if native
work stalls. Partial reads carry issues/truncation. A lazy retained native event
thread registers AT-SPI object/window/focus notifications, receives a bounded64
signal queue and wakes condition-variable waiters using a monotonic invalidation
revision. It does not decode, retain or log event payload text or scan the desktop
when idle. Disconnect/reconnect invalidates continuity; errors are visible in its
capability status. Signals invalidate data; actual differences still come from a
bounded fresh read. This source implementation is not yet a live event pass.

`scripts/release-linux.py` and `packaging/linux/` implement the independent DEB,
source/lock/toolchain/recipe receipts and existing-development-key attestation.
There is one canonical entrypoint. Root coordinates publication only after all
three platform builds and source publication. No hidden alternate release mode,
daemon payload, provider software or launch-time decompression is introduced.

The service is a user unit attached to the graphical session. Installation does
not start a root desktop observer. Explicit user setup/startup management creates
only AxiAccess-owned configuration/autostart entries. Setup is idempotent before
startup. Program upgrades preserve user pairing/configuration and native chats,
provider credentials, signing files and the independent daemon. The package's
normal OS prerequisites remain explicit; another desktop/toolkit's accessibility
coverage and arbitrary distributions are not inferred as qualified.

## Verification and boundaries

Implementation targets Linux x86_64; Windows and Mac adapters are owned separately.
Shared schema additive fields are coordinated with their owner for old JSON
compatibility. Actual runtime success, installed size, sender connection security,
offline reinstall and GUI/MCP acceptance are pending until their receipts exist.
No paid model tests, source resets, private-key copies, chat edits or daemon
changes were performed by this Linux work.

## Actual initial native compilation and discovery correction

The first Linux native library check passed in2m07s with three then-existing
shared-core unused-import warnings. A later bin+library check including native
event code passed in20.97s without warnings. The initial complete optimized
executable compiled in7m37s and reports2.1.0,10,286,424bytes and SHA256
`80a65f54e82fccf6fa51cba5e442e406de5643eef054375a1d3ce179111c5d56`.
It came from explicitly unpublished working-tree digest
`971efa32deb0d4d365cc691997584a95bbe32965aacb04ec47b07c7ade1af441`,
before the final AA07/lazy-text additions. It is not the final signed release.

A real read using the existing graphical user's environment returned zero windows
in13.5ms. That was retained as a discovery defect, not labelled a desktop pass.
An independent, already-installed GI AT-SPI reader returned10applications on the
same desktop. Direct read-only D-Bus queries confirmed that
`org.a11y.Bus.GetAddress` selected a different empty registry. The X11 root's
`AT_SPI_BUS` property identifies the populated graphical bus instead. GNOME's
actual libatspi source (`atspi/atspi-misc.c`, `atspi_get_a11y_bus`) explicitly uses
this order: AT_SPI_BUS_ADDRESS, X11 AT_SPI_BUS for X11 sessions, then session
D-Bus GetAddress. This establishes the actual cause; it was not missing apps or
a need to enable a screen-reader flag. Both status flags remained false and were
not changed.

The correction stays entirely in the Linux adapter: read the existing X11 property
through optional native Xlib, with a bounded address size, balanced display/data
ownership and no property writes. Honor an explicit bus address first; Wayland
uses the normal session D-Bus discovery. A failed explicit selected-display read
is an error rather than silently selecting another desktop. The same five
documents listed above had been read in full before this correction; their
relevant requirements are exact desktop/session identity, preserving the user's
environment, native APIs, truthful errors and no global activation from reads.
No other OS adapter or daemon protocol was changed for this Linux cause.

The retained native event worker subsequently reported an active subscription and
its revision advanced1->5->7 without desktop polling. That is real subscription
activity on the first selected bus, not proof yet of events from the corrected
graphical bus. The absent-DISPLAY/WAYLAND case returned `no_graphical_session`,
not an empty successful inventory. Updated native discovery and final signed
installer results are pending and will be appended after actual execution.

All four package shell files pass `sh -n`; the release Python compiles syntactically.
Systemd ordering was corrected during review to follow graphical-session-pre,
avoiding an After/WantedBy cycle with graphical-session.target. Active-user
systemd lifecycle is implemented; offline users' retained startup entries and
non-systemd foreground shutdown remain explicitly documented boundaries.

## Actual corrected graphical read and public native fixture

The refined working-tree binary compiled without warnings in4m26s,10,398,768bytes,
SHA256 `0f8191275f55bff1e79bd220cc900d65cdc6f457cd8d9242ba039452b915a79b`.
Its recorded native source-tree digest is
`4c53ce61d1061cae8a0969845dc911b49be88863179b9eee12e32a452b31e93a`.
It is still pre-publication evidence, not a final installed release identity.

Corrected native discovery returned six existing windows in65.5ms, each with
native identity and actual measured bounds. The selected IDE window's accessible
chrome yielded16nodes with native roles/state. Its embedded WebKit child names
an unowned sandboxed WebProcess bus; no document text interface was readable there.
An independent GI reader also found no IDE Text interfaces. That coverage is not
presented as a successful IDE-document read. Any failed child query now marks the
native observation partial rather than returning complete success.

A disposable public GTK fixture then demonstrated actual text and protected
controls without using provider histories or credentials. It disabled focus-on-map
and input acceptance; actual X11 active-window readback was unchanged. The fixture
provided public Unicode text with90lines outside its80pixel text viewport, one
checked checkbox and a password control containing a public sentinel. No click,
typing, focus action or application-document change was used. Its own process was
stopped after checking its exact command/user identity.

Observed results, recorded in
[sanitized evidence](evidence/linux-native-fixture-2026-10-23.json):

- Seven windows; all seven have measured bounds/native identities; inventory44ms.
- Compact: nine nodes, two deferred buffers, zero full-text bytes and232preview
  bytes in54ms; no issues or structural truncation.
- Full: the same nine nodes/two buffers,7,104UTF8bytes/6,384Unicode characters
  including the final public off-viewport marker, in55.6ms; no issues/truncation.
- Password native role was identified and protected. Its sentinel was absent from
  the entire native response, not merely omitted from the formatted text.
- The checkbox reported checked=true; all nine nodes retained native role/state.
- Native events were active, revision25, with no event text retention/full-tree
  polling. The native worker measured8,468kB RSS and9,404kB high-water RSS at this
  point; this is a worker measurement, not total IDE/network-sender memory.
- A missing graphical environment still returned a typed error.

No raw application text or user window titles are in the retained evidence.
The harness's Python/GTK is a development-only fixture using already-installed OS
bindings; the sender remained the native Rust binary and the installer ships none
of that fixture/runtime. Wayland, another Linux distribution, final signed DEB
installation and all-framework MCP acceptance are separate, not inferred passes.
