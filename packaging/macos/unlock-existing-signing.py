#!/usr/bin/env python3
"""Unlock an explicitly supplied existing private signing reference in memory.
No imports, ACL/default/search-list changes, password argv, output or persistence.
"""
import ctypes,os,pathlib,stat,sys
if len(sys.argv)!=3:raise SystemExit('Expected existing keychain and approved unlock-reference paths')
keychain_path,reference=map(pathlib.Path,sys.argv[1:])
for p in (keychain_path,reference):
 if not p.is_absolute():raise SystemExit('Signing references must be absolute')
 m=p.lstat()
 if not stat.S_ISREG(m.st_mode) or m.st_uid!=os.getuid() or stat.S_IMODE(m.st_mode)&0o077:raise SystemExit('Signing reference must be private to this account')
size=reference.stat().st_size
if not 0<size<4096:raise SystemExit('Invalid signing-reference size')
security=ctypes.CDLL('/System/Library/Frameworks/Security.framework/Security');core=ctypes.CDLL('/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation');keychain=ctypes.c_void_p()
security.SecKeychainOpen.argtypes=[ctypes.c_char_p,ctypes.POINTER(ctypes.c_void_p)];security.SecKeychainOpen.restype=ctypes.c_int32
security.SecKeychainUnlock.argtypes=[ctypes.c_void_p,ctypes.c_uint32,ctypes.c_void_p,ctypes.c_uint8];security.SecKeychainUnlock.restype=ctypes.c_int32
core.CFRelease.argtypes=[ctypes.c_void_p]
status=security.SecKeychainOpen(os.fsencode(keychain_path),ctypes.byref(keychain))
if status:raise SystemExit('Existing signing keychain open failed: '+str(status))
secret=bytearray(size)
try:
 fd=os.open(reference,os.O_RDONLY|os.O_NOFOLLOW)
 with os.fdopen(fd,'rb') as f:
  meta=os.fstat(f.fileno())
  if meta.st_uid!=os.getuid() or stat.S_IMODE(meta.st_mode)&0o077 or not stat.S_ISREG(meta.st_mode):raise SystemExit('Signing reference changed ownership/type')
  if f.readinto(secret)!=size:raise SystemExit('Signing reference changed size')
 while secret and secret[-1] in (10,13):secret.pop()
 buffer=(ctypes.c_char*len(secret)).from_buffer(secret)
 status=security.SecKeychainUnlock(keychain,len(secret),ctypes.cast(buffer,ctypes.c_void_p),1)
 if status:raise SystemExit('Approved signing unlock failed: '+str(status))
 print('Existing dedicated signing keychain available')
finally:
 for i in range(len(secret)):secret[i]=0
 core.CFRelease(keychain)
