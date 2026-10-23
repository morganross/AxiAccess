use crate::model::{opaque_id, secure_equal, AccessError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub protocol_version: u32,
    pub sender_id: String,
    pub certificate_pem: String,
    pub private_key_pem: String,
    pub certificate_sha256: String,
    pub clients: BTreeMap<String, String>,
    #[serde(default="default_listen")]
    pub listen: std::net::SocketAddr,
}
pub fn default_listen()->std::net::SocketAddr {std::net::SocketAddr::from(([127,0,0,1],8765))}
pub fn default_path() -> PathBuf {
    #[cfg(windows)]
    {
        return PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
            .join("AxiAccess/sender.json");
    }
    #[cfg(target_os = "macos")]
    {
        return PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
            .join("Library/Application Support/AxiAccess/sender.json");
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
            });
        base.join("axiaccess/sender.json")
    }
}
pub fn read(path: &Path) -> Result<Config, Box<dyn std::error::Error>> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Sender config must be a regular private file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        unsafe extern "C" {
            fn geteuid() -> u32;
        }
        if metadata.permissions().mode() & 0o077 != 0 || metadata.uid() != unsafe { geteuid() } {
            return Err(
                "Sender config must be owned by the current user with0600 permissions".into(),
            );
        }
    }
    #[cfg(windows)]
    verify_private_acl(path)?;
    if metadata.len() > 262_144 {
        return Err("Sender config exceeds limit".into());
    }
    let value: Config = serde_json::from_slice(&std::fs::read(path)?)?;
    if value.protocol_version != 1 || value.sender_id.is_empty() {
        return Err("Unsupported or invalid sender configuration".into());
    }
    Ok(value)
}

/// A persistent lock file is separate from the configuration being replaced.
/// Locking sender.json itself would lock its old inode/handle after an atomic
/// replacement and let the next writer bypass the still-active transaction.
/// Never unlink this file on release: waiters must keep the same native object.
struct ConfigTransaction {
    _lock: std::fs::File,
}
impl ConfigTransaction {
    fn acquire(path: &Path, initialize: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let parent = path
            .parent()
            .filter(|value| !value.as_os_str().is_empty() && *value != Path::new("."))
            .ok_or("Sender config requires an explicit private parent directory")?;
        if initialize { create_private_config_directory(parent)?; }
        let parent = parent.canonicalize()?;
        let metadata = std::fs::metadata(&parent)?;
        if !metadata.is_dir() {
            return Err("Sender config parent must be a private directory".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            unsafe extern "C" {
                fn geteuid() -> u32;
            }
            if metadata.uid() != unsafe { geteuid() } || metadata.permissions().mode() & 0o077 != 0 {
                return Err("Sender config directory must be owned by the current user with0700 permissions".into());
            }
        }
        #[cfg(windows)]
        verify_private_acl(&parent)?;
        let mut filename = path.file_name().ok_or("Config requires a file name")?.to_os_string();
        filename.push(".transaction.lock");
        let lock_path = parent.join(filename);
        match std::fs::symlink_metadata(&lock_path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err("Sender config transaction lock must be a regular private file".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE};
            // Keep a persistent lock object; deny deletion while any contender
            // holds a handle, and do not follow a final-component reparse point.
            options.share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
        }
        let lock = options.open(&lock_path)?;
        let metadata = lock.metadata()?;
        if !metadata.is_file() {
            return Err("Sender config transaction lock is not a regular file".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            unsafe extern "C" {
                fn geteuid() -> u32;
            }
            if metadata.uid() != unsafe { geteuid() } || metadata.permissions().mode() & 0o077 != 0 {
                return Err("Sender config transaction lock must be owned by the current user with0600 permissions".into());
            }
        }
        #[cfg(windows)]
        verify_private_acl(&lock_path)?;
        // The OS releases this exclusive lock when the noninherited handle is
        // closed, including process termination. There is no stale lock marker
        // or time-based ownership guess. Read only after this call succeeds.
        lock.lock()?;
        Ok(Self { _lock: lock })
    }
}

/// Only a directory newly created by this operation may receive new access
/// policy. Existing parents are validated by the transaction guard, never
/// chmod'ed or re-ACLed to make an arbitrary --config destination usable.
fn create_private_config_directory(parent: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if std::fs::symlink_metadata(parent).is_ok() { return Ok(()); }
    if let Some(ancestor) = parent.parent().filter(|value| !value.as_os_str().is_empty()) {
        std::fs::create_dir_all(ancestor)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        match std::fs::DirBuilder::new().mode(0o700).create(parent) {
            Ok(()) => {},
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {},
            Err(error) => return Err(error.into()),
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::{core::PCWSTR, Win32::{Foundation::{HLOCAL, LocalFree}, Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES}, Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1}, Storage::FileSystem::CreateDirectoryW}};
        let name: Vec<u16> = parent.as_os_str().encode_wide().chain(Some(0)).collect();
        let sddl: Vec<u16> = format!("D:P(A;OICI;FA;;;{})", current_sid()?).encode_utf16().chain(Some(0)).collect();
        unsafe {
            let mut descriptor = PSECURITY_DESCRIPTOR::default();
            ConvertStringSecurityDescriptorToSecurityDescriptorW(PCWSTR(sddl.as_ptr()), SDDL_REVISION_1, &mut descriptor, None)?;
            let attributes = SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor.0, bInheritHandle: false.into() };
            // Create with the final private ACL atomically: another setup can
            // validate it immediately without a public-then-protected race.
            let result = CreateDirectoryW(PCWSTR(name.as_ptr()), Some(&attributes));
            let _ = LocalFree(HLOCAL(descriptor.0));
            if let Err(error) = result {
                if std::fs::symlink_metadata(parent).is_err() { return Err(error.into()); }
                // A concurrent creator won. The caller validates its actual
                // owner/ACL; it never changes that existing directory here.
            }
        }
    }
    Ok(())
}

