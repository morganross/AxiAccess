"""Linux-only controlled owned-main-thread stall; no observed-app actions.

The fixture pauses only its own native worker's main thread with ptrace, leaving
other threads runnable. It is NOT a naturally observed accessibility API hang.
All stopped/killed PIDs are exact descendants started here and identity checked.
"""
import ctypes
import hashlib
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import time

BINARY = Path(os.environ.get('AXIACCESS_LEASE_TEST_BINARY', '/home/kjhgf/workspaces/AxiAccess/.cache/target/x86_64-unknown-linux-gnu/release/axiaccess')).resolve()
libc = ctypes.CDLL(None, use_errno=True)
libc.ptrace.argtypes = [ctypes.c_uint, ctypes.c_uint, ctypes.c_void_p, ctypes.c_void_p]
libc.ptrace.restype = ctypes.c_long
SEIZE, INTERRUPT, DETACH, EXITKILL, WALL = 0x4206, 0x4207, 17, 0x00100000, 0x40000000

def ptrace(operation, pid, data=0):
    if libc.ptrace(operation, pid, None, ctypes.c_void_p(data)) == -1:
        code = ctypes.get_errno()
        raise OSError(code, os.strerror(code))

def identity(pid):
    stat = Path(f'/proc/{pid}/stat').read_text()
    fields = stat[stat.rfind(')')+2:].split()
    return {'state': fields[0], 'ppid': int(fields[1]), 'start': fields[19],
            'command': Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0')[:-1]}

def living(pid, expected):
    try:
        current = identity(pid)
        return current['start'] == expected['start'] and current['state'] not in ('Z', 'X')
    except FileNotFoundError:
        return False

def next_line(process, seconds=8):
    if not select.select([process.stdout], [], [], seconds)[0]:
        raise RuntimeError('Owned sender response deadline')
    raw = process.stdout.readline(262145)
    if not raw:
        raise RuntimeError('Owned sender disconnected')
    return json.loads(raw)

def send(process, value):
    process.stdin.write(json.dumps(value)+'\n')
    process.stdin.flush()

def case(stall):
    process = subprocess.Popen([str(BINARY), 'sender', '--stdio'], stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    worker = None
    expected = None
    traced = False
    result = {'controlledMainThreadStall': stall, 'parentSignal': 'SIGTERM',
              'observedApplicationActions': False}
    token = 'a' * 64
    try:
        send(process, {'type': 'bootstrap', 'protocolVersion': 1,
                       'token': token, 'clientId': 'owned-parent-lease-fixture'})
        ready = next_line(process)
        result['sourceRevision'] = ready.get('sourceRevision')
        send(process, {'id': 'start-owned-worker', 'token': token,
                       'operation': 'capabilities', 'params': {}})
        response = next_line(process)
        assert response['id'] == 'start-owned-worker'
        children = Path(f'/proc/{process.pid}/task/{process.pid}/children').read_text().split()
        candidates = [(int(pid), identity(int(pid))) for pid in children]
        candidates = [(pid, state) for pid, state in candidates
                      if state['ppid'] == process.pid and state['command'] ==
                      [str(BINARY).encode(), b'__worker']]
        assert len(candidates) == 1, 'No exact owned native worker'
        worker, expected = candidates[0]
        result['ownedWorkerPid'] = worker
        result['parentPid'] = process.pid
        if stall:
            # Seize only main TID, not the process/thread group. EXITKILL cleans
            # this exact tracee if the fixture itself unexpectedly terminates.
            ptrace(SEIZE, worker, EXITKILL)
            traced = True
            ptrace(INTERRUPT, worker)
            stopped_pid, status = os.waitpid(worker, WALL)
            assert stopped_pid == worker and os.WIFSTOPPED(status)
            result['mainThreadTraceStopped'] = True
            send(process, {'id': 'held-native-read', 'token': token,
                           'operation': 'windows', 'params': {}})
            time.sleep(.1)
        started = time.monotonic()
        process.terminate()
        process.wait(timeout=3)
        result['parentExitCode'] = process.returncode
        result['parentExitMs'] = round((time.monotonic()-started)*1000, 1)
        deadline = started + 4.5
        while time.monotonic() < deadline:
            if traced:
                try:
                    pid, status = os.waitpid(worker, os.WNOHANG | WALL)
                    if pid and (os.WIFEXITED(status) or os.WIFSIGNALED(status)):
                        traced = False
                except ChildProcessError:
                    traced = False
            if not living(worker, expected):
                break
            time.sleep(.025)
        result['workerAliveAfter4500ms'] = living(worker, expected)
        result['workerObservedExitMs'] = None if result['workerAliveAfter4500ms'] else round((time.monotonic()-started)*1000,1)
        result['scope'] = 'Controlled runnable-watchdog/paused-main-thread fixture; not a natural AX/AT-SPI provider hang'
        return result
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=3)
        if worker and expected and living(worker, expected):
            if traced:
                try:
                    ptrace(DETACH, worker)
                    traced = False
                except OSError:
                    pass
            deadline = time.monotonic()+2
            while living(worker, expected) and time.monotonic() < deadline:
                time.sleep(.025)
            if living(worker, expected):
                # Exact original child identity only, never any observed app.
                os.kill(worker, signal.SIGKILL)
        if traced and worker:
            try:
                os.waitpid(worker, os.WNOHANG | WALL)
            except ChildProcessError:
                pass

report = {'binarySha256': hashlib.sha256(BINARY.read_bytes()).hexdigest(),
          'version': subprocess.check_output([str(BINARY), '--version'], text=True).strip(),
          'fixtureType': 'Owned child-main-thread fault injection; all other app processes untouched',
          'cases': [case(False), case(True)]}
print(json.dumps(report, indent=2))
