# Windows standalone sender packaging

Date: October 22, 2026, America/Los_Angeles.

## Documents read before implementation

The actual contents of these distinct documents were read. Copies of the old plan in two repositories count once.

1. [AxiAccess README](../README.md): established existing binary/CLI identity and the legacy undeclared Python dependency. The package takes only the new native sender, not Python/server demo files.
2. [Earlier integration plan](../AXIOWL-ACCESS-LISTENER-INTEGRATION-PLAN.md): identified the explicit daemon-isolation requirement. Its engine duplication, STA and action proposals are superseded by the approved plan and are not implemented here.
3. [IDE remote-desktop README](https://github.com/morganross/AxiOwl-Desktop/blob/d7b763ff34d3c34e0cb57eb728077d911a17d916/tools/remote-desktop/README.md): demonstrated owned-process lifecycle and offline native packaging boundaries. AxiAccess remains a separate product and does not reuse the desktop gateway's identity.
4. [Offline installer contract](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/IDE-OFFLINE-INSTALLER-RELEASE-CONTRACT-2026-09-28.md): original-account installation and signature/source binding informed the per-user MSI. Its retired IDE commands and daemon-installation sequence are historical and are not reused.
5. [Installer hardening history](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/IDE-INSTALLER-HARDENING-2026-09-30.md): exact installation path/account checks, durable failure receipts and preserving unrelated runtime processes informed the lifecycle helper.
6. [Single-pass signed MSI history](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/reports/WINDOWS-SINGLE-PASS-SIGNED-MSI-REUSE-2026-10-16.md): build complete authoring once, sign once, and retain real signing evidence. Per-user registry key paths and explicit component GUIDs avoid the previously encountered ICE38 authoring error.
7. [Installer version repair](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/reports/WINDOWS-INSTALLER-VERSION-REPAIR-2026-10-18.md): the executable is in the mandatory feature; the source version must fit MSI limits. AxiAccess's independent version is not coupled to IDE versions.
8. The approved `AxiOwl-Desktop/docs/AXIACCESS-SENDER-IDE-MCP-IMPLEMENTATION-PLAN-2026-10-22.md`: native offline sender, initial x86_64 target, per-user graphical startup, authentication preservation and provenance requirements determine this deliverable.
9. [Windows session independence](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/WINDOWS-SESSION-INDEPENDENCE-2026-09-28.md): original-account identity must not be guessed from an active RDP user or hardcoded session number. AxiAccess accessibility observation requires its selected graphical session even though the separate daemon's IPC can operate in Session0.
10. [Installer lifecycle history](https://github.com/morganross/AxiOwl-Desktop/blob/main/docs/INSTALLER-APPLICATION-AND-DAEMON-LIFECYCLE-2026-09-30.md): exact path/SID/process birth validation and genuine installation receipts informed the sender lifecycle; the historical service stop/restart is expressly excluded from AxiAccess.

## Evidenced cause and implementation

At initial inspection AxiAccess had no sender installer or canonical Windows release script. Its binary could be manually compiled, but that did not provide independent offline deployment, user-session startup or signed provenance. This work adds `scripts/release-windows.ps1`, `packaging/windows/sender.wxs` and `packaging/windows/session.ps1`.

The release entrypoint builds the native Rust sender with the static MSVC CRT, using a cache identity containing source commit, actual source-tree digest, Cargo lock, compiler identity, target, profile and packaging recipe. `AXIACCESS_SOURCE_REVISION` embeds the revision expected by the sender handshake. It signs the executable and MSI through the existing authorized publisher helper and validates actual publisher, timestamp and Authenticode status. Source files, build recipe, native payload hashes, sizes and a release receipt remain beside the MSI. No signing credential is copied into the source tree or package. Release upload, when explicitly configured by the caller, is part of this same entrypoint; the implementation run leaves publication to the coordinating release owner.

The recipe also records actual WiX compiler/linker and signing SDK/Dlib hashes. The entrypoint opens the linked MSI read-only and checks ProductName, ProductCode, ProductVersion, source commit, recipe digest and the exact three-file inventory/sizes against the staged payload before signing it. COM database/view/record handles are released before signing so a readback cannot leave the MSI locked.

The MSI is per-user and installs under `%LOCALAPPDATA%\Programs\AxiAccess`. The native executable belongs to its mandatory feature. It performs idempotent `setup --quiet` after installing the files, starts the configured sender in the original account, and registers an HKCU logon entry. It does not create a Windows service or SYSTEM observer. The installed helper exposes Start/Stop/Status/EnableStartup/DisableStartup.

When installation originates in noninteractive Session0 (for example SSH), Start reports `startDeferred` and does not spawn a Session0 observer or select an arbitrary RDP desktop. Startup occurs in the selected graphical user's session through its normal logon entry, or the operator invokes Start there. Installed CLI configuration is not falsely counted as live accessibility readiness. Installed acceptance must execute the MSI/start helper through the explicitly selected user-session launcher when installing remotely.

MSI setup/start actions pass the original user's `[LocalAppDataFolder]AxiAccess\sender.json` explicitly. They do not depend on a deferred Windows Installer process inheriting the user's `LOCALAPPDATA` environment. This keeps certificate/pairing state in the selected account even when Windows Installer uses its service process for execution. The installed helper also accepts the explicit configuration path and preserves it when enabling startup.

Stop/upgrade/uninstall select the exact installed executable, original account SID, sender command and process creation time. They do not terminate every process with the AxiAccess name. Uninstall removes only MSI-owned files/registry entries. Sender identity, certificate/key, pairing and configuration in `%LOCALAPPDATA%\AxiAccess` are deliberately outside the MSI file/removal tables and are retained.

The MSI invokes the helper using a quoted `-File` argument. It does not interpolate an installation path into PowerShell source; apostrophes in Windows user paths must not turn upgrade/uninstall into script parsing or execution errors.

ProductCode is deterministic for the AxiAccess Windows x64 product version. Rebuilding version 2.1.0 therefore cannot create a second same-version product registration. Distinct future product releases increment Cargo's version and use MajorUpgrade. Byte replacement within an already installed same version uses the standard `msiexec /i <MSI> REINSTALL=ALL REINSTALLMODE=vamus` repair; a plain same-version maintenance open is not falsely reported as installing new bytes.

Native build tools are executed with explicit process exit handling and asynchronously drained stdout/stderr. The initial precompile runner exposed PowerShell 5 treating Cargo's ordinary stderr progress as a terminating error when redirected through PowerShell's error pipeline. The canonical entrypoint captures native streams directly and decides success from the native exit code; it retains the output in per-step logs rather than classifying progress text as a build failure.

## Compatibility and boundaries

- Windows x86_64 only; Mac/Linux adapters and packages are separate owners.
- The native toolchain's OS floor is Windows 10 / Windows Server 2016, per [Rust target support](https://doc.rust-lang.org/rustc/platform-support.html). A graphical user session is still required; Server Core is not promised as an observable desktop. This is an API/toolchain requirement, not multi-version installer acceptance evidence.
- The observer reads the logged-in user's desktop. This package does not claim access to another session, the secure desktop or protected/elevated apps.
- No click/type/action tools, provider installation, native chat reads/writes, provider credential changes, IDE builds or daemon modifications belong to this package.
- The default authenticated TLS endpoint is loopback. Remote bind/pairing is explicit sender configuration; the installer does not create firewall rules or expose a port globally.
- Configuration and sender trust state survive repair/reinstall/upgrade/uninstall. Template cloning requires deliberately generating a distinct sender identity rather than copying an already paired private state directory.

## Verification and remaining acceptance

Observed source checks: both PowerShell scripts parsed without parser errors; `sender.wxs` parsed as XML. Independent integration review found and corrected the revision environment-name mismatch and embedded-path PowerShell quoting before any release build.

The existing Windows node has approximately 170 GiB available at inspection. Its normal Cargo PATH shim is zero bytes; the real existing MSVC toolchain under `.rustup/toolchains/1.99.0-x86_64-pc-windows-msvc/bin` is used explicitly. This is build-tool selection, not a provider credential or cloud resource change.

Actual native compilation, MSI validation/signatures, install/reinstall/uninstall, live sender health and config-preservation evidence will be appended from the real release receipts. Source parsing alone is not installer or desktop-observation acceptance.

The real precommit MSVC release build completed with exit 0 in approximately 200 seconds on the Windows development node. Its unsigned image is 9,405,952 bytes, SHA256 `D1BDB1E4F86D7753A0C77C1374EBA984223959ED0EDC3C547866D54F0E6E22F7`. This validates that coherent precommit source, not the later final release revision.

Focused WiX authoring compilation/linking then identified ICE80: a 64-bit component used `[SystemFolder]` for PowerShell. All lifecycle paths now use `[System64Folder]`. ICE91's generic per-user file warning assumes `ALLUSERS` might request machine installation; this MSI explicitly rejects `ALLUSERS`, is limited/per-user, and never offers a machine installation. Only ICE91 is excluded for that documented false premise, while every other ICE remains enabled and all remaining WiX warnings are fatal. The authoring artifact is explicitly unsigned/precommit and must not be installed or published as a release.
