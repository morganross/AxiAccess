#!/usr/bin/env bash
# Canonical complete native arm64 sender build, signed/notarized offline PKG.
# No source/provider/keychain reset, no observed UI action, no daemon dependency.
set +x
set -euo pipefail
[[ $# == 0 ]] || { echo 'The complete sender release accepts no modes or arguments.' >&2; exit 2; }
[[ "$(uname -s)" == Darwin && "$(uname -m)" == arm64 ]] || { echo 'Run on the supported Apple Silicon Mac.' >&2; exit 2; }
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
TOOLS="${AXIACCESS_MACOS_TOOLCHAIN:-$HOME/.local/share/axiowl-ide-macos/toolchain}"
export PATH="$TOOLS/rust/cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
export CARGO_HOME="${CARGO_HOME:-$TOOLS/rust/cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$TOOLS/rust/rustup}"
export MACOSX_DEPLOYMENT_TARGET=13.0
CONFIG="${AXIACCESS_MACOS_RELEASE_CONFIG:-$HOME/.config/axiowl/ide-macos-release.json}"
# Import public identity/reference metadata; passwords never enter shell argv,
# tracing, environment values, generated scripts, or release receipts.
eval "$(python3 - "$CONFIG" <<'PYCONFIG'
import json,shlex,sys
data=json.load(open(sys.argv[1]))
for output,key in [('SIGNING_KEYCHAIN','signing_keychain'),('APP_IDENTITY','application_identity'),('INSTALLER_IDENTITY','installer_identity'),('NOTARY_PROFILE','notary_profile'),('UNLOCK_REFERENCE','signing_unlock_reference')]:
 print(output+'='+shlex.quote(str(data.get(key,''))))
PYCONFIG
)"
SIGNING_KEYCHAIN="${AXIACCESS_MACOS_SIGNING_KEYCHAIN:-$SIGNING_KEYCHAIN}"
SIGNING_HELPERS="${AXIACCESS_MACOS_SIGNING_HELPERS:-$ROOT/packaging/macos}"
[[ "$APP_IDENTITY" == 'Developer ID Application: '* && "$INSTALLER_IDENTITY" == 'Developer ID Installer: '* ]] || { echo 'Configured Developer ID signing identities required.' >&2; exit 2; }
[[ -f "$SIGNING_KEYCHAIN" && -f "$SIGNING_HELPERS/signing-context.py" ]] || { echo 'Existing dedicated signing context required.' >&2; exit 2; }
for tool in cargo rustc swiftc pkgbuild productbuild codesign productsign xcrun pkgutil spctl ditto; do command -v "$tool" >/dev/null; done
VERSION="$(python3 -c 'import re;print(re.search(r"(?m)^version\s*=\s*\"([^\"]+)\"",open("Cargo.toml").read()).group(1))')"
SOURCE="$(git rev-parse HEAD)"
RUN="$(date -u '+%Y%m%dT%H%M%SZ')-$(uuidgen)"
OUT="$ROOT/release-artifacts/macos/$VERSION/$RUN"
mkdir -p "$OUT"
python3 - "$OUT/source.json" "$SOURCE" <<'PYSOURCE'
import hashlib,json,pathlib,subprocess,sys
root=pathlib.Path.cwd();h=hashlib.sha256()
files=sorted(subprocess.check_output(['git','ls-files','-co','--exclude-standard','-z']).split(b'\0'))
for name in files:
 if not name:continue
 p=root/name.decode();rel=p.relative_to(root)
 if any(v in ('target','release-artifacts','.git') for v in rel.parts) or not p.is_file():continue
 if rel.parts[0] not in ('src','packaging','scripts','Cargo.toml','Cargo.lock','build.rs'):continue
 h.update(name+b'\0');h.update(hashlib.sha256(p.read_bytes()).digest())
