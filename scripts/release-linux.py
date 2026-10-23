#!/usr/bin/env python3
"""Canonical Linux sender release: native build, offline DEB, signing, receipts.

Run from the existing AxiAccess main checkout. Publication is coordinated by the
multi-platform release owner after all native artifacts pass; no script silently
overwrites a release from another source revision.
"""
from pathlib import Path
import datetime
import fcntl
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import tempfile
import tomllib
import uuid

ROOT = Path(__file__).resolve().parents[1]
PREDICATE = 'https://github.com/morganross/AxiAccess/attestations/linux-sender/v1'


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def run(args, *, env=None, capture=False):
    result = subprocess.run([str(a) for a in args], cwd=ROOT, env=env,
                            text=True, capture_output=capture, check=True)
    return result.stdout.strip() if capture else None


def save(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def main():
    if platform.system() != 'Linux' or platform.machine() not in ('x86_64', 'amd64'):
        raise SystemExit('This canonical sender entrypoint builds Linux x86_64 only.')
    cache = ROOT / '.cache'
    cache.mkdir(exist_ok=True)
    lock = (cache / 'release-linux.lock').open('a')
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    if run(['git', 'branch', '--show-current'], capture=True) != 'main':
        raise SystemExit('Use the existing literal main checkout.')
    if run(['git', 'status', '--porcelain', '--untracked-files=normal'], capture=True):
        raise SystemExit('Release source has uncommitted files; retain them and commit the intended source before release.')
    revision = run(['git', 'rev-parse', 'HEAD'], capture=True)
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    target = 'x86_64-unknown-linux-gnu'
    # Build tools already installed on the development machine are reused. No
    # rustup default, package install, private-key copy or global setup occurs.
    tools = Path(os.environ.get('AXIACCESS_BUILD_TOOLS', '/home/kjhgf/workspaces/AxiOwl-IDE-Linux/packaging/linux-ide/cache/release-tools'))
    env = dict(os.environ, CARGO_BUILD_JOBS='2', CARGO_INCREMENTAL='0', AXIACCESS_SOURCE_REVISION=revision)
    if (tools / 'rustup').is_dir():
        env.setdefault('CARGO_HOME', str(tools / 'cargo'))
        env.setdefault('RUSTUP_HOME', str(tools / 'rustup'))
        env.setdefault('RUSTUP_TOOLCHAIN', '1.98.1')
    cargo = shutil.which('cargo')
    if not cargo:
        raise SystemExit('Cargo is required on the build node; it is not a sender runtime dependency.')
    toolchain = run([cargo, '--version'], env=env, capture=True)
    recipe_files = [Path(__file__)] + sorted((ROOT / 'packaging/linux').glob('*'))
    recipe = hashlib.sha256(''.join(digest(p) for p in recipe_files if p.is_file()).encode()).hexdigest()
    lock_digest = digest(ROOT / 'Cargo.lock')
    cache_key = hashlib.sha256(f'{revision}:{target}:{toolchain}:{lock_digest}:{recipe}:release'.encode()).hexdigest()
    # Cargo reuses unchanged dependency compilation in this owned workspace.
    # Immutable produced binaries are separately addressed by the complete key.
    env['CARGO_TARGET_DIR'] = str(cache / 'target')
    out = ROOT / 'dist' / f'linux-{version}-{revision[:12]}' / uuid.uuid4().hex
    out.mkdir(parents=True, exist_ok=True)
    receipt = {'schemaVersion': 1, 'product': 'AxiAccess sender', 'platform': 'linux', 'architecture': 'x86_64',
               'version': version, 'sourceRevision': revision, 'dependencyLockSha256': lock_digest,
               'recipeSha256': recipe, 'toolchain': toolchain, 'target': target, 'cacheKey': cache_key,
               'startedAt': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'status': 'building',
               'daemonChanged': False, 'runtimePython': False, 'runtimeNode': False}
    save(out / 'receipt.json', receipt)
    run([cargo, 'build', '--locked', '--release', '--target', target], env=env)
    binary = Path(env['CARGO_TARGET_DIR']) / target / 'release/axiaccess'
    artifact_cache = cache / 'artifacts' / cache_key
    artifact_cache.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(binary, artifact_cache / 'axiaccess')
    (artifact_cache / 'axiaccess').chmod(0o755)
    save(artifact_cache / 'identity.json', {**receipt, 'binarySha256': digest(binary)})
    imports = run(['readelf', '--version-info', binary], capture=True)
    glibc = sorted({tuple(map(int, v.split('.'))) for v in re.findall(r'GLIBC_([0-9]+(?:\.[0-9]+)+)', imports)})
    if not glibc:
        raise SystemExit('Native ELF glibc requirements could not be determined.')
    glibc_floor = '.'.join(map(str, glibc[-1]))
    with tempfile.TemporaryDirectory(prefix='axiaccess-deb-', dir=out) as temporary:
        stage = Path(temporary)
        def copy(source, relative, mode=0o644):
            destination = stage / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
            destination.chmod(mode)
        copy(binary, 'usr/bin/axiaccess', 0o755)
        copy(ROOT / 'packaging/linux/axiaccess-session', 'usr/bin/axiaccess-session', 0o755)
        copy(ROOT / 'packaging/linux/install-lifecycle', 'usr/lib/axiaccess/install-lifecycle', 0o755)
        copy(ROOT / 'packaging/linux/axiaccess-sender.service', 'usr/lib/systemd/user/axiaccess-sender.service')
        copy(ROOT / 'packaging/linux/axiaccess-sender.desktop', 'usr/share/applications/axiaccess-sender.desktop')
        copy(ROOT / 'packaging/linux/README.md', 'usr/share/doc/axiaccess-sender/README.md')
        inventory = [{'path': p.relative_to(stage).as_posix(), 'sha256': digest(p), 'bytes': p.stat().st_size,
                      'mode': oct(p.stat().st_mode & 0o777)} for p in sorted(stage.rglob('*')) if p.is_file()]
        manifest = {**receipt, 'status': 'built', 'minimumGlibc': glibc_floor, 'files': inventory,
                    'programPayloadBytes': sum(f['bytes'] for f in inventory),
                    'inventoryExcludes': ['usr/share/axiaccess/manifest.json'],
                    'nativeImports': run(['ldd', binary], capture=True),
                    'osPrerequisites': ['graphical desktop session', 'AT-SPI2 accessibility bus', 'D-Bus user session']}
        save(out / 'manifest.json', manifest)
        copy(out / 'manifest.json', 'usr/share/axiaccess/manifest.json')
        installed_bytes = sum(p.stat().st_size for p in stage.rglob('*') if p.is_file())
        (stage / 'DEBIAN').mkdir()
        copy(ROOT / 'packaging/linux/prerm', 'DEBIAN/prerm', 0o755)
        copy(ROOT / 'packaging/linux/postinst', 'DEBIAN/postinst', 0o755)
        (stage / 'DEBIAN/control').write_text(f'''Package: axiaccess-sender
Version: {version}
Architecture: amd64
Section: utils
Priority: optional
Maintainer: Morgan Ross <morganross@rossmorr.com>
Depends: libc6 (>= {glibc_floor}), libgcc-s1, dbus-user-session, at-spi2-core, util-linux
Installed-Size: {(sum(p.stat().st_size for p in stage.rglob('*') if p.is_file()) + 1023)//1024}
Homepage: https://github.com/morganross/AxiAccess
Description: Native read-only semantic desktop observation sender
 Reads application accessibility APIs in a selected graphical user session.
 No input injection, Python runtime, IDE dependency or AxiOwl daemon changes.
''')
        # TemporaryDirectory starts0700 and a developer's umask may be002.
        # Neither belongs in package-owned program directory metadata.
        for directory in [stage, *sorted(p for p in stage.rglob('*') if p.is_dir())]:
            directory.chmod(0o755)
        artifact = out / f'AxiAccess_{version}_amd64.deb'
        run(['dpkg-deb', '--root-owner-group', '--uniform-compression', '-Zxz', '-z6', '--build', stage, artifact])
    receipt.update(status='packaged', artifact=artifact.name, artifactBytes=artifact.stat().st_size,
                   artifactSha256=digest(artifact), binarySha256=digest(binary), manifestSha256=digest(out / 'manifest.json'),
                   installedLogicalBytes=installed_bytes)
    save(out / 'receipt.json', receipt)
    config_path = Path(os.environ.get('AXIACCESS_LINUX_SIGNING_CONFIG', '/home/kjhgf/workspaces/AxiOwl-IDE-Linux/release/linux-ide/release-config.json'))
    config = json.loads(config_path.read_text())
    public_key = Path(os.environ.get('AXIACCESS_LINUX_PUBLIC_KEY', str(config_path.parent / 'axiowl-linux-release.pub')))
    cosign = tools / 'bin/cosign'
    password = subprocess.check_output(['/usr/bin/sudo', '-n', '/usr/bin/systemd-creds', 'decrypt',
        '--name=axiowl-ide-cosign-password', config['password_credential'], '-'], stderr=subprocess.PIPE).strip().decode()
    signing_env = dict(env, COSIGN_PASSWORD=password)
    bundle = out / f'{artifact.name}.sigstore.json'
    try:
        run([cosign, 'attest-blob', '--yes', '--oidc-disable-ambient-providers', '--key', config['signing_key'],
             '--bundle', bundle, '--type', PREDICATE, '--predicate', out / 'manifest.json', artifact], env=signing_env)
    finally:
        signing_env.pop('COSIGN_PASSWORD', None)
        password = None
    run([cosign, 'verify-blob-attestation', '--trusted-root', config_path.parent / 'trusted-root.json',
         '--use-signed-timestamps', '--key', public_key, '--bundle', bundle,
         '--digest', digest(artifact), '--digestAlg', 'sha256', '--type', PREDICATE])
    shutil.copyfile(public_key, out / 'axiaccess-linux-release.pub')
    receipt.update(status='signed', signatureSha256=digest(bundle), signerPublicKeySha256=digest(public_key),
                   completedAt=datetime.datetime.now(datetime.timezone.utc).isoformat(), publication='awaiting coordinated multi-platform release')
    save(out / 'receipt.json', receipt)
    (out / 'SHA256SUMS').write_text(''.join(f'{digest(p)}  {p.name}\n' for p in sorted(out.iterdir()) if p.is_file() and p.name != 'SHA256SUMS'))
    print(json.dumps({'status': 'signed', 'output': str(out), 'receipt': str(out / 'receipt.json')}))


if __name__ == '__main__':
    main()
