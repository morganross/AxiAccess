//! OS observation adapters. No adapter performs input or accessibility actions.
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(windows)]
pub mod windows;

#[cfg(target_os = "linux")]
pub use linux::{
    capabilities, inspect_window, inspect_window_with_options, list_windows, native_event_revision,
    native_event_status, wait_native_events,
};
#[cfg(target_os = "macos")]
pub use macos::{
    capabilities, inspect_window, inspect_window_with_options, list_windows, native_event_revision,
    native_event_status, wait_native_events,
};
#[cfg(windows)]
pub use windows::{
    capabilities, inspect_window, inspect_window_with_options, list_windows, native_event_revision,
    native_event_status, wait_native_events,
};

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn list_windows() -> Result<crate::engine::WindowListResponse, String> {
    Err("unsupportedPlatform: no native accessibility adapter for this OS".into())
}
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn inspect_window(
    _: &str,
    _: usize,
    _: usize,
) -> Result<crate::engine::InspectResponse, String> {
    Err("unsupportedPlatform: no native accessibility adapter for this OS".into())
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn inspect_window_with_options(
    _: &str,
    _: usize,
    _: usize,
    _: bool,
) -> Result<crate::engine::InspectResponse, String> {
    Err("unsupportedPlatform: no native accessibility adapter for this OS".into())
}
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn capabilities() -> serde_json::Value {
    serde_json::json!({"available":false,"readOnly":true,"platform":std::env::consts::OS})
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn native_event_revision() -> u64 {
    0
}
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn native_event_status() -> serde_json::Value {
    serde_json::json!({"supported":false,"ready":false})
}
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn wait_native_events(_: u64, _: std::time::Duration) -> u64 {
    0
}