r={'sourceCommit':sys.argv[2],'sourceTreeSha256':h.hexdigest(),'sourceDirty':bool(subprocess.check_output(['git','status','--porcelain','--','src','packaging','scripts','Cargo.toml','Cargo.lock','build.rs'])),'lockSha256':hashlib.sha256((root/'Cargo.lock').read_bytes()).hexdigest(),'recipeSha256':hashlib.sha256((root/'scripts/release-macos.sh').read_bytes()).hexdigest(),'rustc':subprocess.check_output(['rustc','--version','--verbose'],text=True),'macos':subprocess.check_output(['sw_vers'],text=True),'target':'aarch64-apple-darwin','minimumSystemVersion':'13.0'}
pathlib.Path(sys.argv[1]).write_text(json.dumps(r,indent=2)+'\n')
PYSOURCE
python3 - "$ROOT" <<'PYSPACE'
import shutil,sys
free=shutil.disk_usage(sys.argv[1]).free
if free<2*1024**3:raise SystemExit('Insufficient headroom for sender build/signing: '+str(free))
print('Sender build free bytes: '+str(free))
PYSPACE
# Build dependencies are not installed in the sender. Its runtime is Rust and
# public system frameworks only; Swift launcher uses the system Swift runtime.
AXIACCESS_SOURCE_REVISION="$SOURCE" cargo build --locked --release --target aarch64-apple-darwin
APP="$OUT/AxiAccess Sender.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Library/LaunchAgents"
cp "$ROOT/target/aarch64-apple-darwin/release/axiaccess" "$APP/Contents/MacOS/axiaccess"
swiftc -O -target arm64-apple-macos13.0 -framework AppKit -framework ServiceManagement "$ROOT/packaging/macos/SenderApp.swift" -o "$APP/Contents/MacOS/AxiAccessSender"
swiftc -O -target arm64-apple-macos13.0 "$ROOT/packaging/macos/LoginHelper.swift" -o "$APP/Contents/MacOS/axiaccess-login"
python3 - "$APP" "$OUT/macho-targets.json" <<'PYMACHO'
import json,pathlib,re,subprocess,sys
app,out=map(pathlib.Path,sys.argv[1:]);records=[]
for name in ('axiaccess','AxiAccessSender','axiaccess-login'):
 image=app/'Contents/MacOS'/name
 arch=subprocess.check_output(['lipo','-archs',str(image)],text=True).strip()
 commands=subprocess.check_output(['otool','-l',str(image)],text=True)
 minimum=re.search(r'\bminos\s+([\d.]+)',commands)
 if arch!='arm64' or minimum is None:raise SystemExit('Unexpected sender Mach-O architecture/deployment metadata: '+name)
 version=(tuple(map(int,minimum[1].split('.')))+(0,0,0))[:3]
 if version>(13,0,0):raise SystemExit('Sender binary exceeds declared macOS13 minimum: '+name)
 records.append({'image':name,'architecture':arch,'minimumSystemVersion':minimum[1]})
out.write_text(json.dumps(records,indent=2)+'\n')
PYMACHO
sed "s/AXIACCESS_VERSION/$VERSION/g" "$ROOT/packaging/macos/Info.plist" > "$APP/Contents/Info.plist"
cp "$ROOT/packaging/macos/com.axiowl.axiaccess.sender.plist" "$APP/Contents/Library/LaunchAgents/"
cp "$OUT/source.json" "$APP/Contents/Resources/source.json"
cp "$ROOT/packaging/macos/uninstall.sh" "$APP/Contents/Resources/uninstall.sh"
chmod 755 "$APP/Contents/Resources/uninstall.sh"
if [[ -f "$ROOT/LICENSE" ]]; then cp "$ROOT/LICENSE" "$APP/Contents/Resources/LICENSE"; fi
LOCK="$HOME/.axiowl/ide-release.lock"
mkdir -p "$HOME/.axiowl"
OWNED=0
CONTEXT="$OUT/signing-context.json"
cleanup(){
 local result=$?
 trap - EXIT
 if [[ -f "$CONTEXT" ]]; then python3 "$SIGNING_HELPERS/signing-context.py" restore "$CONTEXT" "$SIGNING_KEYCHAIN" || result=1; fi
 if [[ "$OWNED" == 1 && -f "$LOCK/token" && "$(cat "$LOCK/token")" == "$RUN" ]]; then
  rm -f -- "$LOCK/token" "$LOCK/owner.json"
  rmdir -- "$LOCK" || result=1
 fi
 exit "$result"
}
trap cleanup EXIT
while ! mkdir -m 700 "$LOCK" 2>/dev/null; do
 [[ -d "$LOCK" && ! -L "$LOCK" ]] || { echo 'Invalid shared signing lock.' >&2; exit 1; }
 sleep 1
done
OWNED=1
printf '%s\n' "$RUN" > "$LOCK/token"
python3 - "$LOCK/owner.json" "$RUN" "$$" <<'PYOWNER'
import json,pathlib,sys
pathlib.Path(sys.argv[1]).write_text(json.dumps({'product':'AxiAccess sender','token':sys.argv[2],'pid':int(sys.argv[3]),'purpose':'shared Apple sender signing'}))
PYOWNER
python3 "$SIGNING_HELPERS/signing-context.py" capture "$CONTEXT" "$SIGNING_KEYCHAIN"
python3 "$SIGNING_HELPERS/signing-context.py" apply "$CONTEXT" "$SIGNING_KEYCHAIN"
unlock(){ if [[ -n "$UNLOCK_REFERENCE" ]]; then python3 "$SIGNING_HELPERS/unlock-existing-signing.py" "$SIGNING_KEYCHAIN" "$UNLOCK_REFERENCE"; fi; }
unlock
APP_HASH="$(security find-identity -v -p codesigning "$SIGNING_KEYCHAIN" | awk -v wanted="$APP_IDENTITY" 'index($0,"\"" wanted "\"") {print $2;exit}')"
[[ "$APP_HASH" =~ ^[[:xdigit:]]{40}$ ]] || { echo 'Configured application signing identity unavailable.' >&2; exit 1; }
for image in axiaccess axiaccess-login AxiAccessSender; do
 unlock
 codesign --force --options runtime --timestamp --keychain "$SIGNING_KEYCHAIN" --sign "$APP_HASH" "$APP/Contents/MacOS/$image"
 codesign --verify --strict "$APP/Contents/MacOS/$image"