pub fn setup(path: &Path, quiet: bool) -> Result<Value, Box<dyn std::error::Error>> {
    let _transaction = ConfigTransaction::acquire(path, true)?;
    if path.exists() {
        let config = read(path)?;
        return Ok(metadata(&config, path, false, quiet));
    }
    let cert =
        rcgen::generate_simple_self_signed(vec!["localhost".into(), "axiaccess.local".into()])?;
    let fingerprint = format!("{:x}", Sha256::digest(cert.cert.der().as_ref()));
    let config = Config {
        protocol_version: 1,
        sender_id: opaque_id(),
        certificate_pem: cert.cert.pem(),
        private_key_pem: cert.key_pair.serialize_pem(),
        certificate_sha256: fingerprint,
        clients: BTreeMap::from([("owner".into(), format!("{}{}", opaque_id(), opaque_id()))]),
        listen: default_listen(),
    };
    save_new(path, &config)?;
    Ok(metadata(&config, path, true, quiet))
}
fn metadata(c: &Config, path: &Path, created: bool, quiet: bool) -> Value {
    let mut v = json!({"created":created,"configPath":path,"senderId":c.sender_id,"certificateSha256":c.certificate_sha256,"certificatePem":c.certificate_pem,"clientId":"owner","listen":c.listen});
    if !quiet {
        v["token"] = json!(c.clients.get("owner"));
    }
    v
}
fn save_new(path: &Path, c: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(&serde_json::to_vec_pretty(c)?)?;
    file.sync_all()?;
    Ok(())
}
pub fn edit_client(
    path: &Path,
    id: &str,
    revoke: bool,
) -> Result<Value, Box<dyn std::error::Error>> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("Client ID must contain1..64 ASCII letters/numbers/-/_".into());
    }
    let _transaction = ConfigTransaction::acquire(path, false)?;
    let mut c = read(path)?;
    if revoke {
        c.clients.remove(id);
    } else if c.clients.contains_key(id) {
        return Err("Client already exists; revoke first to rotate".into());
    } else {
        c.clients
            .insert(id.into(), format!("{}{}", opaque_id(), opaque_id()));
    }
    save_replace(path,&c)?;
    let mut v = json!({"clientId":id,"revoked":revoke,"restartRequired":false});
    if !revoke {
        v["token"] = json!(c.clients[id]);
        v["certificatePem"] = json!(c.certificate_pem);
        v["certificateSha256"] = json!(c.certificate_sha256);
        v["senderId"] = json!(c.sender_id);
    }
    Ok(v)
}
pub fn configure_listen(path:&Path,listen:std::net::SocketAddr)->Result<Value,Box<dyn std::error::Error>>{
    if listen.port()==0||listen.ip().is_multicast(){return Err("Configured sender requires a unicast/interface address and a nonzero port".into());}
    let _transaction=ConfigTransaction::acquire(path,false)?;
    let mut config=read(path)?;config.listen=listen;save_replace(path,&config)?;
    Ok(json!({"senderId":config.sender_id,"listen":listen,"restartRequired":true,"certificateSha256":config.certificate_sha256}))
}
fn save_replace(path:&Path,c:&Config)->Result<(),Box<dyn std::error::Error>>{
    let tmp=path.with_extension(format!("{}.tmp",opaque_id()));save_new(&tmp,c)?;
    // Windows rename does not replace an existing file. Use ReplaceFileW to keep
    // the original on failed replacement, rather than deleting credentials first.
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{ReplaceFileW, REPLACE_FILE_FLAGS};
        let a: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<u16> = tmp.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            ReplaceFileW(
                PCWSTR(a.as_ptr()),
                PCWSTR(b.as_ptr()),
                None,
                REPLACE_FILE_FLAGS(0),
                None,
                None,
            )?;
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(tmp, path)?;
    Ok(())
}
pub fn authorize(config: &Config, client: &str, token: &str) -> Result<(), AccessError> {
    if config
        .clients
        .get(client)
        .is_some_and(|expected| secure_equal(expected, token))
    {
        Ok(())
    } else {
        Err(AccessError::new(
            "unauthorized",
            "A paired sender credential is required",
        ))
    }
}

