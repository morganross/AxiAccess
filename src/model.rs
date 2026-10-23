use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME: usize = 262_144;
pub const MAX_TEXT_PAGE: usize = 16_384;
pub const MAX_CAPTURE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_RETAINED_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CAPTURE_COUNT: usize = 32;

pub fn opaque_id() -> String {
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}
impl AccessError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable: matches!(code, "busy" | "timeout" | "backend_unavailable"),
        }
    }
    pub fn native(message: String) -> Self {
        let lower = message.to_lowercase();
        let code = if lower.contains("permission")
            || lower.contains("denied")
            || lower.contains("trusted")
        {
            "permission_denied"
        } else if lower.contains("session") || lower.contains("bus") {
            "session_unavailable"
        } else if lower.contains("timeout") || lower.contains("deadline") {
            "timeout"
        } else if lower.contains("not found") || lower.contains("ambiguous") {
            "target_unavailable"
        } else {
            "backend_unavailable"
        };
        Self::new(code, message)
    }
}
impl std::fmt::Display for AccessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for AccessError {}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub id: String,
    #[serde(
        default,
        rename = "clientScope",
        skip_serializing_if = "Option::is_none"
    )]
    pub client_scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    pub operation: String,
    #[serde(default)]
    pub params: Value,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AccessError>,
}
impl Response {
    pub fn from_result(id: String, result: Result<Value, AccessError>) -> Self {
        match result {
            Ok(result) => Self {
                id,
                result: Some(result),
                error: None,
            },
            Err(error) => Self {
                id,
                result: None,
                error: Some(error),
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    #[serde(default = "default_nodes")]
    pub max_nodes: usize,
    #[serde(default = "default_depth")]
    pub max_depth: usize,
}
fn default_nodes() -> usize {
    250
}
fn default_depth() -> usize {
    32
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_nodes: 250,
            max_depth: 32,
        }
    }
}
impl Limits {
    pub fn checked(self) -> Result<Self, AccessError> {
        if self.max_nodes == 0
            || self.max_nodes > 1000
            || self.max_depth == 0
            || self.max_depth > 64
        {
            return Err(AccessError::new(
                "invalid_arguments",
                "maxNodes must be 1..1000 and maxDepth 1..64",
            ));
        }
        Ok(self)
    }
}
pub fn secure_equal(a: &str, b: &str) -> bool {
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= (a.as_bytes().get(i).copied().unwrap_or(0)
            ^ b.as_bytes().get(i).copied().unwrap_or(0)) as usize;
    }
    diff == 0
}
