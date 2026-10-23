# Configuration transaction serialization

Date: October 23, 2026, America/Los_Angeles.

Scope: `src/transport/config.rs`, shared by the native sender management CLI and the native GUI's management-command subprocesses; README management examples. No provider credentials/chats, production AxiAccess pairing, observed app, daemon, signing material or installation was changed by this repair task.

## Documents read before implementation

The actual full contents of these five distinct directly related repository documents were read before editing this fix. Source listings, filenames and duplicate historical plans do not count as documents.

1. [Sender protocol](../SENDER-PROTOCOL.md): requires persistent sender identity, idempotent setup, private configuration, atomic listen configuration that preserves clients, per-request pairing reload and revocation without restarting the sender. These guarantees require a transaction around the entire read/mutate/write sequence, not merely an atomic file replacement.
2. [Shared sender protocol/MCP repair record](SHARED-SENDER-PROTOCOL-MCP-2026-10-22.md): records owner-only Windows ACLs, Unix0700/0600, Windows `ReplaceFileW`, persistent startup configuration and immediate config-based revocation. The correction preserves those established operations rather than replacing them with a new store or resetting identity.
3. [Windows standalone sender packaging](../WINDOWS-SENDER-PACKAGING-2026-10-22.md): installation/reinstall call idempotent setup in the original account and retain private identity/pairing outside MSI-owned files. Setup therefore needs the same interprocess guard as explicit pair/configure calls; a concurrent installer cannot invent a second identity or truncate the first.
4. [Linux sender operation](../../packaging/linux/README.md): the user-session manager and package lifecycle can issue setup/start commands while management operates separately. It explicitly preserves user-owned configuration on upgrade/uninstall, so transaction ownership belongs to the shared native config library, not a particular CLI wrapper or GUI instance.
5. [TLS acceptance client contract](TLS-ACCEPTANCE-CLIENT-2026-10-22.md): authenticates real native sender calls with isolated product configuration, preserves unrelated credentials and distinguishes malformed fixture requests from product failures. This informed the isolated real-process regression proof and its refusal to use production keys or GUI/app operations.

The approved final AxiAccess/IDE plan and both native lifecycle records had also been read in full earlier in this implementation task. They reinforce the separate sender identity namespace and cross-platform preservation boundaries; the five specific readings above were completed before this transaction edit.

The current README was read in full before its small follow-up edit: it lacked the newly implemented persistent `configure --listen` example, while its Linux session-command list used unescaped table separators. The management examples now distinguish stored startup configuration from a one-run sender override, describe transaction preservation, escape that table cell and remove an extra blank line at EOF. Concurrent Mac minimum/lazy-text documentation edits were preserved.

