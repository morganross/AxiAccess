# Windows sender signing runtime context

Date: October 23, 2026, America/Los_Angeles.

## Documents actually read before the correction

1. [Windows release controller history](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/WINDOWS-IDE-RELEASE-CONTROLLER.md): preserving the original signing execution context and reconciling an uncertain signing outcome instead of blindly re-signing. Its retired controller commands are not run.
2. [Windows offline installer/release contract](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/IDE-OFFLINE-INSTALLER-RELEASE-CONTRACT-2026-09-28.md): complete signing inputs and verified signatures are required; native build success is not release success. The historical daemon/install commands are excluded.
3. [Installer hardening](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/IDE-INSTALLER-HARDENING-2026-09-30.md): retain actual failure receipts and distinguish original failures from recovery problems. No credential or runtime replacement is introduced.
4. [Single-pass signed MSI history](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/reports/WINDOWS-SINGLE-PASS-SIGNED-MSI-REUSE-2026-10-16.md): source/input identity and genuine signing evidence bind each artifact; build tool context is part of that identity.
5. [Windows installer/version repair](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/reports/WINDOWS-INSTALLER-VERSION-REPAIR-2026-10-18.md): process-only launch context corrections and observed full-release results must remain distinct. Native source and version are preserved by an execution-context repair.

The current `AxiOwl-Desktop/scripts/windows/WindowsIdeReleaseLibrary.psm1` tool bootstrap, the existing Artifact Signing helper and Dlib runtime configuration were also read. These code/configuration sources directly establish the missing prerequisite.

## Evidence and boundary

The first canonical AxiAccess Windows release used published source `b55df997cf62f85290e1e69885b8020c3915a6a3`. Native MSVC compilation succeeded in 1m59s. Signing then returned SignTool exit 3, with unchanged candidate bytes and no operation identifier. Its failure receipt remains retained; no MSI was produced or installed and no signing success was claimed.

The existing IDE bootstrap sets `DOTNET_ROOT`, `DOTNET_ROOT_X64` and process PATH to its .NET tool installation before calling the signing helper. The standalone AxiAccess entrypoint initially resolved the signing SDK and native Dlib but omitted this runtime context. `Azure.CodeSigning.Dlib.runtimeconfig.json` explicitly requires `Microsoft.NETCore.App` 8.0.0 with major roll-forward. The existing Windows host has Microsoft.NETCore.App 8.0.31 under `%LOCALAPPDATA%\AxiOwlBuildTools\dotnet`; it was not selected in the standalone signing process.

The absent runtime selection is evidenced. Whether it accounts for the entire SignTool failure is confirmed only by the corrected actual signing invocation; no unsupported assertion about missing credentials is made.

## Correction

The canonical release script accepts an explicit AxiAccess .NET build-runtime root, respects an existing `DOTNET_ROOT`, otherwise resolves the already installed AxiOwl build-tool directory, checks its real runtime list, and selects it only in the release process environment. The recipe records that host's binary hash and actual installed runtime versions. It neither installs a runtime nor changes machine/user PATH settings, certificates, account authentication or private signing files.

This is Windows signing-tool context only. The sender remains native/static-CRT and its MSI contains no .NET, Node, Python, provider authentication or chat data. Mac/Linux packaging, IDE runtime and existing daemon are unchanged.

## Actual verification

The existing runtime's `--list-runtimes` returned Microsoft.NETCore.App 8.0.31, ASP.NETCore.App 8.0.31 and WindowsDesktop.App 8.0.31. PowerShell parsing of the corrected source is checked before execution. Final signature/installer evidence is appended only after the corrected canonical run succeeds.

The unchanged published b55df997 sender was subsequently signed successfully when the operational build process selected that existing .NET root. Cargo reused the compiled input in 0.36 seconds. Native signed SHA256 is `CF698D50B4633F494F9BCFAAE2B9883B1EF1266E1C69EA9F5A2200E714610A3F`, Windows Authenticode `Valid`, with a trusted timestamp and original operation `466d8386-15a8-4e93-9f86-23e1a20462d5`. No credential, certificate, key or application source changed. This confirms the missing runtime selection caused the initial signing failure. A later separate MSI metadata failure does not invalidate that genuine signature; its successful receipt and signed bytes are retained.
