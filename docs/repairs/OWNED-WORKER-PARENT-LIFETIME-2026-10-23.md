# Owned native worker lifetime after coordinator termination

Scope: shared native-worker process isolation only. No observed application,
daemon, provider history/credentials, sender pairing or installer policy changed.

## Documents read before implementation

The full actual contents of these distinct documents were read before editing:

1. [Sender protocol](../SENDER-PROTOCOL.md): private stdin belongs to the owning
   IDE/sender lifetime; native work has a hard deadline and must not escape that
   lifecycle. Public observation tools have no process-control authority.
2. [Shared sender repair](SHARED-SENDER-PROTOCOL-MCP-2026-10-22.md): describes the
   existing request correlation, kill-on-drop lease, worker replacement and
   exact-child ownership. That cleanup assumes the coordinator can execute Rust
   destructors; it does not by itself handle abrupt coordinator death.
3. [Mac native/packaging record](MACOS-NATIVE-AX-AND-SENDER-PACKAGING-2026-10-22.md):
   native AX calls have client timeouts/cooperative observation deadlines;
   GUI/session controls own only their sender child. This motivated preserving
   those API bounds while adding independent private-pipe lifetime enforcement.
4. [Linux package operation](../../packaging/linux/README.md): systemd user and
   foreground startup/stop are different paths; neither can justify killing
   arbitrary processes or another independently installed sender.
5. [IDE remote-runtime lifecycle](https://github.com/morganross/AxiOwl-Desktop/blob/main/tools/remote-desktop/README.md):
   app-owned child shutdown, independent runtime preservation and exact-image
   ownership inform a worker-local solution instead of process-name sweeps.
6. [IDE IPC contract](https://github.com/morganross/AxiOwl-Desktop/blob/main/ide-ipc/README.md):
   pipe ownership, bounded complete frames and explicit disconnection failures
   inform the lease reader; that frozen IPC/daemon protocol is unchanged.

Relevant source also read: `src/observe.rs`, `src/transport/sender.rs`,
`packaging/macos/{SenderApp,LoginHelper}.swift`, Linux user unit/session helper.
Source listings supplement rather than count toward the six document readings.

## Evidenced cause and actual risk

The coordinator's native `Child` uses kill-on-drop and a3-second read deadline.
Its private stdin pipe closes when the coordinator dies. Previously, however,
only the native worker's observation thread read that pipe between operations.
If that thread could not run/return, closed stdin alone did not terminate it;
an abruptly killed coordinator cannot later run its timeout/destructor cleanup.
Mac GUI `Process.terminate()` sends SIGTERM to its exact coordinator; direct IDE
termination can similarly bypass Rust cleanup. Managed service process-group
cleanup may independently protect some startup paths and is not declared broken.

No naturally hung AX/UIA/AT-SPI provider was observed. Ordinary shutdown worked.
The controlled failure-isolation fixture paused only the fixture-owned worker's
main thread, leaving other threads runnable, then terminated only its own sender.
No observed app was signalled, modified, focused, clicked or typed into.

Actual Linux baseline at source`49b368b685f55ddd906ae5725c90e3a059789f88`:

- Normal SIGTERM: parent exited in0.8ms; worker exited after26.4ms.
- Controlled main-thread stall: parent exited in1.3ms; worker remained alive
  after4,500ms, beyond the coordinator's3-second native budget.
- The fixture detached/resumed and cleaned its exact owned PIDs afterward.

This establishes the missing parent-lifetime guarantee under the controlled
stall. It does not claim every SIGTERM leaves an orphan or that an actual Mac
provider ignored its configured AX messaging timeout.

## Narrow change

Only `src/observe.rs` product code changed. Hidden native-worker mode starts a
dedicated private-stdin lease reader, then passes bounded complete frames through
a one-slot channel to the existing native observation thread. The production
coordinator already serializes requests. A malformed extra queue is rejected
rather than blocking the lifetime reader.

Kernel EOF/read failure on that sole parent-owned pipe ends the worker even if
the observation thread is blocked. Unix uses `_exit`; Windows uses
`TerminateProcess(GetCurrentProcess())`. These end only the current owned worker
and avoid atexit/CoreFoundation/stdio cleanup locks potentially held by the
stalled observation thread. There is no PID polling, name-based kill, observed-
application signal, new input tool or separate service. Idle waiting blocks on
the pipe and introduces no desktop scan. Existing native deadlines remain.

The reader has a128KiB stack and one bounded request slot; no CPU/RSS benchmark
is claimed. Entire-process OS/debugger suspension prevents any watchdog thread
from executing and is outside this runnable-watchdog fixture's guarantee.

## Compatibility and OS boundaries

The same private-pipe contract applies to all three native sender targets. Unix
and Windows termination primitives differ only in this worker-local function.
No public CLI/MCP/sender request shape changes. NativeWorker keeps stdin open
while it expects responses. A raw hidden-worker test helper must likewise read
its response before closing stdin; EOF now deliberately revokes worker lifetime,
not a request to finish arbitrary native work after its owner disconnected.

Normal service stop, standalone MCP completion, IDE-owned sender close and
coordinator crash all retain exact-process ownership. No independent sender,
observed app, native credential store or daemon is stopped by this change.

## Actual verification

- Windows GNU`cargo check --all-targets` and native debug binary build passed.
- Linux candidate release compile passed in186.99s from published49b plus only
  `src/observe.rs`; exact original Linux source bytes were restored afterward.
  Its metadata deliberately says`working-tree-parent-lease`, not a published
  commit. The installed sender was not replaced/stopped for this fixture.
- The same Linux controlled-main-thread fixture now observed worker exit1.6ms
  after parent SIGTERM in both normal and stalled-main cases. No worker survived
  the4,500ms bound. Exact source/binary hashes are in the candidate receipt.
- A Windows fixture validated its direct child's image/parent identity, retained
  exact process/thread handles, suspended only the owned main thread and called
  TerminateProcess only on its own coordinator. Worker exit was10.9ms normally
  and11.7ms with the owned main thread suspended. Both passed the4,500ms bound;
  fixture cleanup touched only its retained owned handles.

Sanitized receipts and reproducible fixture sources:
[`docs/evidence/worker-parent-lifetime/`](../evidence/worker-parent-lifetime/).
No user window text, provider contents or credentials are retained.

Mac compilation and an actual Mac fault-injection run are not established by
these Linux/Windows results. Final canonical three-OS artifacts must be rebuilt
from the new coherent source; these candidate proofs do not relabel already
signed49b installers as containing this repair.