done
unlock
codesign --force --options runtime --timestamp --keychain "$SIGNING_KEYCHAIN" --sign "$APP_HASH" "$APP"
codesign --verify --strict --deep "$APP"
ditto -c -k --keepParent "$APP" "$OUT/application.zip"
unlock
xcrun notarytool submit "$OUT/application.zip" --keychain-profile "$NOTARY_PROFILE" --keychain "$SIGNING_KEYCHAIN" --wait --output-format json > "$OUT/notary-app.json"
python3 - "$OUT/notary-app.json" <<'PYNOTARY'
import json,sys
if json.load(open(sys.argv[1])).get('status')!='Accepted':raise SystemExit('Sender app notarization rejected')
PYNOTARY
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"
spctl --assess --type execute "$APP"
python3 - "$OUT/components.plist" <<'PYCOMPONENT'
import plistlib,sys
plistlib.dump([{'BundleIsRelocatable':False,'BundleHasStrictIdentifier':True,'BundleOverwriteAction':'upgrade','RootRelativeBundlePath':'Applications/AxiAccess Sender.app'}],open(sys.argv[1],'wb'))
PYCOMPONENT
mkdir -p "$OUT/payload/Applications"
ditto "$APP" "$OUT/payload/Applications/AxiAccess Sender.app"
pkgbuild --root "$OUT/payload" --component-plist "$OUT/components.plist" --identifier com.axiowl.axiaccess.sender.component --version "$VERSION" --install-location / "$OUT/component.pkg"
productbuild --synthesize --product "$ROOT/packaging/macos/requirements.plist" --package "$OUT/component.pkg" "$OUT/Distribution.xml"
productbuild --distribution "$OUT/Distribution.xml" --package-path "$OUT" "$OUT/unsigned.pkg"
unlock
productsign --keychain "$SIGNING_KEYCHAIN" --sign "$INSTALLER_IDENTITY" "$OUT/unsigned.pkg" "$OUT/AxiAccess-Sender-macOS-arm64-$VERSION.pkg"
PKG="$OUT/AxiAccess-Sender-macOS-arm64-$VERSION.pkg"
unlock
xcrun notarytool submit "$PKG" --keychain-profile "$NOTARY_PROFILE" --keychain "$SIGNING_KEYCHAIN" --wait --output-format json > "$OUT/notary-pkg.json"
python3 - "$OUT/notary-pkg.json" <<'PYNOTARY'
import json,sys
if json.load(open(sys.argv[1])).get('status')!='Accepted':raise SystemExit('Sender PKG notarization rejected')
PYNOTARY
xcrun stapler staple "$PKG"
xcrun stapler validate "$PKG"
pkgutil --check-signature "$PKG" > "$OUT/package-signature.txt"
spctl --assess --type install "$PKG"
otool -L "$APP/Contents/MacOS/axiaccess" > "$OUT/native-dependencies.txt"
python3 - "$OUT" "$APP" "$PKG" <<'PYRECEIPT'
import hashlib,json,pathlib,subprocess,sys
out,app,pkg=map(pathlib.Path,sys.argv[1:]);source=json.loads((out/'source.json').read_text())
inventory=[]
for file in sorted(app.rglob('*')):
 if file.is_file() and not file.is_symlink():inventory.append({'path':str(file.relative_to(app)),'bytes':file.stat().st_size,'sha256':hashlib.sha256(file.read_bytes()).hexdigest()})
r={**source,'artifact':pkg.name,'installerBytes':pkg.stat().st_size,'installerSha256':hashlib.sha256(pkg.read_bytes()).hexdigest(),'applicationBytes':sum(v['bytes'] for v in inventory),'files':inventory,'machoTargets':json.loads((out/'macho-targets.json').read_text()),'signed':True,'notarized':True,'stapled':True,'installed':False,'runtimePython':False,'runtimeNode':False,'daemonIncluded':False,'published':False}
(out/'manifest.json').write_text(json.dumps(r,indent=2)+'\n')
(out/'SHA256SUMS').write_text(r['installerSha256']+'  '+pkg.name+'\n')
print(json.dumps({'installer':str(pkg),'bytes':r['installerBytes'],'sha256':r['installerSha256']}))
PYRECEIPT
# Publication is intentionally separate from package creation authorization.
# The complete artifact and its immutable provenance are ready for the owning
# release coordinator; this script never silently publishes private desktop data.
printf 'Signed, notarized offline sender installer: %s\n' "$PKG"
