# Linux sender DEB directory metadata normalization

Date: October23,2026, America/Los_Angeles. Status: source correction prepared;
new signed package and installation acceptance pending.

## Documents read before this correction

These five distinct relevant repository documents were read in full before the
correction; this is not a filename-only or duplicate-plan count.

1. `../AxiOwl-Desktop/docs/AXIACCESS-SENDER-IDE-MCP-IMPLEMENTATION-PLAN-2026-10-22.md`:
   requires owned-file-only installation, offline completeness, exact manifest/
   source provenance and preserved independent daemon/user data boundaries.
2. `../AxiOwl-Desktop/release/linux-ide/README.md`: specifies executable/program
   mode normalization, no setuid/setgid payload and retained original-user state.
3. `../AxiOwl-Desktop/docs/reports/LINUX-PACKAGING-SCOPE-AND-COMPRESSION-2026-10-22.md`:
   package encoding must preserve exact authenticated payload and installed scope;
   build success alone is not installation/qualification evidence.
4. `../AxiOwl-Desktop/docs/reports/LINUX-WEBKIT-USER-INSTALL-PRESERVATION-2026-10-22.md`:
   keeps elevated system and original-user scopes separate and forbids renderer/
   packaging work from replacing credentials, state or daemon ownership.
5. `../AxiOwl-Desktop/docs/IDE-OFFLINE-INSTALLER-RELEASE-CONTRACT-2026-09-28.md`:
   its retired Windows commands are not executed; the relevant enduring contract
   is complete signed native inputs, original-user preservation and owned program
   installation. Existing system directories must not be repurposed as app state.

## Evidenced root cause

Canonical `b55df997cf62f85290e1e69885b8020c3915a6a3` built/signed the Linux2.1.0
sender candidate and signature verification passed. Its DEB file list showed
`./`0700 and program directories0775. Python TemporaryDirectory creates a0700
staging root; parent.mkdir inherits the build user's002 umask. File modes were
explicitly normalized, but directory modes were not normalized before dpkg-deb.

Actual package install exited0. Readback showed existing `/`, `/usr`, `/usr/bin`,
`/usr/lib` and `/usr/share` remained0755; they were not changed to the archive's
directory modes. The new package-owned `/usr/lib/axiaccess` was0775. This is a
package metadata defect, not evidence that the root filesystem became0700 or
that another user gained access to provider credentials. The candidate and its
signed receipt are retained and are not published as the final release.

## Correction and boundaries

Before dpkg-deb, normalize the staging root and every staged directory to0755.
Regular file755/644 decisions stay intact; no new setuid or shared writable
program directories are introduced. This belongs in the Linux package adapter,
not shared native observation or any Windows/Mac source.

Debian's official dpkg/policy documentation explains that existing directory
permissions are preserved and an owned-directory correction needs explicit
postinst behavior. The signed postinst therefore removes group/other write only
from the three exact AxiAccess-owned program directories when root:root, without
following symlinks. It respects dpkg-statoverride and retains stricter existing
permissions. It never changes `/usr`, other shared directories or user homes.

The correction does not change executable functionality, observation authority,
user config/pairing/chat storage, credentials, signer keys or the AxiOwl daemon.
It will be compiled/packaged from the actual next committed source/recipe, with
a new retained run receipt. No manual mutation of signed artifact contents is
used to claim success.

## Verification

Pending: canonical signed rebuild; DEB directory inventory admission; exact
installed image/package verification and new-owned-directory0755 readback.
The initial candidate's native desktop/public GTK fixture read evidence remains
scoped to its recorded binary. Final installed/TLS results are recorded only
after the corrected artifact is actually installed.
