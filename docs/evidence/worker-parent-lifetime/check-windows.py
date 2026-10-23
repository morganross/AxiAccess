"""Owned Windows coordinator/worker lifetime fault injection; no app input."""
from ctypes import wintypes as W
import ctypes as C
import hashlib,json,pathlib,queue,subprocess,threading,time

root=pathlib.Path(__file__).resolve().parents[2]
binary=root/'repos/AxiAccess/target/debug/axiaccess.exe'
k=C.WinDLL('kernel32',use_last_error=True)
class ProcessEntry(C.Structure):
 _fields_=[('dwSize',W.DWORD),('cntUsage',W.DWORD),('th32ProcessID',W.DWORD),('heap',C.c_size_t),('module',W.DWORD),('threads',W.DWORD),('parent',W.DWORD),('priority',W.LONG),('flags',W.DWORD),('file',W.WCHAR*260)]
class ThreadEntry(C.Structure):
 _fields_=[('dwSize',W.DWORD),('usage',W.DWORD),('thread',W.DWORD),('owner',W.DWORD),('base',W.LONG),('delta',W.LONG),('flags',W.DWORD)]
for name,args,result in [
 ('CreateToolhelp32Snapshot',[W.DWORD,W.DWORD],W.HANDLE),('Process32FirstW',[W.HANDLE,C.POINTER(ProcessEntry)],W.BOOL),('Process32NextW',[W.HANDLE,C.POINTER(ProcessEntry)],W.BOOL),('Thread32First',[W.HANDLE,C.POINTER(ThreadEntry)],W.BOOL),('Thread32Next',[W.HANDLE,C.POINTER(ThreadEntry)],W.BOOL),('OpenProcess',[W.DWORD,W.BOOL,W.DWORD],W.HANDLE),('OpenThread',[W.DWORD,W.BOOL,W.DWORD],W.HANDLE),('CloseHandle',[W.HANDLE],W.BOOL),('SuspendThread',[W.HANDLE],W.DWORD),('ResumeThread',[W.HANDLE],W.DWORD),('WaitForSingleObject',[W.HANDLE,W.DWORD],W.DWORD),('TerminateProcess',[W.HANDLE,W.UINT],W.BOOL),('QueryFullProcessImageNameW',[W.HANDLE,W.DWORD,W.LPWSTR,C.POINTER(W.DWORD)],W.BOOL),('GetThreadTimes',[W.HANDLE,C.POINTER(W.FILETIME),C.POINTER(W.FILETIME),C.POINTER(W.FILETIME),C.POINTER(W.FILETIME)],W.BOOL)]:
 fn=getattr(k,name);fn.argtypes=args;fn.restype=result

def processes(parent):
 snapshot=k.CreateToolhelp32Snapshot(2,0);entry=ProcessEntry();entry.dwSize=C.sizeof(entry);result=[]
 try:
  ok=k.Process32FirstW(snapshot,C.byref(entry))
  while ok:
   if entry.parent==parent and entry.file.lower()=='axiaccess.exe':result.append(entry.th32ProcessID)
   ok=k.Process32NextW(snapshot,C.byref(entry))
 finally:k.CloseHandle(snapshot)
 assert len(result)==1,'Expected exactly one owned native worker'
 handle=k.OpenProcess(0x1000|0x100000|1,False,result[0]);assert handle
 name=C.create_unicode_buffer(32768);length=W.DWORD(len(name));assert k.QueryFullProcessImageNameW(handle,0,name,C.byref(length))
 assert pathlib.Path(name.value).resolve()==binary.resolve(),'Worker image identity mismatch'
 return result[0],handle

def main_thread(pid):
 snapshot=k.CreateToolhelp32Snapshot(4,0);entry=ThreadEntry();entry.dwSize=C.sizeof(entry);found=[]
 try:
  ok=k.Thread32First(snapshot,C.byref(entry))
  while ok:
   if entry.owner==pid:
    handle=k.OpenThread(0x40|2,False,entry.thread)
    if handle:
     times=[W.FILETIME() for _ in range(4)]
     if k.GetThreadTimes(handle,*[C.byref(t) for t in times]):found.append(((times[0].dwHighDateTime<<32)|times[0].dwLowDateTime,entry.thread,handle))
     else:k.CloseHandle(handle)
   ok=k.Thread32Next(snapshot,C.byref(entry))
 finally:k.CloseHandle(snapshot)
 assert found;found.sort()
 for _,_,handle in found[1:]:k.CloseHandle(handle)
 return found[0][1:]

def response(process):
 result=queue.Queue();threading.Thread(target=lambda:result.put(process.stdout.readline(262145)),daemon=True).start();return json.loads(result.get(timeout=10))
def send(process,value):process.stdin.write(json.dumps(value)+'\n');process.stdin.flush()
def case(stall):
 process=subprocess.Popen([str(binary),'sender','--stdio'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True)
 worker_handle=thread_handle=None
 try:
  token='b'*64;send(process,{'type':'bootstrap','protocolVersion':1,'token':token,'clientId':'owned-lease-check'});ready=response(process)
  send(process,{'id':'owned-worker','token':token,'operation':'capabilities','params':{}});assert response(process)['id']=='owned-worker'
  worker,worker_handle=processes(process.pid)
  result={'controlledMainThreadStall':stall,'sourceRevision':ready.get('sourceRevision'),'parentPid':process.pid,'ownedWorkerPid':worker,'parentSignal':'TerminateProcess exact owned coordinator','observedApplicationActions':False}
  if stall:
   tid,thread_handle=main_thread(worker);assert k.SuspendThread(thread_handle)==0;result['suspendedOwnedMainThread']=tid
   send(process,{'id':'held-read','token':token,'operation':'windows','params':{}});time.sleep(.1)
  started=time.monotonic();process.terminate();process.wait(timeout=3);wait=k.WaitForSingleObject(worker_handle,4500)
  result['workerAliveAfter4500ms']=wait==258;result['workerObservedExitMs']=None if wait==258 else round((time.monotonic()-started)*1000,1)
  assert wait==0,'Owned worker survived parent death beyond the bound'
  return result
 finally:
  if process.poll() is None:process.kill();process.wait(timeout=3)
  if thread_handle:
   if worker_handle and k.WaitForSingleObject(worker_handle,0)==258:k.ResumeThread(thread_handle)
   k.CloseHandle(thread_handle)
  if worker_handle:
   if k.WaitForSingleObject(worker_handle,2000)==258:k.TerminateProcess(worker_handle,1)
   k.CloseHandle(worker_handle)

result={'binarySha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'fixtureType':'Owned main-thread suspension, not a naturally observed UI Automation hang','cases':[case(False),case(True)]}
target=root/'output/axiaccess-implementation-20261022/worker-parent-lease-windows.json';target.write_text(json.dumps(result,indent=2));print(json.dumps(result,indent=2))