#[cfg(windows)]
fn current_sid() -> Result<&'static str, Box<dyn std::error::Error>> {
    use std::sync::OnceLock;
    static SID: OnceLock<Result<String, String>> = OnceLock::new();
    use windows::{
        core::PWSTR,
        Win32::{
            Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL},
            Security::Authorization::ConvertSidToStringSidW,
            Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER},
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        },
    };
    match SID.get_or_init(|| unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|e| e.to_string())?;
        let result = (|| -> Result<String, String> {
            let mut size = 0;
            let _ = GetTokenInformation(token, TokenUser, None, 0, &mut size);
            if size == 0 || size > 65536 {
                return Err("Invalid token user information size".into());
            }
            let mut data = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
            GetTokenInformation(
                token,
                TokenUser,
                Some(data.as_mut_ptr().cast()),
                size,
                &mut size,
            )
            .map_err(|e| e.to_string())?;
            let user = &*(data.as_ptr() as *const TOKEN_USER);
            let mut text = PWSTR::null();
            ConvertSidToStringSidW(user.User.Sid, &mut text).map_err(|e| e.to_string())?;
            let result = text.to_string().map_err(|e| e.to_string());
            let _ = LocalFree(HLOCAL(text.0.cast()));
            result
        })();
        let _ = CloseHandle(token);
        result
    }) {
        Ok(s) => Ok(s),
        Err(e) => Err(e.clone().into()),
    }
}
#[cfg(windows)]
fn verify_private_acl(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{core::PCWSTR, Win32::{Foundation::{LocalFree,HLOCAL},
        Security::{ACL,DACL_SECURITY_INFORMATION,PSECURITY_DESCRIPTOR},
        Security::Authorization::{GetNamedSecurityInfoW,SE_FILE_OBJECT}}};
    let filename: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        let mut dacl: *mut ACL = std::ptr::null_mut();
        GetNamedSecurityInfoW(PCWSTR(filename.as_ptr()), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION,
            None,None,Some(&mut dacl),None,&mut descriptor).ok()?;
        let result = validate_owner_acl(dacl);
        let _ = LocalFree(HLOCAL(descriptor.0));
        result
    }
}

