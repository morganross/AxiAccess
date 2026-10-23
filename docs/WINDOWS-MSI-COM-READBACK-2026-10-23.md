# Windows MSI readback: suppress COM void results

Date: October 23, 2026, America/Los_Angeles.

## Five related documents read before implementation

1. [IDE MSI reuse](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/IDE-MSI-MESSAGING-REUSE-2026-10-01.md): actual MSI table/binary readback is distinct from installed acceptance; keep exact original-account/process boundaries.
2. [MSI upgrade argument failure](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/IDE-MSI-UPGRADE-ARGUMENT-FAILURE-2026-10-01.md): a hand-assembled successful fixture can miss the real invocation context. The regression exercise here must use the canonical StrictMode, not just a permissive caller.
3. [MSI build execution status](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/IDE-MSI-BUILD-EXECUTION-AND-INSTALLATION-STATUS-2026-09-30.md): native compile/sign/link success is incomplete when the enclosing release fails; preserve the actual signed component and do not claim deployment.
4. [Installer hardening](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/IDE-INSTALLER-HARDENING-2026-09-30.md): retain original failures and genuine component identity; exact ownership/rollback evidence is not interchangeable with a successful final installation.
5. [Single-pass signed MSI](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/reports/WINDOWS-SINGLE-PASS-SIGNED-MSI-REUSE-2026-10-16.md): validate actual signed bytes and input identity before reuse; keep the original completed signing evidence rather than relabeling it as a new signing event.

## Evidenced root cause

On the actual published `b55df997cf62f85290e1e69885b8020c3915a6a3` release, the sender signed successfully and the MSI linked. `Read-MsiMetadata` then returned an `Object[]` containing twelve null COM return values plus the intended `OrderedDictionary`. Windows Installer automation's `Execute()` and `Close()` calls emitted those values into PowerShell's success pipeline.

The canonical entrypoint uses StrictMode Latest; accessing `ProductName` on that mixed result failed. The earlier focused caller omitted StrictMode, so PowerShell's permissive property enumeration skipped missing members and concealed the defect. The real diagnostic explicitly enumerated every returned item and established twelve null items followed by one dictionary. This is an actual release failure, not an inference from a filename or an unrun test.

## Correction and boundaries

Cast every read-only view `Execute()`/`Close()` call to `[void]`. The function must return exactly one metadata dictionary, retaining every real identity/inventory check and COM resource release. This changes Windows release tooling only; it does not alter AxiAccess observation, sender credentials/configuration, IDE behavior, other OS adapters or the daemon.

Do not repeat a successful signing operation just to recover the metadata step. Preserve the genuinely signed native image and its original signer receipt, compare exact unsigned input/source/policy/signed output and actual signature before reuse, then complete MSI validation/signing through the canonical path. New signing is required only when the actual input identity changes.

## Actual verification

Before the fix, real MSI diagnostics reported `System.Object[]`, twelve `<null COM return>` items, and one `OrderedDictionary` containing the correct product/version/source/file table. The failed canonical receipt is retained. The corrected focused read must run with StrictMode Latest and require one dictionary; final MSI signing/install/reinstall results are recorded separately after execution.
