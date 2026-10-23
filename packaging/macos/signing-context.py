#!/usr/bin/env python3
"""Temporarily expose the explicitly approved existing signing keychain.

Matches the common Mac producer workflow. No certificate, password, trust,
default keychain, ACL, partition list, or lock policy is changed.
"""
import json, os, pathlib, shlex, stat, subprocess, sys

operation, snapshot_name, selected = sys.argv[1:]
snapshot = pathlib.Path(snapshot_name)
security = "/usr/bin/security"
def current():
    result = subprocess.run([security, "list-keychains", "-d", "user"], check=True, capture_output=True, text=True)
    return shlex.split(result.stdout)
def write_state(state):
    temporary = snapshot.with_suffix(".pending")
    with os.fdopen(os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600), "w") as stream:
        json.dump(state, stream)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, snapshot)
def read_state():
    metadata = snapshot.lstat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
        raise RuntimeError("signing context snapshot is not private to this account")
    return json.loads(snapshot.read_text())
def set_list(paths):
    if not paths:
        raise RuntimeError("refusing an empty keychain search list")
    subprocess.run([security, "list-keychains", "-d", "user", "-s", *paths], check=True)
    if current() != paths:
        raise RuntimeError("keychain search-list readback differs from requested paths")
if operation == "capture":
    if not os.path.isabs(selected) or not pathlib.Path(selected).is_file() or snapshot.exists():
        raise RuntimeError("existing absolute dedicated keychain and fresh snapshot are required")
    original = current()
    # codesign limits identity lookup with --keychain, but issuer lookup
    # uses the user search context. Put this exact private issuer store first;
    # retain every other store and restore the original order afterward.
    proposed = [selected, *[p for p in original if os.path.realpath(p) != os.path.realpath(selected)]]
    write_state({"original": original, "proposed": proposed, "selected": selected, "armed": False})
    print("signing_context_captured=true")
elif operation == "apply":
    state = read_state()
    if state["selected"] != selected:
        raise RuntimeError("signing keychain differs from captured approval")
    if state["armed"]:
        if os.path.realpath(selected) not in {os.path.realpath(p) for p in current()}:
            raise RuntimeError("another process removed the active signing keychain")
    else:
        if current() != state["original"]:
            raise RuntimeError("another process changed the signing context before application")
        if state["proposed"] != state["original"]:
            state["armed"] = True
            write_state(state)
            set_list(state["proposed"])
            print("signing_context_applied=true")
elif operation == "restore":
    state = read_state()
    if state["armed"]:
        observed = current()
        if observed == state["proposed"]:
            restored = state["original"]
        elif os.path.realpath(selected) not in {os.path.realpath(p) for p in state["original"]}:
            restored = [p for p in observed if os.path.realpath(p) != os.path.realpath(state["selected"])]
        else:
            # It was already present before our temporary reorder. Preserve a
            # concurrent owner's current list instead of removing their store.
            restored = observed
        if restored != observed:
            set_list(restored)
        state["armed"] = False
        write_state(state)
        print("signing_context_restored=true")
        if restored != state["original"]:
            print("signing_context_concurrent_changes_preserved=true")
else:
    raise RuntimeError("unknown signing context operation")
