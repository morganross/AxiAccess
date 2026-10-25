use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

// ============================================================================
// DATA STRUCTURES FOR LOW-LEVEL OBSERVATION & CONTROL
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlActionRequest {
    pub target: String,
    /// Action type: "invoke" | "set_value" | "send_keys" | "click_xy" | "toggle" | "select" | "expand" | "collapse" | "scroll" | "menu" | "window_state"
    pub action: String,
    /// Optional element ID (e.g. "@e5"), element name substring, or menu item name
    pub selector: Option<String>,
    /// Optional role filter (e.g. "button", "edit", "menuitem")
    pub role: Option<String>,
    /// Value / text / keys payload for set_value, send_keys, menu, or window_state
    pub value: Option<String>,
    /// Optional client X coordinate for click_xy or scroll delta X
    pub x: Option<i32>,
    /// Optional client Y coordinate for click_xy or scroll delta Y
    pub y: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlActionResponse {
    pub success: bool,
    pub target_hwnd: isize,
    pub target_process: String,
    pub target_title: String,
    pub action: String,
    pub method_used: String,
    pub matched_element: Option<String>,
    pub details: String,
    pub execution_time_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrWordBox {
    pub text: String,
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrLineBox {
    pub line_index: usize,
    pub text: String,
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
    pub words: Vec<OcrWordBox>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrWindowResponse {
    pub hwnd: isize,
    pub pid: u32,
    pub process: String,
    pub title: String,
    pub capture_method: String,
    pub window_width: i32,
    pub window_height: i32,
    pub line_count: usize,
    pub word_count: usize,
    pub char_count: usize,
    pub scan_time_ms: f64,
    pub full_text: String,
    pub lines: Vec<OcrLineBox>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitChainNode {
    pub object_type: String,
    pub object_status: String,
    pub object_name: String,
    pub process_id: u32,
    pub thread_id: u32,
    pub wait_time: u32,
    pub context_switches: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildProcessNode {
    pub pid: u32,
    pub parent_pid: u32,
    pub name: String,
    pub threads: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpEndpointInfo {
    pub pid: u32,
    pub process: String,
    pub local_addr: String,
    pub local_port: u16,
    pub remote_addr: String,
    pub remote_port: u16,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessTelemetryResponse {
    pub hwnd: isize,
    pub pid: u32,
    pub ui_thread_id: u32,
    pub process: String,
    pub title: String,
    pub is_hung_window: bool,
    pub wait_chain_status: String,
    pub is_deadlocked: bool,
    pub wait_chain: Vec<WaitChainNode>,
    pub working_set_mb: f64,
    pub private_bytes_mb: f64,
    pub handle_count: u32,
    pub gdi_objects: u32,
    pub user_objects: u32,
    pub io_read_ops: u64,
    pub io_write_ops: u64,
    pub io_read_bytes: u64,
    pub io_write_bytes: u64,
    pub child_processes: Vec<ChildProcessNode>,
    pub tcp_connections: Vec<TcpEndpointInfo>,
    pub menu_items: Vec<String>,
    pub scan_time_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpTargetInfo {
    pub port: u16,
    pub pid: u32,
    pub process: String,
    pub target_id: String,
    pub target_type: String,
    pub title: String,
    pub url: String,
    pub websocket_debugger_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DotNetPipeInfo {
    pub pid: u32,
    pub process: String,
    pub pipe_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotObjectInfo {
    pub display_name: String,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugChannelsResponse {
    pub scan_time_ms: f64,
    pub cdp_targets: Vec<CdpTargetInfo>,
    pub dotnet_diagnostic_pipes: Vec<DotNetPipeInfo>,
    pub com_running_objects: Vec<RotObjectInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleBufferResponse {
    pub pid: u32,
    pub process: String,
    pub buffer_width: i16,
    pub buffer_height: i16,
    pub cursor_x: i16,
    pub cursor_y: i16,
    pub visible_lines: usize,
    pub total_non_empty_lines: usize,
    pub text: String,
    pub scan_time_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxDesktopResponse {
    pub action: String,
    pub desktop_name: String,
    pub available_desktops: Vec<String>,
    pub spawned_pid: Option<u32>,
    pub sandbox_windows: Vec<String>,
    pub status: String,
}

// ============================================================================
// PUBLIC CROSS-PLATFORM DISPATCHERS
// ============================================================================

pub fn control_window(req: ControlActionRequest) -> Result<ControlActionResponse, String> {
    #[cfg(windows)]
    {
        return win_lowlevel::control_window(req);
    }
    #[cfg(not(windows))]
    {
        let _ = req;
        Err("Low-level control is currently implemented for Windows native.".to_string())
    }
}

pub fn ocr_window(target_id: &str) -> Result<OcrWindowResponse, String> {
    #[cfg(windows)]
    {
        return win_lowlevel::ocr_window(target_id);
    }
    #[cfg(not(windows))]
    {
        let _ = target_id;
        Err("Native DWM + Windows.Media.Ocr is only available on Windows.".to_string())
    }
}

pub fn inspect_telemetry(target_id: &str) -> Result<ProcessTelemetryResponse, String> {
    #[cfg(windows)]
    {
        return win_lowlevel::inspect_telemetry(target_id);
    }
    #[cfg(not(windows))]
    {
        let _ = target_id;
        Err("Wait Chain Traversal & Win32 Telemetry is only available on Windows.".to_string())
    }
}

pub fn discover_debug_channels() -> DebugChannelsResponse {
    #[cfg(windows)]
    {
        return win_lowlevel::discover_debug_channels();
    }
    #[cfg(not(windows))]
    {
        DebugChannelsResponse {
            scan_time_ms: 0.0,
            cdp_targets: vec![],
            dotnet_diagnostic_pipes: vec![],
            com_running_objects: vec![],
        }
    }
}

pub fn read_console_buffer(target: &str) -> Result<ConsoleBufferResponse, String> {
    #[cfg(windows)]
    {
        return win_lowlevel::read_console_buffer(target);
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        Err("Win32 Console Screen Buffer API is only available on Windows.".to_string())
    }
}

pub fn write_console_input(target: &str, text: &str, send_enter: bool) -> Result<serde_json::Value, String> {
    #[cfg(windows)]
    {
        return win_lowlevel::write_console_input(target, text, send_enter);
    }
    #[cfg(not(windows))]
    {
        let _ = (target, text, send_enter);
        Err("Win32 Console Input API is only available on Windows.".to_string())
    }
}

pub fn manage_sandbox_desktop(
    action: &str,
    desktop_name: &str,
    spawn_cmd: Option<&str>,
) -> Result<SandboxDesktopResponse, String> {
    #[cfg(windows)]
    {
        return win_lowlevel::manage_sandbox_desktop(action, desktop_name, spawn_cmd);
    }
    #[cfg(not(windows))]
    {
        let _ = (action, desktop_name, spawn_cmd);
        Err("Win32 HDESK Sandbox API is only available on Windows.".to_string())
    }
}

// ============================================================================
// WINDOWS NATIVE LOW-LEVEL IMPLEMENTATION
// ============================================================================
#[cfg(windows)]
mod win_lowlevel {
    use super::*;
    use crate::engine;
    use std::ffi::{c_void, OsString};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows::core::{Interface, BSTR, PCWSTR, PWSTR};
    use windows::Graphics::Imaging::BitmapDecoder;
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};
    use windows::Win32::Foundation::{BOOL, CloseHandle, HANDLE, HWND, LPARAM, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, SRCCOPY,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FindClose, FindFirstFileW, FindNextFileW, FILE_GENERIC_READ,
        FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, WIN32_FIND_DATAW,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CreateBindCtx, GetRunningObjectTable, IMoniker,
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Console::{
        AttachConsole, FreeConsole, GetConsoleScreenBufferInfo, ReadConsoleOutputCharacterW,
        WriteConsoleInputW, CONSOLE_SCREEN_BUFFER_INFO, COORD, INPUT_RECORD, KEY_EVENT,
        KEY_EVENT_RECORD, KEY_EVENT_RECORD_0,
    };
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::StationsAndDesktops::{
        CreateDesktopW, EnumDesktopWindows, EnumDesktopsW, GetProcessWindowStation, OpenDesktopW,
        SetThreadDesktop, DESKTOP_CONTROL_FLAGS,
    };
    use windows::Win32::System::Threading::{
        CreateProcessW, GetProcessHandleCount, GetProcessIoCounters, OpenProcess,
        QueryFullProcessImageNameW, IO_COUNTERS, PROCESS_INFORMATION, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ, STARTUPINFOW,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationExpandCollapsePattern,
        IUIAutomationInvokePattern, IUIAutomationScrollPattern, IUIAutomationSelectionItemPattern,
        IUIAutomationTogglePattern, IUIAutomationTreeWalker, IUIAutomationValuePattern,
        ScrollAmount_LargeDecrement, ScrollAmount_LargeIncrement, ScrollAmount_NoAmount,
        UIA_ExpandCollapsePatternId, UIA_InvokePatternId, UIA_ScrollPatternId,
        UIA_SelectionItemPatternId, UIA_TogglePatternId, UIA_ValuePatternId,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetMenu, GetMenuItemCount, GetMenuItemID, GetMenuStringW, GetSubMenu, GetWindowRect,
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsHungAppWindow,
        PostMessageW, SendMessageW, ShowWindow, BM_CLICK, MF_BYPOSITION, SW_MAXIMIZE,
        SW_MINIMIZE, SW_RESTORE, WM_CHAR, WM_CLOSE, WM_COMMAND, WM_KEYDOWN, WM_KEYUP,
        WM_LBUTTONDOWN, WM_LBUTTONUP, WM_SETTEXT,
    };

    // Direct FFI bindings for Wait Chain Traversal, PrintWindow, Psapi, and IpHlpApi
    #[link(name = "user32")]
    extern "system" {
        fn PrintWindow(hwnd: HWND, hdcblt: *mut c_void, nflags: u32) -> BOOL;
        fn GetGuiResources(hprocess: HANDLE, uiflags: u32) -> u32;
    }

    #[link(name = "psapi")]
    extern "system" {
        fn K32GetProcessMemoryInfo(
            process: HANDLE,
            ppsmemcounters: *mut PROCESS_MEMORY_COUNTERS_EX,
            cb: u32,
        ) -> BOOL;
    }

    #[repr(C)]
    #[derive(Default)]
    struct PROCESS_MEMORY_COUNTERS_EX {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
        private_usage: usize,
    }

    #[link(name = "advapi32")]
    extern "system" {
        fn OpenThreadWaitChainSession(flags: u32, callback: *const c_void) -> *mut c_void;
        fn CloseThreadWaitChainSession(wct_handle: *mut c_void);
        fn GetThreadWaitChain(
            wct_handle: *mut c_void,
            context: usize,
            flags: u32,
            thread_id: u32,
            node_count: *mut u32,
            node_info_array: *mut WAITCHAIN_NODE_INFO,
            is_cycle: *mut i32,
        ) -> BOOL;
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct WAITCHAIN_NODE_INFO {
        object_type: u32,
        object_status: u32,
        union_data: [u8; 280],
    }

    #[link(name = "iphlpapi")]
    extern "system" {
        fn GetExtendedTcpTable(
            p_tcp_table: *mut u8,
            pdw_size: *mut u32,
            b_order: i32,
            ul_af: u32,
            table_class: u32,
            reserved: u32,
        ) -> u32;
    }

    fn init_uia() -> Result<IUIAutomation, String> {
        unsafe {
            let wide: Vec<u16> = "Default\0".encode_utf16().collect();
            if let Ok(hdesk) = OpenDesktopW(
                PCWSTR(wide.as_ptr()),
                DESKTOP_CONTROL_FLAGS(0),
                false,
                0x01FF,
            ) {
                let _ = SetThreadDesktop(hdesk);
            }
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("CoCreateInstance(CUIAutomation) failed: {}", e))
        }
    }

    fn get_process_name(pid: u32) -> String {
        if pid == 0 {
            return "system".to_string();
        }
        unsafe {
            if let Ok(h_proc) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                let mut buf = [0u16; 512];
                let mut size = buf.len() as u32;
                let ok = QueryFullProcessImageNameW(
                    h_proc,
                    PROCESS_NAME_WIN32,
                    PWSTR(buf.as_mut_ptr()),
                    &mut size,
                );
                let _ = CloseHandle(h_proc);
                if ok.is_ok() && size > 0 {
                    let path = OsString::from_wide(&buf[..size as usize])
                        .to_string_lossy()
                        .into_owned();
                    if let Some(name) = path.rsplit('\\').next() {
                        return name.to_string();
                    }
                    return path;
                }
            }
        }
        format!("pid:{}", pid)
    }

    fn resolve_target_window(target_id: &str) -> Result<engine::WindowInfo, String> {
        let list = engine::list_windows();
        let t_low = target_id.trim().to_lowercase();
        if t_low == "focused" || t_low == "active" {
            if let Some(w) = list.windows.iter().find(|w| w.focused) {
                return Ok(w.clone());
            }
        }
        for w in &list.windows {
            if w.hwnd.to_string() == target_id.trim() || w.pid.to_string() == target_id.trim() {
                return Ok(w.clone());
            }
        }
        for w in &list.windows {
            if w.title.to_lowercase().contains(&t_low)
                || w.process.to_lowercase().contains(&t_low)
                || w.class_name.to_lowercase().contains(&t_low)
            {
                return Ok(w.clone());
            }
        }
        Err(format!("Target window '{}' not found", target_id))
    }

    fn ctrl_type_name(ctrl_id: i32) -> &'static str {
        match ctrl_id {
            50000 => "button",
            50001 => "calendar",
            50002 => "checkbox",
            50003 => "combobox",
            50004 => "edit",
            50005 => "link",
            50006 => "image",
            50007 => "listitem",
            50008 => "list",
            50009 => "menu",
            50010 => "menubar",
            50011 => "menuitem",
            50012 => "progressbar",
            50013 => "radiobutton",
            50014 => "scrollbar",
            50015 => "slider",
            50016 => "spinner",
            50017 => "statusbar",
            50018 => "tab",
            50019 => "tabitem",
            50020 => "text",
            50021 => "toolbar",
            50022 => "tooltip",
            50023 => "tree",
            50024 => "treeitem",
            50025 => "custom",
            50026 => "group",
            50027 => "thumb",
            50028 => "datagrid",
            50029 => "dataitem",
            50030 => "document",
            50031 => "splitbutton",
            50032 => "window",
            50033 => "pane",
            _ => "custom",
        }
    }

    fn find_matching_uia_element(
        uia: &IUIAutomation,
        root: &IUIAutomationElement,
        selector: Option<&str>,
        role_filter: Option<&str>,
    ) -> Option<(IUIAutomationElement, String, String, String)> {
        let walker: IUIAutomationTreeWalker = unsafe { uia.ControlViewWalker().ok()? };
        let sel_low = selector.unwrap_or("").trim().to_lowercase();
        let role_low = role_filter.unwrap_or("").trim().to_lowercase();
        let mut counter = 0usize;

        fn dfs(
            el: &IUIAutomationElement,
            depth: usize,
            walker: &IUIAutomationTreeWalker,
            sel_low: &str,
            role_low: &str,
            counter: &mut usize,
        ) -> Option<(IUIAutomationElement, String, String, String)> {
            if depth > 10 || *counter > 600 {
                return None;
            }
            let ctrl_id = unsafe { el.CurrentControlType().map(|c| c.0).unwrap_or(50025) };
            let role = ctrl_type_name(ctrl_id).to_string();
            let name = unsafe {
                el.CurrentName()
                    .unwrap_or_default()
                    .to_string()
                    .trim()
                    .to_string()
            };
            let offscreen = unsafe { el.CurrentIsOffscreen().unwrap_or(BOOL(0)).as_bool() };

            let is_container = matches!(
                role.as_str(),
                "pane" | "group" | "custom" | "separator" | "scrollbar" | "thumb" | "image" | "tooltip"
            );
            let is_meaningful = !name.is_empty() || (!is_container && !offscreen);

            if is_meaningful {
                *counter += 1;
                let eid = format!("@e{}", *counter);
                let role_ok = role_low.is_empty() || role == role_low;
                let sel_ok = if sel_low.is_empty() {
                    true
                } else if sel_low.starts_with("@e") {
                    eid.to_lowercase() == sel_low
                } else {
                    name.to_lowercase().contains(sel_low)
                };

                if role_ok && sel_ok && (!sel_low.is_empty() || !role_low.is_empty()) {
                    return Some((el.clone(), eid, role, name));
                }
            }

            unsafe {
                let mut child_opt = walker.GetFirstChildElement(el).ok();
                while let Some(child) = child_opt {
                    if let Some(found) = dfs(&child, depth + 1, walker, sel_low, role_low, counter) {
                        return Some(found);
                    }
                    child_opt = walker.GetNextSiblingElement(&child).ok();
                }
            }
            None
        }

        dfs(root, 0, &walker, &sel_low, &role_low, &mut counter)
    }

    // ========================================================================
    // 1. FOCUS-FREE BACKGROUND CONTROL (UIA Patterns + Win32 SendMessage/PostMessage)
    // ========================================================================
    pub fn control_window(req: ControlActionRequest) -> Result<ControlActionResponse, String> {
        let t0 = Instant::now();
        let win = resolve_target_window(&req.target)?;
        let hwnd = HWND(win.hwnd as *mut c_void);
        let action_low = req.action.trim().to_lowercase();

        // Action A: Window State Control (Minimize, Maximize, Restore, Close)
        if action_low == "window_state" {
            let state_cmd = req.value.as_deref().unwrap_or("restore").to_lowercase();
            unsafe {
                match state_cmd.as_str() {
                    "minimize" => {
                        let _ = ShowWindow(hwnd, SW_MINIMIZE);
                    }
                    "maximize" => {
                        let _ = ShowWindow(hwnd, SW_MAXIMIZE);
                    }
                    "close" => {
                        let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                    _ => {
                        let _ = ShowWindow(hwnd, SW_RESTORE);
                    }
                }
            }
            return Ok(ControlActionResponse {
                success: true,
                target_hwnd: win.hwnd,
                target_process: win.process,
                target_title: win.title,
                action: req.action,
                method_used: "Win32 ShowWindow / PostMessageW(WM_CLOSE)".to_string(),
                matched_element: None,
                details: format!("Applied window state '{}'", state_cmd),
                execution_time_ms: (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
            });
        }

        // Action B: Background Coordinate Click via PostMessageW(WM_LBUTTONDOWN/UP)
        if action_low == "click_xy" {
            let x = req.x.unwrap_or(10) as usize;
            let y = req.y.unwrap_or(10) as usize;
            let lparam = LPARAM(((y & 0xFFFF) << 16 | (x & 0xFFFF)) as isize);
            unsafe {
                let _ = PostMessageW(hwnd, WM_LBUTTONDOWN, WPARAM(1), lparam);
                let _ = PostMessageW(hwnd, WM_LBUTTONUP, WPARAM(0), lparam);
            }
            return Ok(ControlActionResponse {
                success: true,
                target_hwnd: win.hwnd,
                target_process: win.process,
                target_title: win.title,
                action: req.action,
                method_used: "Win32 PostMessageW(WM_LBUTTONDOWN + WM_LBUTTONUP)".to_string(),
                matched_element: Some(format!("client({}, {})", x, y)),
                details: format!("Dispatched background click at client coordinates ({}, {}) without moving physical cursor", x, y),
                execution_time_ms: (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
            });
        }

        // Action C: Native Win32 Menu Bar Command Trigger (WM_COMMAND)
        if action_low == "menu" {
            let query = req
                .selector
                .as_deref()
                .or(req.value.as_deref())
                .unwrap_or("")
                .to_lowercase();
            if let Some((menu_id, menu_label)) = find_menu_command_id(hwnd, &query) {
                unsafe {
                    let _ = PostMessageW(hwnd, WM_COMMAND, WPARAM(menu_id as usize), LPARAM(0));
                }
                return Ok(ControlActionResponse {
                    success: true,
                    target_hwnd: win.hwnd,
                    target_process: win.process,
                    target_title: win.title,
                    action: req.action,
                    method_used: "Win32 GetMenu + PostMessageW(WM_COMMAND)".to_string(),
                    matched_element: Some(menu_label.clone()),
                    details: format!("Triggered native menu command '{}' (ID {}) in background", menu_label, menu_id),
                    execution_time_ms: (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
                });
            } else {
                return Err(format!("No native Win32 menu item matching '{}' found on window", query));
            }
        }

        // Action D: Background Keystroke Injection via PostMessageW(WM_CHAR / WM_KEYDOWN)
        if action_low == "send_keys" {
            let payload = req.value.as_deref().unwrap_or("");
            let target_child = resolve_element_hwnd(win.hwnd, req.selector.as_deref(), req.role.as_deref())
                .unwrap_or(hwnd);
            let mut sent_count = 0usize;
            unsafe {
                for ch in payload.chars() {
                    if ch == '\n' || ch == '\r' {
                        let _ = PostMessageW(target_child, WM_KEYDOWN, WPARAM(0x0D), LPARAM(0));
                        let _ = PostMessageW(target_child, WM_KEYUP, WPARAM(0x0D), LPARAM(0));
                    } else if ch == '\t' {
                        let _ = PostMessageW(target_child, WM_KEYDOWN, WPARAM(0x09), LPARAM(0));
                        let _ = PostMessageW(target_child, WM_KEYUP, WPARAM(0x09), LPARAM(0));
                    } else {
                        let _ = PostMessageW(target_child, WM_CHAR, WPARAM(ch as usize), LPARAM(0));
                    }
                    sent_count += 1;
                }
            }
            return Ok(ControlActionResponse {
                success: true,
                target_hwnd: win.hwnd,
                target_process: win.process,
                target_title: win.title,
                action: req.action,
                method_used: "Win32 PostMessageW(WM_CHAR / WM_KEYDOWN) Background Queue".to_string(),
                matched_element: Some(format!("hwnd:{}", target_child.0 as isize)),
                details: format!("Injected {} characters directly into window message queue without focus stealing", sent_count),
                execution_time_ms: (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
            });
        }

        // Action E: UIA Pattern & Hybrid Element Actions (invoke, set_value, toggle, select, expand, collapse, scroll)
        let uia = init_uia()?;
        let root_el = unsafe {
            uia.ElementFromHandle(hwnd)
                .map_err(|e| format!("ElementFromHandle failed: {}", e))?
        };

        let (target_el, eid, el_role, el_name) = if req.selector.is_some() || req.role.is_some() {
            find_matching_uia_element(
                &uia,
                &root_el,
                req.selector.as_deref(),
                req.role.as_deref(),
            )
            .ok_or_else(|| {
                format!(
                    "No UIA element matching selector={:?} role={:?} in '{}'",
                    req.selector, req.role, win.title
                )
            })?
        } else {
            (root_el.clone(), "@root".to_string(), "window".to_string(), win.title.clone())
        };

        let matched_desc = format!("{} \"{}\" [{}]", el_role, el_name, eid);

        match action_low.as_str() {
            "invoke" | "click" => {
                unsafe {
                    // 1. Try UIA InvokePattern
                    if let Ok(unk) = target_el.GetCurrentPattern(UIA_InvokePatternId) {
                        if let Ok(inv) = unk.cast::<IUIAutomationInvokePattern>() {
                            if inv.Invoke().is_ok() {
                                return Ok(ControlActionResponse {
                                    success: true,
                                    target_hwnd: win.hwnd,
                                    target_process: win.process,
                                    target_title: win.title,
                                    action: req.action,
                                    method_used: "UIA IUIAutomationInvokePattern::Invoke()".to_string(),
                                    matched_element: Some(matched_desc),
                                    details: "Invoked element semantically in background without moving cursor".to_string(),
                                    execution_time_ms: (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
                                });
                            }
                        }
                    }

                    // 2. Try UIA TogglePattern or SelectionItemPattern
                    if let Ok(unk) = target_el.GetCurrentPattern(UIA_TogglePatternId) {
                        if let Ok(tog) = unk.cast::<IUIAutomationTogglePattern>() {
                            if tog.Toggle().is_ok() {
                                return Ok(ControlActionResponse {
                                    success: true,
                                    target_hwnd: win.hwnd,
                                    target_process: win.process,
                                    target_title: win.title,
                                    action: req.action,
                                    method_used: "UIA IUIAutomationTogglePattern::Toggle()".to_string(),
                                    matched_element: Some(matched_desc),
                                    details: "Toggled element state in background".to_string(),
                                    execution_time_ms: (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
                                });
                            }
                        }
                    }

                    if let Ok(unk) = target_el.GetCurrentPattern(UIA_SelectionItemPatternId) {
                        if let Ok(sel) = unk.cast::<IUIAutomationSelectionItemPattern>() {
                            if sel.Select().is_ok() {
                                return Ok(ControlActionResponse {
                                    success: true,
                                    target_hwnd: win.hwnd,
                                    target_process: win.process,
                                    target_title: win.title,
                                    action: req.action,
                                    method_used: "UIA IUIAutomationSelectionItemPattern::Select()".to_string(),
                                    matched_element: Some(matched_desc),
                                    details: "Selected item in background".to_string(),
                                    execution_time_ms: (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
                                });
                            }
                        }
                    }

                    // 3. Fallback: Win32 Native Handle BM_CLICK or Background Coordinate PostMessageW
                    if let Ok(el_hwnd) = target_el.CurrentNativeWindowHandle() {
                        if !el_hwnd.0.is_null() {
                            let _ = PostMessageW(el_hwnd, BM_CLICK, WPARAM(0), LPARAM(0));
                            return Ok(ControlActionResponse {
                                success: true,
                                target_hwnd: win.hwnd,
                                target_process: win.process,
                                target_title: win.title,
                                action: req.action,
                                method_used: "Win32 PostMessageW(BM_CLICK)".to_string(),
                                matched_element: Some(matched_desc),
                                details: "Sent native BM_CLICK message to control HWND".to_string(),
                                execution_time_ms: (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
                            });
                        }
                    }

                    if let Ok(rect) = target_el.CurrentBoundingRectangle() {
                        let mut win_rect = RECT::default();
                        let _ = GetWindowRect(hwnd, &mut win_rect);
                        let cx = ((rect.left + rect.right) / 2 - win_rect.left).max(1) as usize;
                        let cy = ((rect.top + rect.bottom) / 2 - win_rect.top).max(1) as usize;
                        let lparam = LPARAM(((cy & 0xFFFF) << 16 | (cx & 0xFFFF)) as isize);
                        let _ = PostMessageW(hwnd, WM_LBUTTONDOWN, WPARAM(1), lparam);
                        let _ = PostMessageW(hwnd, WM_LBUTTONUP, WPARAM(0), lparam);
                        return Ok(ControlActionResponse {
                            success: true,
                            target_hwnd: win.hwnd,
                            target_process: win.process,
                            target_title: win.title,
                            action: req.action,
                            method_used: "Win32 PostMessageW(WM_LBUTTONDOWN/UP at BoundingRect Center)".to_string(),
                            matched_element: Some(matched_desc),
                            details: format!("Posted background click to element center ({}, {})", cx, cy),
                            execution_time_ms: (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
                        });
                    }
                }
                Err("Element does not support Invoke/Toggle/Select or coordinate messaging".to_string())
            }
            "set_value" | "type" => {
                let val_str = req.value.clone().unwrap_or_default();
                unsafe {
                    // 1. Try UIA ValuePattern::SetValue
                    if let Ok(unk) = target_el.GetCurrentPattern(UIA_ValuePatternId) {
                        if let Ok(vp) = unk.cast::<IUIAutomationValuePattern>() {
                            let bstr = BSTR::from(val_str.as_str());
                            if vp.SetValue(&bstr).is_ok() {
                                return Ok(ControlActionResponse {
                                    success: true,
                                    target_hwnd: win.hwnd,
                                    target_process: win.process,
                                    target_title: win.title,
                                    action: req.action,
                                    method_used: "UIA IUIAutomationValuePattern::SetValue()".to_string(),
                                    matched_element: Some(matched_desc),
                                    details: format!("Set value ({} chars) via UIA ValuePattern", val_str.len()),
                                    execution_time_ms: (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
                                });
                            }
                        }
                    }

                    // 2. Fallback: Win32 WM_SETTEXT on element HWND or child edit control
                    let target_hwnd = target_el
                        .CurrentNativeWindowHandle()
                        .ok()
                        .filter(|h| !h.0.is_null())
                        .unwrap_or(hwnd);
                    let wide: Vec<u16> = OsString::from(&val_str)
                        .encode_wide()
                        .chain(std::iter::once(0))
                        .collect();
                    let res = SendMessageW(
                        target_hwnd,
                        WM_SETTEXT,
                        WPARAM(0),
                        LPARAM(wide.as_ptr() as isize),
                    );
                    if res.0 != 0 {
                        return Ok(ControlActionResponse {
                            success: true,
                            target_hwnd: win.hwnd,
                            target_process: win.process,
                            target_title: win.title,
                            action: req.action,
                            method_used: "Win32 SendMessageW(WM_SETTEXT)".to_string(),
                            matched_element: Some(matched_desc),
                            details: format!("Set control text ({} chars) via Win32 WM_SETTEXT", val_str.len()),
                            execution_time_ms: (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
                        });
                    }
                }
                Err("Could not set value via UIA ValuePattern or Win32 WM_SETTEXT".to_string())
            }
            "toggle" => unsafe {
                if let Ok(unk) = target_el.GetCurrentPattern(UIA_TogglePatternId) {
                    if let Ok(tog) = unk.cast::<IUIAutomationTogglePattern>() {
                        tog.Toggle().map_err(|e| format!("Toggle failed: {}", e))?;
                        return Ok(ControlActionResponse {
                            success: true,
                            target_hwnd: win.hwnd,
                            target_process: win.process,
                            target_title: win.title,
                            action: req.action,
                            method_used: "UIA IUIAutomationTogglePattern::Toggle()".to_string(),
                            matched_element: Some(matched_desc),
                            details: "Toggled control state".to_string(),
                            execution_time_ms: (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
                        });
                    }
                }
                Err("Element does not support TogglePattern".to_string())
            },
            "select" => unsafe {
                if let Ok(unk) = target_el.GetCurrentPattern(UIA_SelectionItemPatternId) {
                    if let Ok(sel) = unk.cast::<IUIAutomationSelectionItemPattern>() {
                        sel.Select().map_err(|e| format!("Select failed: {}", e))?;
                        return Ok(ControlActionResponse {
                            success: true,
                            target_hwnd: win.hwnd,
                            target_process: win.process,
                            target_title: win.title,
                            action: req.action,
                            method_used: "UIA IUIAutomationSelectionItemPattern::Select()".to_string(),
                            matched_element: Some(matched_desc),
                            details: "Selected item".to_string(),
                            execution_time_ms: (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
                        });
                    }
                }
                Err("Element does not support SelectionItemPattern".to_string())
            },
            "expand" | "collapse" => unsafe {
                if let Ok(unk) = target_el.GetCurrentPattern(UIA_ExpandCollapsePatternId) {
                    if let Ok(ec) = unk.cast::<IUIAutomationExpandCollapsePattern>() {
                        if action_low == "expand" {
                            ec.Expand().map_err(|e| format!("Expand failed: {}", e))?;
                        } else {
                            ec.Collapse().map_err(|e| format!("Collapse failed: {}", e))?;
                        }
                        return Ok(ControlActionResponse {
                            success: true,
                            target_hwnd: win.hwnd,
                            target_process: win.process,
                            target_title: win.title,
                            action: req.action,
                            method_used: "UIA IUIAutomationExpandCollapsePattern".to_string(),
                            matched_element: Some(matched_desc),
                            details: format!("Executed {} on element", action_low),
                            execution_time_ms: (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
                        });
                    }
                }
                Err("Element does not support ExpandCollapsePattern".to_string())
            },
            "scroll" => unsafe {
                let dir = req.value.as_deref().unwrap_or("down").to_lowercase();
                if let Ok(unk) = target_el.GetCurrentPattern(UIA_ScrollPatternId) {
                    if let Ok(sp) = unk.cast::<IUIAutomationScrollPattern>() {
                        let v_amt = match dir.as_str() {
                            "up" => ScrollAmount_LargeDecrement,
                            "down" => ScrollAmount_LargeIncrement,
                            _ => ScrollAmount_NoAmount,
                        };
                        let h_amt = match dir.as_str() {
                            "left" => ScrollAmount_LargeDecrement,
                            "right" => ScrollAmount_LargeIncrement,
                            _ => ScrollAmount_NoAmount,
                        };
                        sp.Scroll(h_amt, v_amt)
                            .map_err(|e| format!("Scroll failed: {}", e))?;
                        return Ok(ControlActionResponse {
                            success: true,
                            target_hwnd: win.hwnd,
                            target_process: win.process,
                            target_title: win.title,
                            action: req.action,
                            method_used: "UIA IUIAutomationScrollPattern::Scroll()".to_string(),
                            matched_element: Some(matched_desc),
                            details: format!("Scrolled '{}' in background", dir),
                            execution_time_ms: (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
                        });
                    }
                }
                Err("Element does not support ScrollPattern".to_string())
            },
            other => Err(format!("Unsupported control action '{}'", other)),
        }
    }

    fn resolve_element_hwnd(
        root_hwnd: isize,
        selector: Option<&str>,
        role: Option<&str>,
    ) -> Option<HWND> {
        let uia = init_uia().ok()?;
        let root_el = unsafe { uia.ElementFromHandle(HWND(root_hwnd as *mut c_void)).ok()? };
        if selector.is_some() || role.is_some() {
            if let Some((el, _, _, _)) = find_matching_uia_element(&uia, &root_el, selector, role) {
                if let Ok(h) = unsafe { el.CurrentNativeWindowHandle() } {
                    if !h.0.is_null() {
                        return Some(h);
                    }
                }
            }
        }
        // Also try finding first child edit/document native handle if none specified
        if let Some((el, _, _, _)) = find_matching_uia_element(&uia, &root_el, None, Some("edit"))
            .or_else(|| find_matching_uia_element(&uia, &root_el, None, Some("document")))
        {
            if let Ok(h) = unsafe { el.CurrentNativeWindowHandle() } {
                if !h.0.is_null() {
                    return Some(h);
                }
            }
        }
        None
    }

    fn enumerate_win32_menus(hwnd: HWND) -> Vec<(u32, String)> {
        let mut out = Vec::new();
        unsafe {
            let hmenu = GetMenu(hwnd);
            if hmenu.0.is_null() {
                return out;
            }
            fn walk_menu(
                menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
                prefix: &str,
                out: &mut Vec<(u32, String)>,
            ) {
                unsafe {
                    let count = GetMenuItemCount(menu);
                    if count <= 0 {
                        return;
                    }
                    for i in 0..count {
                        let mut buf = [0u16; 256];
                        let len = GetMenuStringW(menu, i as u32, Some(&mut buf), MF_BYPOSITION);
                        let label = if len > 0 {
                            OsString::from_wide(&buf[..len as usize])
                                .to_string_lossy()
                                .replace('&', "")
                                .trim()
                                .to_string()
                        } else {
                            String::new()
                        };
                        let path = if prefix.is_empty() {
                            label.clone()
                        } else if !label.is_empty() {
                            format!("{} > {}", prefix, label)
                        } else {
                            prefix.to_string()
                        };
                        let sub = GetSubMenu(menu, i);
                        if !sub.0.is_null() {
                            walk_menu(sub, &path, out);
                        } else {
                            let id = GetMenuItemID(menu, i);
                            if id != u32::MAX && id != 0 && !label.is_empty() {
                                out.push((id, path));
                            }
                        }
                    }
                }
            }
            walk_menu(hmenu, "", &mut out);
        }
        out
    }

    fn find_menu_command_id(hwnd: HWND, query_low: &str) -> Option<(u32, String)> {
        let items = enumerate_win32_menus(hwnd);
        for (id, label) in items {
            if label.to_lowercase().contains(query_low) || id.to_string() == query_low {
                return Some((id, label));
            }
        }
        None
    }

    // ========================================================================
    // 2. DWM OCCLUDED WINDOW CAPTURE (PrintWindow PW_RENDERFULLCONTENT) + NATIVE OS OCR
    // ========================================================================
    pub fn ocr_window(target_id: &str) -> Result<OcrWindowResponse, String> {
        let t0 = Instant::now();
        let win = resolve_target_window(target_id)?;
        let hwnd = HWND(win.hwnd as *mut c_void);

        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);
            let width = (rect.right - rect.left).max(1);
            let height = (rect.bottom - rect.top).max(1);

            if width <= 1 || height <= 1 {
                return Err(format!(
                    "Window '{}' has invalid dimensions ({}x{}) — restore if minimized",
                    win.title, width, height
                ));
            }

            let hdc_screen = GetDC(HWND::default());
            let hdc_mem = CreateCompatibleDC(hdc_screen);

            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = width;
            bmi.bmiHeader.biHeight = height; // Bottom-up DIB for standard BMP format
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB.0;

            let mut bits_ptr: *mut c_void = std::ptr::null_mut();
            let hbm: HBITMAP = CreateDIBSection(
                hdc_mem,
                &bmi,
                DIB_RGB_COLORS,
                &mut bits_ptr,
                None,
                0,
            )
            .map_err(|e| format!("CreateDIBSection failed: {}", e))?;

            let old_obj = SelectObject(hdc_mem, hbm);

            // PW_RENDERFULLCONTENT = 0x00000002 renders DWM DirectComposition / GPU surfaces even when occluded!
            let pw_ok = PrintWindow(hwnd, hdc_mem.0, 0x00000002).as_bool();
            let capture_method = if pw_ok {
                "DWM PrintWindow(PW_RENDERFULLCONTENT) + Windows.Media.Ocr"
            } else {
                let hdc_win = GetDC(hwnd);
                let _ = BitBlt(hdc_mem, 0, 0, width, height, hdc_win, 0, 0, SRCCOPY);
                let _ = ReleaseDC(hwnd, hdc_win);
                "GDI BitBlt + Windows.Media.Ocr"
            };

            let pixel_bytes_len = (width as usize) * (height as usize) * 4;
            let pixel_slice = std::slice::from_raw_parts(bits_ptr as *const u8, pixel_bytes_len);

            // Build in-memory 32-bpp BMP file
            let file_header_size = 14usize;
            let info_header_size = 40usize;
            let total_size = file_header_size + info_header_size + pixel_bytes_len;
            let mut bmp_data = Vec::with_capacity(total_size);

            // BITMAPFILEHEADER (14 bytes)
            bmp_data.extend_from_slice(b"BM");
            bmp_data.extend_from_slice(&(total_size as u32).to_le_bytes());
            bmp_data.extend_from_slice(&0u16.to_le_bytes());
            bmp_data.extend_from_slice(&0u16.to_le_bytes());
            bmp_data.extend_from_slice(&((file_header_size + info_header_size) as u32).to_le_bytes());

            // BITMAPINFOHEADER (40 bytes)
            bmp_data.extend_from_slice(&(info_header_size as u32).to_le_bytes());
            bmp_data.extend_from_slice(&width.to_le_bytes());
            bmp_data.extend_from_slice(&height.to_le_bytes());
            bmp_data.extend_from_slice(&1u16.to_le_bytes());
            bmp_data.extend_from_slice(&32u16.to_le_bytes());
            bmp_data.extend_from_slice(&0u32.to_le_bytes());
            bmp_data.extend_from_slice(&(pixel_bytes_len as u32).to_le_bytes());
            bmp_data.extend_from_slice(&2835i32.to_le_bytes());
            bmp_data.extend_from_slice(&2835i32.to_le_bytes());
            bmp_data.extend_from_slice(&0u32.to_le_bytes());
            bmp_data.extend_from_slice(&0u32.to_le_bytes());

            bmp_data.extend_from_slice(pixel_slice);

            // Cleanup GDI handles immediately
            let _ = SelectObject(hdc_mem, old_obj);
            let _ = DeleteObject(hbm);
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(HWND::default(), hdc_screen);

            // Pipe BMP buffer into WinRT InMemoryRandomAccessStream -> BitmapDecoder -> OcrEngine
            let stream = InMemoryRandomAccessStream::new()
                .map_err(|e| format!("InMemoryRandomAccessStream failed: {}", e))?;
            let writer = DataWriter::CreateDataWriter(&stream)
                .map_err(|e| format!("CreateDataWriter failed: {}", e))?;
            writer
                .WriteBytes(&bmp_data)
                .map_err(|e| format!("WriteBytes failed: {}", e))?;
            writer
                .StoreAsync()
                .map_err(|e| format!("StoreAsync start failed: {}", e))?
                .get()
                .map_err(|e| format!("StoreAsync complete failed: {}", e))?;
            writer
                .FlushAsync()
                .map_err(|e| format!("FlushAsync start failed: {}", e))?
                .get()
                .map_err(|e| format!("FlushAsync complete failed: {}", e))?;
            let _ = writer.DetachStream();
            stream
                .Seek(0)
                .map_err(|e| format!("Stream Seek(0) failed: {}", e))?;

            let decoder = BitmapDecoder::CreateAsync(&stream)
                .map_err(|e| format!("BitmapDecoder::CreateAsync start failed: {}", e))?
                .get()
                .map_err(|e| format!("BitmapDecoder::CreateAsync get failed: {}", e))?;

            let soft_bmp = decoder
                .GetSoftwareBitmapAsync()
                .map_err(|e| format!("GetSoftwareBitmapAsync start failed: {}", e))?
                .get()
                .map_err(|e| format!("GetSoftwareBitmapAsync get failed: {}", e))?;

            let ocr_engine = OcrEngine::TryCreateFromUserProfileLanguages()
                .map_err(|e| format!("OcrEngine::TryCreateFromUserProfileLanguages failed: {}", e))?;

            let ocr_result = ocr_engine
                .RecognizeAsync(&soft_bmp)
                .map_err(|e| format!("RecognizeAsync start failed: {}", e))?
                .get()
                .map_err(|e| format!("RecognizeAsync get failed: {}", e))?;

            let ocr_lines = ocr_result
                .Lines()
                .map_err(|e| format!("OcrResult::Lines failed: {}", e))?;

            let line_count = ocr_lines.Size().unwrap_or(0) as usize;
            let mut lines_out = Vec::with_capacity(line_count);
            let mut full_text_lines = Vec::with_capacity(line_count);
            let mut total_words = 0usize;

            for i in 0..line_count {
                if let Ok(line) = ocr_lines.GetAt(i as u32) {
                    let line_text = line.Text().unwrap_or_default().to_string();
                    let mut words_out = Vec::new();
                    let mut min_x = i32::MAX;
                    let mut min_y = i32::MAX;
                    let mut max_r = i32::MIN;
                    let mut max_b = i32::MIN;

                    if let Ok(words) = line.Words() {
                        let w_count = words.Size().unwrap_or(0);
                        for j in 0..w_count {
                            if let Ok(w) = words.GetAt(j) {
                                let wt = w.Text().unwrap_or_default().to_string();
                                let br = w.BoundingRect().unwrap_or_default();
                                let wx = br.X.round() as i32;
                                let wy = br.Y.round() as i32;
                                let ww = br.Width.round() as i32;
                                let wh = br.Height.round() as i32;
                                min_x = min_x.min(wx);
                                min_y = min_y.min(wy);
                                max_r = max_r.max(wx + ww);
                                max_b = max_b.max(wy + wh);
                                total_words += 1;
                                words_out.push(OcrWordBox {
                                    text: wt,
                                    left: wx,
                                    top: wy,
                                    width: ww,
                                    height: wh,
                                });
                            }
                        }
                    }

                    if min_x == i32::MAX {
                        min_x = 0;
                        min_y = 0;
                        max_r = 0;
                        max_b = 0;
                    }

                    full_text_lines.push(line_text.clone());
                    lines_out.push(OcrLineBox {
                        line_index: i + 1,
                        text: line_text,
                        left: min_x,
                        top: min_y,
                        width: (max_r - min_x).max(0),
                        height: (max_b - min_y).max(0),
                        words: words_out,
                    });
                }
            }

            let full_text = full_text_lines.join("\n");
            let char_count = full_text.chars().count();
            let elapsed_ms = (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0;

            Ok(OcrWindowResponse {
                hwnd: win.hwnd,
                pid: win.pid,
                process: win.process,
                title: win.title,
                capture_method: capture_method.to_string(),
                window_width: width,
                window_height: height,
                line_count: lines_out.len(),
                word_count: total_words,
                char_count,
                scan_time_ms: elapsed_ms,
                full_text,
                lines: lines_out,
            })
        }
    }

    // ========================================================================
    // 3. WAIT CHAIN TRAVERSAL (WCT) + PROCESS & KERNEL TELEMETRY SIDECHANNEL
    // ========================================================================
    pub fn inspect_telemetry(target_id: &str) -> Result<ProcessTelemetryResponse, String> {
        let t0 = Instant::now();
        let win = resolve_target_window(target_id)?;
        let hwnd = HWND(win.hwnd as *mut c_void);

        unsafe {
            let mut pid = win.pid;
            let ui_tid = GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let is_hung = IsHungAppWindow(hwnd).as_bool();

            // 1. Wait Chain Traversal (WCT) on the UI thread
            let mut wait_chain = Vec::new();
            let mut is_cycle = 0i32;
            let wct_session = OpenThreadWaitChainSession(0, std::ptr::null());
            if !wct_session.is_null() && ui_tid != 0 {
                let mut nodes = [WAITCHAIN_NODE_INFO {
                    object_type: 0,
                    object_status: 0,
                    union_data: [0u8; 280],
                }; 16];
                let mut count = 16u32;
                // WCT_GETINFO_ALL_FLAGS = 7
                let ok = GetThreadWaitChain(
                    wct_session,
                    0,
                    7,
                    ui_tid,
                    &mut count,
                    nodes.as_mut_ptr(),
                    &mut is_cycle,
                )
                .as_bool();

                if ok {
                    for i in 0..(count as usize).min(16) {
                        let n = &nodes[i];
                        let obj_type = match n.object_type {
                            1 => "CriticalSection",
                            2 => "SendMessage",
                            3 => "Mutex",
                            4 => "Alpc",
                            5 => "Com",
                            6 => "ThreadWait",
                            7 => "ProcessWait",
                            8 => "Thread",
                            9 => "ComActivation",
                            10 => "Unknown",
                            11 => "SocketIo",
                            12 => "SmbIo",
                            _ => "SyncObject",
                        };
                        let obj_status = match n.object_status {
                            1 => "NoAccess",
                            2 => "Running",
                            3 => "Blocked",
                            4 => "PidOnly",
                            5 => "PidOnlyRpcss",
                            6 => "Owned",
                            7 => "NotOwned",
                            8 => "Abandoned",
                            _ => "Unknown",
                        };

                        if n.object_type == 8 {
                            // Thread node: union_data has ProcessId (u32), ThreadId (u32), WaitTime (u32), ContextSwitches (u32)
                            let p_id = u32::from_ne_bytes(n.union_data[0..4].try_into().unwrap_or([0; 4]));
                            let t_id = u32::from_ne_bytes(n.union_data[4..8].try_into().unwrap_or([0; 4]));
                            let w_time = u32::from_ne_bytes(n.union_data[8..12].try_into().unwrap_or([0; 4]));
                            let ctx_sw = u32::from_ne_bytes(n.union_data[12..16].try_into().unwrap_or([0; 4]));
                            wait_chain.push(WaitChainNode {
                                object_type: obj_type.to_string(),
                                object_status: obj_status.to_string(),
                                object_name: format!("Thread {} (PID {})", t_id, p_id),
                                process_id: p_id,
                                thread_id: t_id,
                                wait_time: w_time,
                                context_switches: ctx_sw,
                            });
                        } else {
                            // LockObject: union_data starts with ObjectName (WCHAR[128] = 256 bytes)
                            let wide_slice = std::slice::from_raw_parts(
                                n.union_data.as_ptr() as *const u16,
                                128,
                            );
                            let nul_pos = wide_slice.iter().position(|&c| c == 0).unwrap_or(128);
                            let name = OsString::from_wide(&wide_slice[..nul_pos])
                                .to_string_lossy()
                                .into_owned();
                            wait_chain.push(WaitChainNode {
                                object_type: obj_type.to_string(),
                                object_status: obj_status.to_string(),
                                object_name: if name.is_empty() {
                                    format!("<anonymous {}>", obj_type)
                                } else {
                                    name
                                },
                                process_id: 0,
                                thread_id: 0,
                                wait_time: 0,
                                context_switches: 0,
                            });
                        }
                    }
                }
                CloseThreadWaitChainSession(wct_session);
            }

            let wait_chain_status = if is_cycle != 0 {
                "DEADLOCKED".to_string()
            } else if is_hung {
                "HUNG_MESSAGE_LOOP".to_string()
            } else if wait_chain.iter().any(|n| n.object_status == "Blocked") {
                "BLOCKED_ON_WAIT_CHAIN".to_string()
            } else {
                "HEALTHY_RESPONSIVE".to_string()
            };

            // 2. Process Memory, Handle, GUI Resource & I/O Counters
            let mut working_set_mb = 0.0;
            let mut private_bytes_mb = 0.0;
            let mut handle_count = 0u32;
            let mut gdi_objects = 0u32;
            let mut user_objects = 0u32;
            let mut io_read_ops = 0u64;
            let mut io_write_ops = 0u64;
            let mut io_read_bytes = 0u64;
            let mut io_write_bytes = 0u64;

            if let Ok(h_proc) = OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
                false,
                pid,
            )
            .or_else(|_| OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid))
            {
                let mut mem = PROCESS_MEMORY_COUNTERS_EX {
                    cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
                    ..Default::default()
                };
                if K32GetProcessMemoryInfo(h_proc, &mut mem, mem.cb).as_bool() {
                    working_set_mb =
                        ((mem.working_set_size as f64 / (1024.0 * 1024.0)) * 10.0).round() / 10.0;
                    private_bytes_mb =
                        ((mem.private_usage as f64 / (1024.0 * 1024.0)) * 10.0).round() / 10.0;
                }
                let _ = GetProcessHandleCount(h_proc, &mut handle_count);
                gdi_objects = GetGuiResources(h_proc, 0);
                user_objects = GetGuiResources(h_proc, 1);

                let mut io_ctrs = IO_COUNTERS::default();
                if GetProcessIoCounters(h_proc, &mut io_ctrs).is_ok() {
                    io_read_ops = io_ctrs.ReadOperationCount;
                    io_write_ops = io_ctrs.WriteOperationCount;
                    io_read_bytes = io_ctrs.ReadTransferCount;
                    io_write_bytes = io_ctrs.WriteTransferCount;
                }
                let _ = CloseHandle(h_proc);
            }

            // 3. Child Process Tree (via ToolHelp32 Snapshot)
            let child_processes = collect_child_processes(pid);
            let mut tracked_pids: HashSet<u32> = HashSet::new();
            tracked_pids.insert(pid);
            for cp in &child_processes {
                tracked_pids.insert(cp.pid);
            }

            // 4. Active TCP Endpoints for Target Process & Children
            let all_tcp = list_system_tcp_endpoints();
            let tcp_connections: Vec<TcpEndpointInfo> = all_tcp
                .into_iter()
                .filter(|t| tracked_pids.contains(&t.pid))
                .collect();

            // 5. Native Win32 Menu Bar Items
            let menu_items: Vec<String> = enumerate_win32_menus(hwnd)
                .into_iter()
                .map(|(id, path)| format!("[id:{}] {}", id, path))
                .collect();

            let elapsed_ms = (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0;

            Ok(ProcessTelemetryResponse {
                hwnd: win.hwnd,
                pid,
                ui_thread_id: ui_tid,
                process: win.process,
                title: win.title,
                is_hung_window: is_hung,
                wait_chain_status,
                is_deadlocked: is_cycle != 0,
                wait_chain,
                working_set_mb,
                private_bytes_mb,
                handle_count,
                gdi_objects,
                user_objects,
                io_read_ops,
                io_write_ops,
                io_read_bytes,
                io_write_bytes,
                child_processes,
                tcp_connections,
                menu_items,
                scan_time_ms: elapsed_ms,
            })
        }
    }

    fn collect_child_processes(root_pid: u32) -> Vec<ChildProcessNode> {
        let mut all_procs = Vec::new();
        unsafe {
            if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
                let mut entry = PROCESSENTRY32W {
                    dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                    ..Default::default()
                };
                if Process32FirstW(snap, &mut entry).is_ok() {
                    loop {
                        let nul = entry
                            .szExeFile
                            .iter()
                            .position(|&c| c == 0)
                            .unwrap_or(entry.szExeFile.len());
                        let name = OsString::from_wide(&entry.szExeFile[..nul])
                            .to_string_lossy()
                            .into_owned();
                        all_procs.push(ChildProcessNode {
                            pid: entry.th32ProcessID,
                            parent_pid: entry.th32ParentProcessID,
                            name,
                            threads: entry.cntThreads,
                        });
                        if Process32NextW(snap, &mut entry).is_err() {
                            break;
                        }
                    }
                }
                let _ = CloseHandle(snap);
            }
        }

        // BFS to find all descendants of root_pid
        let mut descendants = Vec::new();
        let mut queue = vec![root_pid];
        let mut visited = HashSet::new();
        visited.insert(root_pid);

        while let Some(curr) = queue.pop() {
            for p in &all_procs {
                if p.parent_pid == curr && !visited.contains(&p.pid) {
                    visited.insert(p.pid);
                    descendants.push(p.clone());
                    queue.push(p.pid);
                }
            }
        }
        descendants
    }

    fn list_system_tcp_endpoints() -> Vec<TcpEndpointInfo> {
        let mut results = Vec::new();
        unsafe {
            let mut size = 0u32;
            // AF_INET = 2, TCP_TABLE_OWNER_PID_ALL = 5
            let _ = GetExtendedTcpTable(std::ptr::null_mut(), &mut size, 1, 2, 5, 0);
            if size == 0 {
                return results;
            }
            let mut buf = vec![0u8; size as usize];
            if GetExtendedTcpTable(buf.as_mut_ptr(), &mut size, 1, 2, 5, 0) == 0 && buf.len() >= 4
            {
                let num_entries = u32::from_ne_bytes(buf[0..4].try_into().unwrap_or([0; 4])) as usize;
                let mut offset = 4usize;
                for _ in 0..num_entries {
                    if offset + 24 > buf.len() {
                        break;
                    }
                    let state_code = u32::from_ne_bytes(buf[offset..offset + 4].try_into().unwrap());
                    let local_ip = &buf[offset + 4..offset + 8];
                    let local_port = u16::from_be_bytes([buf[offset + 8], buf[offset + 9]]);
                    let remote_ip = &buf[offset + 12..offset + 16];
                    let remote_port = u16::from_be_bytes([buf[offset + 16], buf[offset + 17]]);
                    let owner_pid = u32::from_ne_bytes(buf[offset + 20..offset + 24].try_into().unwrap());
                    offset += 24;

                    let state = match state_code {
                        1 => "CLOSED",
                        2 => "LISTEN",
                        3 => "SYN_SENT",
                        4 => "SYN_RCVD",
                        5 => "ESTABLISHED",
                        6 => "FIN_WAIT1",
                        7 => "FIN_WAIT2",
                        8 => "CLOSE_WAIT",
                        9 => "CLOSING",
                        10 => "LAST_ACK",
                        11 => "TIME_WAIT",
                        12 => "DELETE_TCB",
                        _ => "UNKNOWN",
                    };

                    results.push(TcpEndpointInfo {
                        pid: owner_pid,
                        process: get_process_name(owner_pid),
                        local_addr: format!(
                            "{}.{}.{}.{}",
                            local_ip[0], local_ip[1], local_ip[2], local_ip[3]
                        ),
                        local_port,
                        remote_addr: format!(
                            "{}.{}.{}.{}",
                            remote_ip[0], remote_ip[1], remote_ip[2], remote_ip[3]
                        ),
                        remote_port,
                        state: state.to_string(),
                    });
                }
            }
        }
        results
    }

    // ========================================================================
    // 4. RUNTIME & FRAMEWORK DEBUG PROTOCOL DISCOVERY (CDP, .NET Pipes, COM ROT)
    // ========================================================================
    pub fn discover_debug_channels() -> DebugChannelsResponse {
        let t0 = Instant::now();

        // A. Discover Chrome DevTools Protocol (CDP) ports on loopback
        let tcp_list = list_system_tcp_endpoints();
        let mut candidate_ports: Vec<(u16, u32, String)> = Vec::new();
        let mut seen_ports = HashSet::new();

        for ep in &tcp_list {
            if ep.state == "LISTEN" && (ep.local_addr == "127.0.0.1" || ep.local_addr == "0.0.0.0")
            {
                let p_low = ep.process.to_lowercase();
                let is_cdp_candidate = (9220..=9250).contains(&ep.local_port)
                    || p_low.contains("chrome")
                    || p_low.contains("msedge")
                    || p_low.contains("webview")
                    || p_low.contains("electron")
                    || p_low.contains("code")
                    || p_low.contains("cef")
                    || p_low.contains("axiowl")
                    || p_low.contains("node");
                if is_cdp_candidate && ep.local_port != 8765 && seen_ports.insert(ep.local_port) {
                    candidate_ports.push((ep.local_port, ep.pid, ep.process.clone()));
                }
            }
        }

        let mut cdp_targets = Vec::new();
        for (port, pid, proc_name) in candidate_ports {
            if let Some(targets) = probe_cdp_port(port, pid, &proc_name) {
                cdp_targets.extend(targets);
            }
        }

        // B. Discover .NET Core / .NET 5+ Diagnostics Named Pipes (\\.\pipe\dotnet-diagnostic-*)
        let mut dotnet_diagnostic_pipes = Vec::new();
        unsafe {
            let pattern: Vec<u16> = "\\\\.\\pipe\\dotnet-diagnostic-*\0"
                .encode_utf16()
                .collect();
            let mut find_data = WIN32_FIND_DATAW::default();
            if let Ok(h_find) = FindFirstFileW(PCWSTR(pattern.as_ptr()), &mut find_data) {
                loop {
                    let nul = find_data
                        .cFileName
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(find_data.cFileName.len());
                    let pipe_name = OsString::from_wide(&find_data.cFileName[..nul])
                        .to_string_lossy()
                        .into_owned();
                    let extracted_pid = pipe_name
                        .strip_prefix("dotnet-diagnostic-")
                        .and_then(|s| s.split('-').next())
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(0);
                    dotnet_diagnostic_pipes.push(DotNetPipeInfo {
                        pid: extracted_pid,
                        process: get_process_name(extracted_pid),
                        pipe_path: format!(r"\\.\pipe\{}", pipe_name),
                    });
                    if FindNextFileW(h_find, &mut find_data).is_err() {
                        break;
                    }
                }
                let _ = FindClose(h_find);
            }
        }

        // C. Inspect COM Running Object Table (ROT)
        let mut com_running_objects = Vec::new();
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            if let Ok(rot) = GetRunningObjectTable(0) {
                if let Ok(enum_moniker) = rot.EnumRunning() {
                    if let Ok(bind_ctx) = CreateBindCtx(0) {
                        let mut monikers: [Option<IMoniker>; 1] = [None];
                        let mut fetched = 0u32;
                        while enum_moniker.Next(&mut monikers, Some(&mut fetched)).is_ok()
                            && fetched == 1
                        {
                            if let Some(moniker) = monikers[0].take() {
                                if let Ok(disp_pwstr) = moniker.GetDisplayName(&bind_ctx, None) {
                                    let disp = disp_pwstr.to_string().unwrap_or_default();
                                    if !disp.is_empty() {
                                        let cat = if disp.contains("VisualStudio") {
                                            "VisualStudio DTE"
                                        } else if disp.ends_with(".xlsx")
                                            || disp.ends_with(".docx")
                                            || disp.ends_with(".pptx")
                                        {
                                            "Microsoft Office Document"
                                        } else if disp.starts_with('!') {
                                            "ActiveX / COM Service Moniker"
                                        } else {
                                            "COM Moniker"
                                        };
                                        com_running_objects.push(RotObjectInfo {
                                            display_name: disp,
                                            category: cat.to_string(),
                                        });
                                    }
                                }
                            }
                            if com_running_objects.len() >= 50 {
                                break;
                            }
                        }
                    }
                }
            }
        }

        let elapsed_ms = (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0;
        DebugChannelsResponse {
            scan_time_ms: elapsed_ms,
            cdp_targets,
            dotnet_diagnostic_pipes,
            com_running_objects,
        }
    }

    fn probe_cdp_port(port: u16, pid: u32, proc_name: &str) -> Option<Vec<CdpTargetInfo>> {
        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().ok()?;
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(45)).ok()?;
        let _ = stream.set_read_timeout(Some(Duration::from_millis(80)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(45)));

        let req = format!(
            "GET /json/list HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            port
        );
        stream.write_all(req.as_bytes()).ok()?;

        let mut resp = String::new();
        let _ = stream.read_to_string(&mut resp);
        let body = resp.split("\r\n\r\n").nth(1)?;
        let arr: Vec<serde_json::Value> = serde_json::from_str(body.trim()).ok()?;

        let mut out = Vec::new();
        for item in arr {
            out.push(CdpTargetInfo {
                port,
                pid,
                process: proc_name.to_string(),
                target_id: item["id"].as_str().unwrap_or("").to_string(),
                target_type: item["type"].as_str().unwrap_or("page").to_string(),
                title: item["title"].as_str().unwrap_or("").to_string(),
                url: item["url"].as_str().unwrap_or("").to_string(),
                websocket_debugger_url: item["webSocketDebuggerUrl"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
            });
        }
        Some(out)
    }

    // ========================================================================
    // 5. CONSOLE & TERMINAL SCREEN BUFFER INTERCEPTION (AttachConsole + CONOUT$)
    // ========================================================================
    pub fn read_console_buffer(target: &str) -> Result<ConsoleBufferResponse, String> {
        let t0 = Instant::now();
        let pid = resolve_console_pid(target)?;
        let proc_name = get_process_name(pid);

        // Run AttachConsole on a dedicated OS thread so we don't disturb main process stdio
        let handle = std::thread::spawn(move || -> Result<ConsoleBufferResponse, String> {
            unsafe {
                let _ = FreeConsole();
                if let Err(e) = AttachConsole(pid) {
                    let _ = AttachConsole(u32::MAX);
                    return Err(format!("AttachConsole(pid={}) failed: {}", pid, e));
                }

                let conout_wide: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
                let h_out = CreateFileW(
                    PCWSTR(conout_wide.as_ptr()),
                    FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None,
                    OPEN_EXISTING,
                    Default::default(),
                    None,
                )
                .map_err(|e| {
                    let _ = FreeConsole();
                    let _ = AttachConsole(u32::MAX);
                    format!("CreateFileW(CONOUT$) failed: {}", e)
                })?;

                let mut csbi = CONSOLE_SCREEN_BUFFER_INFO::default();
                if let Err(e) = GetConsoleScreenBufferInfo(h_out, &mut csbi) {
                    let _ = CloseHandle(h_out);
                    let _ = FreeConsole();
                    let _ = AttachConsole(u32::MAX);
                    return Err(format!("GetConsoleScreenBufferInfo failed: {}", e));
                }

                let width = csbi.dwSize.X.max(1);
                let height = csbi.dwSize.Y.max(1);
                let read_rows = height.min(300); // Read up to 300 rows
                let start_row = (csbi.dwCursorPosition.Y - read_rows + 1).max(0);

                let mut lines = Vec::new();
                for row in start_row..(start_row + read_rows) {
                    let mut row_buf = vec![0u16; width as usize];
                    let mut chars_read = 0u32;
                    let coord = COORD { X: 0, Y: row };
                    if ReadConsoleOutputCharacterW(h_out, &mut row_buf, coord, &mut chars_read)
                        .is_ok()
                        && chars_read > 0
                    {
                        let s = OsString::from_wide(&row_buf[..chars_read as usize])
                            .to_string_lossy()
                            .trim_end()
                            .to_string();
                        lines.push(s);
                    }
                }

                let _ = CloseHandle(h_out);
                let _ = FreeConsole();
                let _ = AttachConsole(u32::MAX); // Re-attach to parent console (ATTACH_PARENT_PROCESS)

                // Trim trailing blank lines
                while lines.last().map_or(false, |l| l.is_empty()) {
                    lines.pop();
                }

                let non_empty = lines.iter().filter(|l| !l.trim().is_empty()).count();
                let visible_lines =
                    (csbi.srWindow.Bottom - csbi.srWindow.Top + 1).max(0) as usize;
                let text = lines.join("\n");

                Ok(ConsoleBufferResponse {
                    pid,
                    process: proc_name,
                    buffer_width: width,
                    buffer_height: height,
                    cursor_x: csbi.dwCursorPosition.X,
                    cursor_y: csbi.dwCursorPosition.Y,
                    visible_lines,
                    total_non_empty_lines: non_empty,
                    text,
                    scan_time_ms: (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
                })
            }
        });

        handle
            .join()
            .map_err(|_| "Console reader thread panicked".to_string())?
    }

    pub fn write_console_input(
        target: &str,
        text: &str,
        send_enter: bool,
    ) -> Result<serde_json::Value, String> {
        let t0 = Instant::now();
        let pid = resolve_console_pid(target)?;
        let proc_name = get_process_name(pid);
        let text_owned = text.to_string();

        let handle = std::thread::spawn(move || -> Result<usize, String> {
            unsafe {
                let _ = FreeConsole();
                if let Err(e) = AttachConsole(pid) {
                    let _ = AttachConsole(u32::MAX);
                    return Err(format!("AttachConsole(pid={}) failed: {}", pid, e));
                }

                let conin_wide: Vec<u16> = "CONIN$\0".encode_utf16().collect();
                let h_in = CreateFileW(
                    PCWSTR(conin_wide.as_ptr()),
                    FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None,
                    OPEN_EXISTING,
                    Default::default(),
                    None,
                )
                .map_err(|e| {
                    let _ = FreeConsole();
                    let _ = AttachConsole(u32::MAX);
                    format!("CreateFileW(CONIN$) failed: {}", e)
                })?;

                let mut records = Vec::new();
                for ch in text_owned.encode_utf16() {
                    for &down in &[BOOL(1), BOOL(0)] {
                        let mut rec = INPUT_RECORD::default();
                        rec.EventType = KEY_EVENT as u16;
                        rec.Event.KeyEvent = KEY_EVENT_RECORD {
                            bKeyDown: down,
                            wRepeatCount: 1,
                            wVirtualKeyCode: 0,
                            wVirtualScanCode: 0,
                            uChar: KEY_EVENT_RECORD_0 { UnicodeChar: ch },
                            dwControlKeyState: 0,
                        };
                        records.push(rec);
                    }
                }

                if send_enter {
                    for &down in &[BOOL(1), BOOL(0)] {
                        let mut rec = INPUT_RECORD::default();
                        rec.EventType = KEY_EVENT as u16;
                        rec.Event.KeyEvent = KEY_EVENT_RECORD {
                            bKeyDown: down,
                            wRepeatCount: 1,
                            wVirtualKeyCode: 0x0D, // VK_RETURN
                            wVirtualScanCode: 0x1C,
                            uChar: KEY_EVENT_RECORD_0 {
                                UnicodeChar: '\r' as u16,
                            },
                            dwControlKeyState: 0,
                        };
                        records.push(rec);
                    }
                }

                let mut written = 0u32;
                let res = WriteConsoleInputW(h_in, &records, &mut written);
                let _ = CloseHandle(h_in);
                let _ = FreeConsole();
                let _ = AttachConsole(u32::MAX);
                res.map_err(|e| format!("WriteConsoleInputW failed: {}", e))?;
                Ok(written as usize)
            }
        });

        let events_written = handle
            .join()
            .map_err(|_| "Console writer thread panicked".to_string())??;

        Ok(serde_json::json!({
            "success": true,
            "pid": pid,
            "process": proc_name,
            "events_written": events_written,
            "execution_time_ms": (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0
        }))
    }

    fn resolve_console_pid(target: &str) -> Result<u32, String> {
        if let Ok(pid) = target.trim().parse::<u32>() {
            return Ok(pid);
        }
        if let Ok(w) = resolve_target_window(target) {
            let children = collect_child_processes(w.pid);
            for child in &children {
                let c_low = child.name.to_lowercase();
                if matches!(
                    c_low.as_str(),
                    "powershell.exe"
                        | "pwsh.exe"
                        | "cmd.exe"
                        | "wsl.exe"
                        | "bash.exe"
                        | "python.exe"
                ) {
                    return Ok(child.pid);
                }
            }
            return Ok(w.pid);
        }
        Err(format!("Could not resolve console PID for '{}'", target))
    }

    // ========================================================================
    // 6. ISOLATED HIDDEN SANDBOX DESKTOPS (CreateDesktopW / EnumDesktopsW)
    // ========================================================================
    unsafe extern "system" fn enum_desktop_proc(lpszdesktop: PCWSTR, lparam: LPARAM) -> BOOL {
        let vec_ptr = lparam.0 as *mut Vec<String>;
        if let Ok(name) = lpszdesktop.to_string() {
            (*vec_ptr).push(name);
        }
        BOOL(1)
    }

    unsafe extern "system" fn enum_sandbox_wnd_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let vec_ptr = lparam.0 as *mut Vec<String>;
        let len = GetWindowTextLengthW(hwnd);
        if len > 0 {
            let mut buf = vec![0u16; (len + 1) as usize];
            let read = GetWindowTextW(hwnd, &mut buf);
            let title = OsString::from_wide(&buf[..read as usize])
                .to_string_lossy()
                .into_owned();
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            (*vec_ptr).push(format!(
                "hwnd:{} pid:{} ({}) \"{}\"",
                hwnd.0 as isize,
                pid,
                get_process_name(pid),
                title
            ));
        }
        BOOL(1)
    }

    pub fn manage_sandbox_desktop(
        action: &str,
        desktop_name: &str,
        spawn_cmd: Option<&str>,
    ) -> Result<SandboxDesktopResponse, String> {
        let desk_name = if desktop_name.trim().is_empty() {
            "AxiAccessSandbox"
        } else {
            desktop_name.trim()
        };

        unsafe {
            let mut available_desktops: Vec<String> = Vec::new();
            if let Ok(hwinsta) = GetProcessWindowStation() {
                let lparam = LPARAM(&mut available_desktops as *mut Vec<String> as isize);
                let _ = EnumDesktopsW(hwinsta, Some(enum_desktop_proc), lparam);
            }

            let action_low = action.trim().to_lowercase();
            if action_low == "list" {
                let mut sandbox_windows = Vec::new();
                let wide: Vec<u16> = format!("{}\0", desk_name).encode_utf16().collect();
                if let Ok(hdesk) = OpenDesktopW(
                    PCWSTR(wide.as_ptr()),
                    DESKTOP_CONTROL_FLAGS(0),
                    false,
                    0x01FF,
                ) {
                    let lparam = LPARAM(&mut sandbox_windows as *mut Vec<String> as isize);
                    let _ = EnumDesktopWindows(hdesk, Some(enum_sandbox_wnd_proc), lparam);
                }
                return Ok(SandboxDesktopResponse {
                    action: "list".to_string(),
                    desktop_name: desk_name.to_string(),
                    available_desktops,
                    spawned_pid: None,
                    sandbox_windows,
                    status: "Enumerated window station desktops".to_string(),
                });
            }

            // Create or open the isolated kernel desktop object
            let wide_desk: Vec<u16> = format!("{}\0", desk_name).encode_utf16().collect();
            let hdesk = CreateDesktopW(
                PCWSTR(wide_desk.as_ptr()),
                PCWSTR::null(),
                None,
                DESKTOP_CONTROL_FLAGS(0),
                0x01FF, // DESKTOP_ALL_ACCESS
                None,
            )
            .map_err(|e| format!("CreateDesktopW('{}') failed: {}", desk_name, e))?;

            let mut spawned_pid = None;
            if let Some(cmd_str) = spawn_cmd.filter(|s| !s.trim().is_empty()) {
                let mut cmd_wide: Vec<u16> = OsString::from(cmd_str)
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();
                let mut desk_mutable: Vec<u16> = wide_desk.clone();
                let mut si = STARTUPINFOW::default();
                si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
                si.lpDesktop = PWSTR(desk_mutable.as_mut_ptr());
                let mut pi = PROCESS_INFORMATION::default();

                CreateProcessW(
                    PCWSTR::null(),
                    PWSTR(cmd_wide.as_mut_ptr()),
                    None,
                    None,
                    false,
                    Default::default(),
                    None,
                    PCWSTR::null(),
                    &si,
                    &mut pi,
                )
                .map_err(|e| format!("CreateProcessW on sandbox desktop failed: {}", e))?;

                spawned_pid = Some(pi.dwProcessId);
                let _ = CloseHandle(pi.hThread);
                let _ = CloseHandle(pi.hProcess);
                std::thread::sleep(Duration::from_millis(350));
            }

            let mut sandbox_windows = Vec::new();
            let lparam = LPARAM(&mut sandbox_windows as *mut Vec<String> as isize);
            let _ = EnumDesktopWindows(hdesk, Some(enum_sandbox_wnd_proc), lparam);

            if !available_desktops.iter().any(|d| d == desk_name) {
                available_desktops.push(desk_name.to_string());
            }

            Ok(SandboxDesktopResponse {
                action: action_low,
                desktop_name: desk_name.to_string(),
                available_desktops,
                spawned_pid,
                sandbox_windows,
                status: format!(
                    "Isolated kernel desktop '{}' active (off-screen, zero focus interference)",
                    desk_name
                ),
            })
        }
    }
}