#[cfg(windows)]
unsafe fn validate_owner_acl(dacl: *mut windows::Win32::Security::ACL) -> Result<(),Box<dyn std::error::Error>> {
    use windows::{core::PWSTR,Win32::{Foundation::{LocalFree,HLOCAL},
        Security::{GetAce,ACCESS_ALLOWED_ACE,PSID},
        Security::Authorization::ConvertSidToStringSidW}};
    if dacl.is_null() || (*dacl).AceCount == 0 {
        return Err("Sender credentials require an owner-only ACL".into());
    }
    // ReplaceFileW can retain equivalent explicit and inherited ACEs for the
    // same owner. Validate every principal, not a particular ACE count.
    for index in 0..(*dacl).AceCount {
        let mut ace = std::ptr::null_mut();
        GetAce(dacl,index as u32,&mut ace)?;
        let allowed = &*(ace as *const ACCESS_ALLOWED_ACE);
        if allowed.Header.AceType != 0 { return Err("Unexpected sender credential ACL".into()); }
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(PSID((&allowed.SidStart as *const u32).cast_mut().cast()),&mut text)?;
        let actual = text.to_string();
        let _ = LocalFree(HLOCAL(text.0.cast()));
        if actual? != current_sid()? { return Err("Sender credential owner does not match this user".into()); }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    struct Fixture { directory: PathBuf, config: PathBuf }
    impl Fixture {
        fn empty() -> Self {
            let directory = std::env::temp_dir().join(format!("axiaccess-config-transaction-{}", opaque_id()));
            let config = directory.join("sender.json");
            drop(ConfigTransaction::acquire(&config, true).expect("create isolated private fixture"));
            Self { directory, config }
        }
        fn configured() -> Self {
            let fixture = Self::empty();
            setup(&fixture.config, true).expect("initialize isolated fixture");
            fixture
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            // Only this test's explicitly named private temporary directory.
            if self.directory.starts_with(std::env::temp_dir())
                && self.directory.file_name().is_some_and(|name| name.to_string_lossy().starts_with("axiaccess-config-transaction-")) {
                let _ = std::fs::remove_dir_all(&self.directory);
            }
        }
    }
    struct Actor { child: Child, ready: PathBuf, result: PathBuf }
    impl Actor {
        fn spawn(fixture: &Fixture, name: &str, mode: &str) -> Self {
            let ready = fixture.directory.join(format!("{name}.ready"));
            let result = fixture.directory.join(format!("{name}.public-result.json"));
            let mut command = Command::new(std::env::current_exe().expect("test executable"));
            command.args(["--exact", "transport::config::tests::transaction_child", "--nocapture"])
                .env("AXIACCESS_TRANSACTION_TEST_MODE", mode)
                .env("AXIACCESS_TRANSACTION_TEST_CONFIG", &fixture.config)
                .env("AXIACCESS_TRANSACTION_TEST_CLIENT", name)
                .env("AXIACCESS_TRANSACTION_TEST_READY", &ready)
                .env("AXIACCESS_TRANSACTION_TEST_RESULT", &result)
                .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
            if mode == "bare" { command.current_dir(fixture.directory.join("unrelated-cwd")); }
            let child = command.spawn().expect("spawn real transaction subprocess");
            Self { child, ready, result }
        }
        fn wait_ready(&mut self) {
            let deadline = Instant::now() + Duration::from_secs(15);
            while !self.ready.exists() {
                assert!(self.child.try_wait().expect("query child").is_none(), "transaction subprocess exited before its barrier");
                assert!(Instant::now() < deadline, "transaction subprocess barrier timed out");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        fn finish(&mut self) {
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                if let Some(status) = self.child.try_wait().expect("query child") {
                    assert!(status.success(), "transaction subprocess failed with {status}");
                    return;
                }
                assert!(Instant::now() < deadline, "transaction subprocess completion timed out");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    impl Drop for Actor {
        fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); }
    }

    // Runs only when the parent explicitly selects this test in a fresh OS
    // process. No production-only timing hooks or privileged config are used.
    #[test]
    fn transaction_child() {
        let Ok(mode) = std::env::var("AXIACCESS_TRANSACTION_TEST_MODE") else { return; };
        let config = PathBuf::from(std::env::var_os("AXIACCESS_TRANSACTION_TEST_CONFIG").expect("fixture path"));
        let ready = PathBuf::from(std::env::var_os("AXIACCESS_TRANSACTION_TEST_READY").expect("barrier path"));
        let result = PathBuf::from(std::env::var_os("AXIACCESS_TRANSACTION_TEST_RESULT").expect("result path"));
        let client = std::env::var("AXIACCESS_TRANSACTION_TEST_CLIENT").expect("fixture client");
        let value = if mode == "hold" {
            let _transaction = ConfigTransaction::acquire(&config, false).expect("hold native lock");
            std::fs::write(ready, b"locked").expect("signal held lock");
            std::thread::sleep(Duration::from_secs(60));
            return;
        } else {
            std::fs::write(ready, b"ready").expect("signal mutation intent");
            match mode.as_str() {
                "setup" => setup(&config, true).expect("concurrent setup"),
                "pair" => edit_client(&config, &client, false).expect("concurrent pair"),
                "revoke" => edit_client(&config, "victim", true).expect("concurrent revoke"),
                "configure" => configure_listen(&config, "127.0.0.1:19765".parse().unwrap()).expect("concurrent configure"),
                "bare" => {
                    assert!(setup(Path::new("sender.json"), true).is_err(), "a bare config must be rejected before touching CWD");
                    assert!(setup(Path::new("./sender.json"), true).is_err(), "a current-directory alias must be rejected before touching CWD");
                    json!({"created":false})
                },
                _ => panic!("Unknown isolated test mode"),
            }
        };
        let public = json!({"created":value["created"],"senderId":read(&config).expect("read fixture").sender_id});
        // No token, private key, application text or real configuration output.
        std::fs::write(result, serde_json::to_vec(&public).unwrap()).expect("write public fixture result");
    }

    #[test]
    fn concurrent_process_mutations_preserve_revocation_pairing_and_identity() {
        let fixture = Fixture::configured();
        let original_acl = permission_fingerprint(&fixture.config);
        edit_client(&fixture.config, "victim", false).unwrap();
        let before = read(&fixture.config).unwrap_or_else(|error| {
            #[cfg(windows)] let owner = current_sid().unwrap();
            #[cfg(not(windows))] let owner = "";
            panic!("{error}; private fixture original ACL={}, replaced ACL={}, directory ACL={}",
                original_acl.replace(owner,"OWNER"), permission_fingerprint(&fixture.config).replace(owner,"OWNER"),
                permission_fingerprint(&fixture.directory).replace(owner,"OWNER"));
        });
        let transaction = ConfigTransaction::acquire(&fixture.config, false).unwrap();
        let mut actors = vec![Actor::spawn(&fixture, "configure", "configure"), Actor::spawn(&fixture, "revoke", "revoke")];
        for index in 0..6 { actors.push(Actor::spawn(&fixture, &format!("new-client-{index}"), "pair")); }
        for actor in &mut actors { actor.wait_ready(); }
        std::thread::sleep(Duration::from_millis(120));
        for actor in &mut actors { assert!(actor.child.try_wait().unwrap().is_none(), "writer bypassed the held native transaction lock"); }
        drop(transaction);
        for actor in &mut actors { actor.finish(); }
        let after = read(&fixture.config).unwrap();
        assert!(!after.clients.contains_key("victim"), "a concurrent writer resurrected a revoked client");
        for index in 0..6 { assert!(after.clients.contains_key(&format!("new-client-{index}")), "a paired client was lost"); }
        assert_eq!(after.listen, "127.0.0.1:19765".parse().unwrap());
        assert!(before.sender_id == after.sender_id && before.certificate_pem == after.certificate_pem
            && before.private_key_pem == after.private_key_pem && before.certificate_sha256 == after.certificate_sha256
            && before.clients["owner"] == after.clients["owner"], "unrelated sender identity changed");
    }

    #[test]
    fn concurrent_process_setup_creates_one_identity_and_retains_private_files() {
        let fixture = Fixture::empty();
        let transaction = ConfigTransaction::acquire(&fixture.config, false).unwrap();
        let mut actors: Vec<_> = (0..6).map(|index| Actor::spawn(&fixture, &format!("setup-{index}"), "setup")).collect();
        for actor in &mut actors { actor.wait_ready(); }
        std::thread::sleep(Duration::from_millis(120));
        for actor in &mut actors { assert!(actor.child.try_wait().unwrap().is_none(), "setup bypassed the held native transaction lock"); }
        drop(transaction);
        for actor in &mut actors { actor.finish(); }
        let config = read(&fixture.config).unwrap();
        let mut created = 0;
        for actor in &actors {
            let public: Value = serde_json::from_slice(&std::fs::read(&actor.result).unwrap()).unwrap();
            assert_eq!(public["senderId"], config.sender_id);
            if public["created"] == true { created += 1; }
        }
        assert_eq!(created, 1);
        let bytes = std::fs::read(&fixture.config).unwrap();
        setup(&fixture.config, true).unwrap();
        assert!(bytes == std::fs::read(&fixture.config).unwrap(), "idempotent setup changed sender bytes");
        #[cfg(windows)] { verify_private_acl(&fixture.config).unwrap(); verify_private_acl(&fixture.directory.join("sender.json.transaction.lock")).unwrap(); }
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&fixture.directory).unwrap().permissions().mode() & 0o777, 0o700);
            for path in [&fixture.config, &fixture.directory.join("sender.json.transaction.lock")] {
                assert_eq!(std::fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o600);
            }
        }
    }

    #[test]
    fn native_transaction_lock_is_released_when_holder_process_exits() {
        let fixture = Fixture::configured();
        let mut holder = Actor::spawn(&fixture, "holder", "hold");
        holder.wait_ready();
        let mut writer = Actor::spawn(&fixture, "after-crash", "pair");
        writer.wait_ready();
        std::thread::sleep(Duration::from_millis(120));
        assert!(writer.child.try_wait().unwrap().is_none(), "writer bypassed the live holder");
        holder.child.kill().unwrap(); holder.child.wait().unwrap();
        writer.finish();
        assert!(read(&fixture.config).unwrap().clients.contains_key("after-crash"));
        assert!(fixture.directory.join("sender.json.transaction.lock").is_file(), "the native lock object must persist across transactions");
    }

    #[cfg(unix)]
    fn permission_fingerprint(path: &Path) -> String {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let metadata = std::fs::metadata(path).unwrap();
        format!("{}:{}:{}", metadata.uid(), metadata.gid(), metadata.permissions().mode())
    }
    #[cfg(windows)]
    fn permission_fingerprint(path: &Path) -> String {
        use std::os::windows::ffi::OsStrExt;
        use windows::{core::{PCWSTR, PWSTR}, Win32::{Foundation::{HLOCAL, LocalFree}, Security::{DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR}, Security::Authorization::{GetNamedSecurityInfoW, ConvertSecurityDescriptorToStringSecurityDescriptorW, SE_FILE_OBJECT, SDDL_REVISION_1}}};
        let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            let mut descriptor = PSECURITY_DESCRIPTOR::default();
            GetNamedSecurityInfoW(PCWSTR(name.as_ptr()), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION,
                None, None, None, None, &mut descriptor).ok().unwrap();
            let mut text = PWSTR::null();
            let converted = ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor, SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION, &mut text, None);
            let result = converted.map(|_| text.to_string().unwrap());
            let _ = LocalFree(HLOCAL(descriptor.0));
            if !text.is_null() { let _ = LocalFree(HLOCAL(text.0.cast())); }
            result.unwrap()
        }
    }
    #[test]
    fn bare_config_rejects_without_changing_cwd_files_or_permissions() {
        let fixture = Fixture::configured();
        let cwd = fixture.directory.join("unrelated-cwd");
        std::fs::create_dir(&cwd).unwrap();
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&cwd, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(cwd.join("keep.txt"), b"unrelated current directory").unwrap();
        let permissions = permission_fingerprint(&cwd);
        let mut actor = Actor::spawn(&fixture, "bare-path", "bare");
        actor.finish();
        assert!(permissions == permission_fingerprint(&cwd), "bare config changed unrelated CWD permissions");
        let entries: Vec<_> = std::fs::read_dir(&cwd).unwrap().map(|entry| entry.unwrap().file_name()).collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("keep.txt")]);
        assert_eq!(std::fs::read(cwd.join("keep.txt")).unwrap(), b"unrelated current directory");
    }

    #[cfg(windows)]
    #[test]
    fn owner_acl_accepts_equivalent_owner_entries_but_rejects_foreign_principals() {
        use windows::{core::PCWSTR,Win32::{Foundation::{BOOL,HLOCAL,LocalFree},Security::{GetSecurityDescriptorDacl,PSECURITY_DESCRIPTOR},Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW,SDDL_REVISION_1}}};
        let owner = current_sid().unwrap();
        for (sddl,accepted) in [
            (format!("D:(A;;FA;;;{owner})(A;ID;FA;;;{owner})"),true),
            (format!("D:(A;;FA;;;{owner})(A;;FR;;;WD)"),false),
            (format!("D:(A;;FA;;;{owner})(D;;FR;;;WD)"),false),
        ] {
            let text: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
            unsafe {
                let mut descriptor = PSECURITY_DESCRIPTOR::default();
                ConvertStringSecurityDescriptorToSecurityDescriptorW(PCWSTR(text.as_ptr()),SDDL_REVISION_1,&mut descriptor,None).unwrap();
                let mut present = BOOL(0); let mut defaulted = BOOL(0); let mut dacl = std::ptr::null_mut();
                GetSecurityDescriptorDacl(descriptor,&mut present,&mut dacl,&mut defaulted).unwrap();
                let result = validate_owner_acl(dacl).is_ok();
                let _ = LocalFree(HLOCAL(descriptor.0));
                assert_eq!(result,accepted,"owner-only policy accepted an unexpected principal/ACE");
            }
        }
    }
}
