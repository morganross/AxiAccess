# Linux native event invalidation: exclude observer connection noise

Date: October23,2026, America/Los_Angeles. Status: correction prepared;
final native event/release verification pending.

## Five documents actually read before this correction

1. `../AxiOwl-Desktop/docs/AXIACCESS-SENDER-IDE-MCP-IMPLEMENTATION-PLAN-2026-10-22.md`:
   native events are hints about observed data, not an excuse for repeated whole
   desktop reads; idle work and retained event queues must be bounded.
2. `README.md`: preserves observation/change tracking and read-only scope; client
   connection management must not appear as a fictitious application change.
3. `AXIOWL-ACCESS-LISTENER-INTEGRATION-PLAN.md`: historical event/diff intent;
   its unimplemented event claims were not accepted as verification.
4. `../AxiOwl-Desktop/ide-ipc/README.md`: sequence/epoch continuity and resync must
   describe actual source continuity and must not replay/start unrelated work.
5. `../AxiOwl-Desktop/tools/remote-desktop/README.md`: explicit owned process/
   session boundaries; observation clients are not the observed applications.

All five actual contents were read before the edit, not just search snippets.
GNOME Registry/Event D-Bus XML and actual libatspi implementation were also read.

## Evidence and root cause

An installed sourceb55df997 TLS inspection of the public stationary GTK fixture
returned a complete nine-node observation but included
`native_events_during_capture`. The source event handler increments its global
revision for every D-Bus NameOwnerChanged signal. Each native read opens/closes
its own accessibility-bus connection, which changes its unique bus-name owner.
Those observer lifecycle events are not changes to the selected application.
This source-defined condition adds false event warnings and can wake long waits
needlessly. The response was complete; this is not relabelled as a failed text
read, failed password filter, or failed TLS authentication.

## Correction and compatibility

Record a bounded set of actual application owners discovered from the registry,
keyed with accessibility-bus identity. NameOwnerChanged invalidates data only for
those owners or for the registry itself. Registry ownership changes reconnect
and re-register native event interests. Ordinary object/window/focus events
continue invalidating observations. No event content/text is retained or logged;
the owner set has a512-entry ceiling and gap handling remains explicit.

The change is Linux-only and does not change Windows UIA or Mac AX observers,
shared delta semantics, credentials, chats, observed application state or daemon
behavior. It introduces no click/type/focus/shell action. Actual corrected
warnings/wait behavior and final compiled source are recorded after verification.
