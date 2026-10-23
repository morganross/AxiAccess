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
pub fn setup(path: &Path, quiet: bool) -> Result<Value, Box<dyn std::error::Error>> {
    if path.exists() {
        let config = read(path)?;
        return Ok(metadata(&config, path, false, quiet));
    }
    let parent = path.parent().ok_or("Config requires a parent directory")?;
    std::fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
    }
    protect_directory(parent)?;
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
fn protect_directory(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            core::PCWSTR,
            Win32::{
                Foundation::{LocalFree, BOOL, HLOCAL},
                Security::Authorization::{
                    ConvertStringSecurityDescriptorToSecurityDescriptorW, SetNamedSecurityInfoW,
                    SDDL_REVISION_1, SE_FILE_OBJECT,
                },
                Security::{
                    GetSecurityDescriptorDacl, DACL_SECURITY_INFORMATION,
                    PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
                },
            },
        };
        let sddl: Vec<u16> = format!("D:P(A;OICI;FA;;;{})", current_sid()?)
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let filename: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            let mut descriptor = PSECURITY_DESCRIPTOR::default();
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )?;
            let mut present = BOOL(0);
            let mut defaulted = BOOL(0);
            let mut dacl = std::ptr::null_mut();
            let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted)?;
                if !present.as_bool() || dacl.is_null() {
                    return Err("Private sender ACL missing".into());
                }
                SetNamedSecurityInfoW(
                    PCWSTR(filename.as_ptr()),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    PSID::default(),
                    PSID::default(),
                    Some(dacl),
                    None,
                )
                .ok()?;
                Ok(())
            })();
            let _ = LocalFree(HLOCAL(descriptor.0));
            result?;
        }
        verify_private_acl(path)?;
    }
    #[cfg(not(windows))]
    let _ = path;
    Ok(())
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
    use windows::{
        core::{PCWSTR, PWSTR},
        Win32::{
            Foundation::{LocalFree, HLOCAL},
            Security::Authorization::{
                ConvertSidToStringSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
            },
            Security::{
                GetAce, ACCESS_ALLOWED_ACE, ACL, DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
                PSID,
            },
        },
    };
    let filename: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        let mut dacl: *mut ACL = std::ptr::null_mut();
        GetNamedSecurityInfoW(
            PCWSTR(filename.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(&mut dacl),
            None,
            &mut descriptor,
        )
        .ok()?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            if dacl.is_null() || (*dacl).AceCount != 1 {
                return Err("Sender credentials require an owner-only ACL".into());
            }
            let mut ace = std::ptr::null_mut();
            GetAce(dacl, 0, &mut ace)?;
            let allowed = &*(ace as *const ACCESS_ALLOWED_ACE);
            if allowed.Header.AceType != 0 {
                return Err("Unexpected sender credential ACL".into());
            }
            let mut text = PWSTR::null();
            ConvertSidToStringSidW(
                PSID((&allowed.SidStart as *const u32).cast_mut().cast()),
                &mut text,
            )?;
            let actual = text.to_string()?;
            let _ = LocalFree(HLOCAL(text.0.cast()));
            if actual != current_sid()? {
                return Err("Sender credential owner does not match this user".into());
            }
            Ok(())
        })();
        let _ = LocalFree(HLOCAL(descriptor.0));
        result
    }
}