Primary API reference checked: [Rust `File::lock`](https://doc.rust-lang.org/std/fs/struct.File.html#method.lock). The stable1.89 API provides native interprocess exclusive locking and releases its lock on handle closure. The current implementation maps to `flock(LOCK_EX)` on Unix and `LockFileEx` on Windows; this repair uses the common standard API, not guessed lock ownership or a stale marker timeout. The current development toolchain is Rust1.99.0.

## Exact evidenced root cause

At source `49b368b685f55ddd906ae5725c90e3a059789f88`, `edit_client` and `configure_listen` independently read the complete private `Config`, mutate one field, then atomically replace the JSON. Atomic replacement prevents partial JSON, but it does not serialize the preceding reads.

For example, configure can read a map containing a client; revoke removes that client and completes its replacement; configure then writes its older full map plus the new address, resurrecting the revoked client. Concurrent pairing can similarly lose one newly issued client. `setup` also checked existence before identity creation without a shared interprocess transaction.

This is a shared configuration cause on every OS. It is not an accessibility API failure and does not belong in an OS observation adapter or the IDE daemon.

## Change and preserved behavior

- Added a `ConfigTransaction` guard in the same native library. `setup`, `edit_client` (pair/revoke) and `configure_listen` acquire it before reading/creating the configuration and retain it through the existing synced write and atomic replacement.
- The lock is a persistent empty `CONFIG_FILENAME.transaction.lock` alongside the private config. Its parent is canonicalized so equivalent directory aliases share the same lock object. It is deliberately separate from the replaced JSON inode/handle.
- Never delete the lock file on unlock: another process may already be waiting on that native object. Handle closure/process exit releases actual OS ownership; a leftover empty lock file is not a stale held lock and requires no cleanup heuristic.
- Lock directory/file must be current-user private: Unix0700/0600 and owner UID, or owner-only Windows ACL policy. Windows opens the lock without delete sharing and with final-component reparse-point opening. New private directories are created with Unix0700 or an atomic owner-only Windows security descriptor. Existing parents are validated, never chmod'ed or re-ACLed. An unexpected public/foreign-owner parent or lock is an error, not silently weakened permissions.
- Bare `--config sender.json` and equivalent `./sender.json` are rejected before any filesystem operation. The guard never substitutes the process's unrelated current directory and changes its permissions/ACL to prepare a lock. Default and explicitly parented private configuration paths retain their normal behavior.
- Existing config serialization, private keys, certificate/fingerprint, client tokens, defaulted listen address, idempotent setup and `ReplaceFileW`/Unix rename behavior remain. `read` and sender request authentication remain lock-free readers of atomic configuration; a pending management transaction does not queue all desktop observations behind its lock.
- Existing CLI and GUI callers already use these three shared functions; there is no second wrapper-specific locking implementation. The Mac GUI invokes native management subprocesses rather than writing private JSON itself. Windows/Linux session setup uses the same native command path.
- No dependency was added. Building this API requires Rust1.89 or later; the current supported release toolchains use1.99. Native package source/cache receipts must identify the new source when rebuilt.

This serializes cooperating current-version writers. Arbitrary manual edits or older executables that do not use the guard cannot be made transactional by an advisory lock. Use the matching updated management binary during upgrade; do not claim protection against an independently running obsolete writer. Network filesystems must support the native lock operation; an unsupported operation reports failure instead of bypassing the guard.

Native Windows preservation also required correcting an ACL representation assumption. Under an atomically created private parent, the isolated fixture's initial config DACL was `D:(A;ID;FA;;;OWNER)`. `ReplaceFileW` produced `D:AI(A;;FA;;;OWNER)(A;ID;FA;;;OWNER)`: equivalent explicit and inherited grants to that same owner. The previous `AceCount == 1` check rejected this still-owner-only ACL. The verifier now checks every native ACE: it must be an ordinary allow entry for the current owner's SID, with no additional principal. No ACL is granted, rewritten or relaxed to another user. A separate in-memory native Windows descriptor test accepts equivalent owner entries and rejects both an Everyone allow entry and an unrecognized deny entry. The test does not grant permissions on any file.

## Meaningful real-process proof

Added four regression scenarios plus one subprocess-only test helper under `cfg(test)`. They create unique, private `axiaccess-config-transaction-*` directories under the current user's temporary directory; no production product path or application data is used. Only public test metadata is retained transiently, and every exact owned subprocess/directory is cleaned after completion.

1. **Held-lock mutation barrier:** the parent holds the native transaction lock; eight distinct OS subprocesses request one listen change, one revoke and six new pairings. Each signals mutation intent. All remain blocked while the lock is held. After release they all exit successfully; the revoked client stays absent, all six new clients remain, the address changes and original sender/certificate/private-key/owner-token fields stay identical. This would fail if a writer only protected its final rename or ignored the guard.
2. **Concurrent setup:** six OS subprocesses are held behind the same lock before a configuration exists. After release, exactly one reports creation; all six report the same sender ID. Repeating setup preserves the exact configuration bytes. Native private-file/lock ACL checks pass on Windows; Unix assertions check directory0700/config0600/lock0600 when run there.
3. **Holder termination:** one subprocess holds the native lock while a pairing writer is blocked. Terminating that exact test holder releases the OS lock; the waiting writer completes and the lock file remains. No stale timestamp, retry deletion or guessed process PID ownership is used.
4. **Unrelated current directory:** a fresh subprocess is given a different isolated working directory with an unrelated sentinel file. Both `sender.json` and `./sender.json` setup reject. The exact native Windows DACL (or Unix owner/group/mode), directory entries and sentinel bytes remain unchanged. The Windows test compares the native security descriptor, not just the read-only attribute.

## Actual verification

Observed on the local Windows GNU toolchain, Rust1.99.0:

```text
cargo check --lib --locked
Finished dev profile successfully on the final source (3.69 seconds).

cargo test --lib transport::config::tests --locked -- --nocapture --test-threads=1
Finished test profile (18.77 seconds after final parent/ACL corrections).
running 6 tests
bare_config_rejects_without_changing_cwd_files_or_permissions ... ok
concurrent_process_mutations_preserve_revocation_pairing_and_identity ... ok
concurrent_process_setup_creates_one_identity_and_retains_private_files ... ok
native_transaction_lock_is_released_when_holder_process_exits ... ok
owner_acl_accepts_equivalent_owner_entries_but_rejects_foreign_principals ... ok
transaction_child ... ok
6 passed; 0 failed; 14 filtered out; test execution 0.67 seconds.
```

The helper passes trivially in the top-level runner unless explicitly launched with the isolated test environment by a parent regression scenario. The four actual scenarios use real new OS processes, not threads or a mocked lock; the fifth assertion group uses real Windows descriptor parsing in memory. All config mutation happens only inside isolated fixtures. Independent review caught the initial current-directory fallback and the equivalent `./`/existing-parent permission risk before publication. Both were removed. Changing new-parent creation then exposed the native equivalent-owner ACL case above (three tests passed/two failed); after correcting the representation check, the final six-test rerun passed without warnings.

`git diff --check -- src/transport/config.rs` reported no whitespace errors. Git separately noted the checkout's normal LF-to-CRLF conversion policy; that is not a Rust/compiler warning.

Not performed by this repair owner: native Linux/Mac lock execution, MSVC release builds, final sender installer rebuild/reinstall, remote TLS/IDE GUI/all-framework calls. Root coordinates those source-bound artifact reruns. Cross-platform source and shared native API are implemented, but Windows fixture success alone does not establish Linux/Mac runtime acceptance.
