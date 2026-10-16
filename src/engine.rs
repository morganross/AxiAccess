use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowBounds {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub hwnd: isize,
    pub pid: u32,
    pub process: String,
    pub title: String,
    pub class_name: String,
    pub offscreen: bool,
    pub minimized: bool,
    pub focused: bool,
    pub bounds: WindowBounds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowListResponse {
    pub platform: String,
    pub screen_reader_flag: bool,
    pub scan_time_ms: f64,
    pub count: usize,
    pub windows: Vec<WindowInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBufferInfo {
    pub id: String,
    pub role: String,
    pub name: String,
    pub source: String,
    pub total_chars: usize,
    pub visible_chars: usize,
    pub offscreen_chars: usize,
    pub total_lines: usize,
    pub visible_lines: usize,
    pub preview: String,
    pub full_text: String,
    pub visible_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub id: String,
    pub role: String,
    pub name: String,
    pub depth: usize,
    pub offscreen: bool,
    pub focused: bool,
    pub enabled: bool,
    pub has_text: bool,
    pub text_source: Option<String>,
    pub total_chars: usize,
    pub offscreen_chars: usize,
    pub preview: String,
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectStats {
    pub raw_nodes: usize,
    pub semantic_nodes: usize,
    pub compression_pct: f64,
    pub text_buffers_found: usize,
    pub total_extracted_chars: usize,
    pub offscreen_extracted_chars: usize,
    pub scan_time_ms: f64,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectWindowMeta {
    pub hwnd: isize,
    pub pid: u32,
    pub process: String,
    pub title: String,
    pub class_name: String,
    pub offscreen: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectResponse {
    pub window: InspectWindowMeta,
    pub stats: InspectStats,
    pub llm_yaml: String,
    pub text_buffers: Vec<TextBufferInfo>,
    pub tree: Vec<TreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmFormatResponse {
    pub verbosity: String,
    pub target: String,
    pub hwnd: Option<isize>,
    pub process: Option<String>,
    pub char_count: usize,
    pub estimated_tokens: usize,
    pub stats: Option<InspectStats>,
    pub llm_text: String,
}

pub fn set_screen_reader_flag(enabled: bool) -> bool {
    #[cfg(windows)]
    {
        return win_impl::set_screen_reader_flag(enabled);
    }
    #[cfg(target_os = "macos")]
    {
        return mac_impl::set_screen_reader_flag(enabled);
    }
    #[cfg(target_os = "linux")]
    {
        return linux_impl::set_screen_reader_flag(enabled);
    }
}

pub fn get_screen_reader_flag() -> bool {
    #[cfg(windows)]
    {
        return win_impl::get_screen_reader_flag();
    }
    #[cfg(target_os = "macos")]
    {
        return mac_impl::get_screen_reader_flag();
    }
    #[cfg(target_os = "linux")]
    {
        return linux_impl::get_screen_reader_flag();
    }
}

pub fn list_windows() -> WindowListResponse {
    #[cfg(windows)]
    {
        return win_impl::list_windows();
    }
    #[cfg(target_os = "macos")]
    {
        return mac_impl::list_windows();
    }
    #[cfg(target_os = "linux")]
    {
        return linux_impl::list_windows();
    }
}

pub fn inspect_window(
    target_id: &str,
    max_depth: usize,
    max_nodes: usize,
) -> Result<InspectResponse, String> {
    #[cfg(windows)]
    {
        return win_impl::inspect_window(target_id, max_depth, max_nodes);
    }
    #[cfg(target_os = "macos")]
    {
        return mac_impl::inspect_window(target_id, max_depth, max_nodes);
    }
    #[cfg(target_os = "linux")]
    {
        return linux_impl::inspect_window(target_id, max_depth, max_nodes);
    }
}

pub fn format_for_llm(target_id: &str, verbosity: &str) -> Result<LlmFormatResponse, String> {
    let verb = match verbosity.trim().to_lowercase().as_str() {
        "glance" | "text_only" | "interactive" | "compact" | "full" => {
            verbosity.trim().to_lowercase()
        }
        _ => "compact".to_string(),
    };

    let target_lower = target_id.trim().to_lowercase();
    if target_lower == "all" || target_lower == "desktop" || verb == "glance" {
        let win_data = list_windows();
        let mut lines = vec![format!(
            "# Desktop Glance ({} windows, scanned in {:.1}ms)",
            win_data.count, win_data.scan_time_ms
        )];

        for w in &win_data.windows {
            let state = if w.focused {
                "FOCUSED"
            } else if w.minimized {
                "MINIMIZED"
            } else if w.offscreen {
                "OFFSCREEN"
            } else {
                "VISIBLE"
            };
            lines.push(format!(
                "- [{}] hwnd:{} pid:{} ({}) \"{}\"",
                state, w.hwnd, w.pid, w.process, w.title
            ));
        }

        if target_lower != "all" && target_lower != "desktop" {
            if let Ok(snap) = inspect_window(target_id, 6, 150) {
                let s = &snap.stats;
                let w = &snap.window;
                lines.push(format!(
                    "\n## Target Window Summary: \"{}\" ({})",
                    w.title, w.process
                ));
                lines.push(format!(
                    "- Semantic Elements: {} (pruned from {} raw nodes)",
                    s.semantic_nodes, s.raw_nodes
                ));
                lines.push(format!(
                    "- Text Buffers: {} ({} total chars, {} off-screen chars)",
                    s.text_buffers_found, s.total_extracted_chars, s.offscreen_extracted_chars
                ));
                for b in &snap.text_buffers {
                    let prev: String = b.preview.chars().take(100).collect();
                    lines.push(format!(
                        "  * [{}] {} \"{}\" ({}): {} chars ({} offscreen) -> \"{}\"",
                        b.id, b.role, b.name, b.source, b.total_chars, b.offscreen_chars, prev
                    ));
                }
            }
        }

        let text_out = lines.join("\n");
        let char_count = text_out.len();
        return Ok(LlmFormatResponse {
            verbosity: "glance".to_string(),
            target: target_id.to_string(),
            hwnd: None,
            process: None,
            char_count,
            estimated_tokens: (char_count / 4).max(1),
            stats: None,
            llm_text: text_out,
        });
    }

    let max_depth = if verb == "interactive" || verb == "compact" {
        8
    } else {
        11
    };
    let max_nodes = if verb == "interactive" { 250 } else { 400 };

    let snap = inspect_window(target_id, max_depth, max_nodes)?;
    let w = &snap.window;
    let s = &snap.stats;

    let text_out = if verb == "text_only" {
        let mut lines = vec![
            format!(
                "# Extracted Text Only — \"{}\" ({}, pid:{})",
                w.title, w.process, w.pid
            ),
            format!(
                "# Buffers: {} | Total Chars: {} | Off-Screen Chars: {}",
                s.text_buffers_found, s.total_extracted_chars, s.offscreen_extracted_chars
            ),
            String::new(),
        ];

        if !snap.text_buffers.is_empty() {
            for b in &snap.text_buffers {
                lines.push(format!(
                    "--- [{}] {} \"{}\" ({} | Total: {} chars, {} lines | Off-screen: {} chars) ---",
                    b.id, b.role, b.name, b.source, b.total_chars, b.total_lines, b.offscreen_chars
                ));
                lines.push(b.full_text.clone());
                lines.push(String::new());
            }
        } else {
            lines.push(
                "# (No TextPattern container buffers; falling back to readable text nodes)"
                    .to_string(),
            );
            fn collect_names(nodes: &[TreeNode], out: &mut Vec<String>) {
                for n in nodes {
                    if !n.name.is_empty()
                        && matches!(
                            n.role.as_str(),
                            "text" | "document" | "edit" | "listitem" | "link" | "tabitem"
                        )
                    {
                        out.push(format!("- [{}] {}", n.role, n.name));
                    }
                    collect_names(&n.children, out);
                }
            }
            collect_names(&snap.tree, &mut lines);
        }
        lines.join("\n")
    } else if verb == "interactive" {
        let mut lines = vec![
            format!(
                "# Interactive Controls & Buffers — \"{}\" ({}, pid:{})",
                w.title, w.process, w.pid
            ),
            "controls:".to_string(),
        ];
        fn collect_interactive(nodes: &[TreeNode], out: &mut Vec<String>) {
            for n in nodes {
                let is_action = matches!(
                    n.role.as_str(),
                    "button"
                        | "splitbutton"
                        | "checkbox"
                        | "radiobutton"
                        | "combobox"
                        | "edit"
                        | "link"
                        | "menuitem"
                        | "tabitem"
                        | "treeitem"
                        | "listitem"
                        | "slider"
                        | "document"
                );
                if (is_action || n.has_text) && (!n.offscreen || n.has_text) {
                    let mut flags = Vec::new();
                    if n.focused {
                        flags.push("focused".to_string());
                    }
                    if n.has_text {
                        flags.push(format!(
                            "{}:{}c",
                            n.text_source.as_deref().unwrap_or("Text"),
                            n.total_chars
                        ));
                    }
                    let f_str = if flags.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", flags.join(", "))
                    };
                    let name_trunc: String = n.name.chars().take(80).collect();
                    let name_str = if name_trunc.is_empty() {
                        String::new()
                    } else {
                        format!(" \"{}\"", name_trunc)
                    };
                    out.push(format!("  - {}{} [{}]{}", n.role, name_str, n.id, f_str));
                }
                collect_interactive(&n.children, out);
            }
        }
        collect_interactive(&snap.tree, &mut lines);
        lines.join("\n")
    } else if verb == "compact" {
        snap.llm_yaml.clone()
    } else {
        let mut lines = vec![
            snap.llm_yaml.clone(),
            "\n# =========================================================================="
                .to_string(),
            "# FULL UNRENDERED & VISIBLE DOCUMENT TEXT BUFFERS (INLINE)".to_string(),
            "# =========================================================================="
                .to_string(),
        ];
        for b in &snap.text_buffers {
            lines.push(format!(
                "\n## Buffer {} ({} \"{}\") — {} chars ({} off-screen)",
                b.id, b.role, b.name, b.total_chars, b.offscreen_chars
            ));
            lines.push("```text".to_string());
            lines.push(b.full_text.clone());
            lines.push("```".to_string());
        }
        lines.join("\n")
    };

    let char_count = text_out.len();
    Ok(LlmFormatResponse {
        verbosity: verb,
        target: w.title.clone(),
        hwnd: Some(w.hwnd),
        process: Some(w.process.clone()),
        char_count,
        estimated_tokens: (char_count / 4).max(1),
        stats: Some(s.clone()),
        llm_text: text_out,
    })
}

// ============================================================================
// WINDOWS NATIVE IMPLEMENTATION (Win32 + UIAutomationCore COM in Pure Rust)
// ============================================================================
#[cfg(windows)]
mod win_impl {
    use super::*;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows::core::{Interface, BSTR, PCWSTR, PWSTR};
    use windows::Win32::Foundation::{BOOL, CloseHandle, HWND, LPARAM, RECT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::StationsAndDesktops::{
        EnumDesktopWindows, OpenDesktopW, SetThreadDesktop,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationCacheRequest, IUIAutomationElement,
        IUIAutomationTextPattern, IUIAutomationTreeWalker, IUIAutomationValuePattern,
        UIA_ControlTypePropertyId, UIA_ClassNamePropertyId, UIA_HasKeyboardFocusPropertyId,
        UIA_IsEnabledPropertyId, UIA_IsOffscreenPropertyId, UIA_IsTextPatternAvailablePropertyId,
        UIA_IsValuePatternAvailablePropertyId, UIA_NamePropertyId, UIA_TextPatternId,
        UIA_ValuePatternId,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindowTextLengthW, IsIconic, IsWindowVisible,
        SystemParametersInfoW, SPIF_SENDCHANGE, SPI_GETSCREENREADER, SPI_SETSCREENREADER,
    };

    fn control_type_name(ctrl_id: i32) -> &'static str {
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
            50034 => "header",
            50035 => "headeritem",
            50036 => "table",
            50037 => "titlebar",
            50038 => "separator",
            _ => "custom",
        }
    }

    fn is_container_role(role: &str) -> bool {
        matches!(
            role,
            "pane" | "group" | "custom" | "separator" | "scrollbar" | "thumb" | "image" | "tooltip"
        )
    }

    fn is_text_bearing_role(role: &str) -> bool {
        matches!(role, "document" | "edit" | "pane" | "window")
    }

    pub fn set_screen_reader_flag(enabled: bool) -> bool {
        unsafe {
            let _ = SystemParametersInfoW(
                SPI_SETSCREENREADER,
                if enabled { 1 } else { 0 },
                None,
                SPIF_SENDCHANGE,
            );
        }
        get_screen_reader_flag()
    }

    pub fn get_screen_reader_flag() -> bool {
        unsafe {
            let mut flag: BOOL = BOOL(0);
            let _ = SystemParametersInfoW(
                SPI_GETSCREENREADER,
                0,
                Some(&mut flag as *mut BOOL as *mut std::ffi::c_void),
                 Default::default(),
            );
            flag.as_bool()
        }
    }

    fn attach_interactive_desktop() -> Option<windows::Win32::System::StationsAndDesktops::HDESK> {
        unsafe {
            let wide: Vec<u16> = "Default\0".encode_utf16().collect();
            if let Ok(hdesk) = OpenDesktopW(
                PCWSTR(wide.as_ptr()),
                windows::Win32::System::StationsAndDesktops::DESKTOP_CONTROL_FLAGS(0),
                false,
                0x01FF,
            ) {
                let _ = SetThreadDesktop(hdesk);
                return Some(hdesk);
            }
        }
        None
    }

    fn init_uia() -> Result<IUIAutomation, String> {
        attach_interactive_desktop();
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("Failed to initialize IUIAutomation: {}", e))
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

    unsafe extern "system" fn enum_wnd_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let vec_ptr = lparam.0 as *mut Vec<isize>;
        if IsWindowVisible(hwnd).as_bool() {
            let len = GetWindowTextLengthW(hwnd);
            if len > 0 {
                (*vec_ptr).push(hwnd.0 as isize);
            }
        }
        BOOL(1)
    }

    fn enum_interactive_hwnds() -> Vec<isize> {
        let hdesk = attach_interactive_desktop();
        let mut hwnds: Vec<isize> = Vec::new();
        let lparam = LPARAM(&mut hwnds as *mut Vec<isize> as isize);
        unsafe {
            if let Some(desk) = hdesk {
                let _ = EnumDesktopWindows(desk, Some(enum_wnd_proc), lparam);
            } else {
                let _ = EnumWindows(Some(enum_wnd_proc), lparam);
            }
        }
        hwnds
    }

    pub fn list_windows() -> WindowListResponse {
        let t0 = Instant::now();
        set_screen_reader_flag(true);
        let uia = match init_uia() {
            Ok(u) => u,
            Err(_) => {
                return WindowListResponse {
                    platform: "Windows UIA (Rust Native)".to_string(),
                    screen_reader_flag: get_screen_reader_flag(),
                    scan_time_ms: 0.0,
                    count: 0,
                    windows: vec![],
                }
            }
        };

        let hwnds = enum_interactive_hwnds();
        let focused_hwnd = unsafe { GetForegroundWindow().0 as isize };
        let mut windows = Vec::new();

        for hwnd_val in hwnds {
            let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
            unsafe {
                if let Ok(el) = uia.ElementFromHandle(hwnd) {
                    let name = el
                        .CurrentName()
                        .unwrap_or_else(|_| BSTR::new())
                        .to_string()
                        .trim()
                        .to_string();
                    let cls = el
                        .CurrentClassName()
                        .unwrap_or_else(|_| BSTR::new())
                        .to_string();
                    let pid = el.CurrentProcessId().unwrap_or(0) as u32;
                    let offscreen = el.CurrentIsOffscreen().unwrap_or(BOOL(0)).as_bool();

                    if matches!(cls.as_str(), "Progman" | "WorkerW" | "IME" | "MSCTFIME UI") {
                        continue;
                    }
                    if cls == "Windows.UI.Core.CoreWindow"
                        && (name.is_empty() || name == "Windows Input Experience")
                    {
                        continue;
                    }
                    if name == "Program Manager" {
                        continue;
                    }

                    let rect: RECT = el.CurrentBoundingRectangle().unwrap_or_default();
                    let w = (rect.right - rect.left).max(0);
                    let h = (rect.bottom - rect.top).max(0);
                    let is_min = IsIconic(hwnd).as_bool();
                    if w == 0 && h == 0 && !is_min {
                        continue;
                    }

                    let proc_name = get_process_name(pid);
                    let title = if name.is_empty() {
                        format!("({} - {})", proc_name, cls)
                    } else {
                        name
                    };

                    windows.push(WindowInfo {
                        hwnd: hwnd_val,
                        pid,
                        process: proc_name,
                        title,
                        class_name: cls,
                        offscreen,
                        minimized: is_min,
                        focused: hwnd_val == focused_hwnd,
                        bounds: WindowBounds {
                            left: rect.left,
                            top: rect.top,
                            width: w,
                            height: h,
                        },
                    });
                }
            }
        }

        windows.sort_by(|a, b| {
            (!a.focused)
                .cmp(&(!b.focused))
                .then(a.minimized.cmp(&b.minimized))
                .then(a.offscreen.cmp(&b.offscreen))
                .then(a.process.to_lowercase().cmp(&b.process.to_lowercase()))
        });

        let elapsed_ms = (t0.elapsed().as_secs_f64() * 1000.0 * 10.0).round() / 10.0;
        let count = windows.len();
        WindowListResponse {
            platform: "Windows UIA (Rust Native COM + Default Desktop Bridge)".to_string(),
            screen_reader_flag: get_screen_reader_flag(),
            scan_time_ms: elapsed_ms,
            count,
            windows,
        }
    }

    fn find_window_element(uia: &IUIAutomation, target_id: &str) -> Option<IUIAutomationElement> {
        let hwnds = enum_interactive_hwnds();
        let target_lower = target_id.to_lowercase();

        // 1. Exact HWND match
        for &hwnd_val in &hwnds {
            if hwnd_val.to_string() == target_id {
                let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                if let Ok(el) = unsafe { uia.ElementFromHandle(hwnd) } {
                    return Some(el);
                }
            }
        }

        // 2. Exact PID match
        for &hwnd_val in &hwnds {
            let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
            if let Ok(el) = unsafe { uia.ElementFromHandle(hwnd) } {
                let pid = unsafe { el.CurrentProcessId().unwrap_or(0) };
                if pid.to_string() == target_id {
                    return Some(el);
                }
            }
        }

        // 3. Substring match on Title, Class, or Process
        for &hwnd_val in &hwnds {
            let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
            if let Ok(el) = unsafe { uia.ElementFromHandle(hwnd) } {
                let name = unsafe { el.CurrentName().unwrap_or_default().to_string().to_lowercase() };
                let cls = unsafe {
                    el.CurrentClassName()
                        .unwrap_or_default()
                        .to_string()
                        .to_lowercase()
                };
                let pid = unsafe { el.CurrentProcessId().unwrap_or(0) as u32 };
                let proc_name = get_process_name(pid).to_lowercase();
                if name.contains(&target_lower)
                    || cls.contains(&target_lower)
                    || proc_name.contains(&target_lower)
                {
                    return Some(el);
                }
            }
        }
        None
    }

    fn build_cache_request(uia: &IUIAutomation) -> Result<IUIAutomationCacheRequest, String> {
        unsafe {
            let req = uia
                .CreateCacheRequest()
                .map_err(|e| format!("CreateCacheRequest failed: {}", e))?;
            let _ = req.AddProperty(UIA_ControlTypePropertyId);
            let _ = req.AddProperty(UIA_NamePropertyId);
            let _ = req.AddProperty(UIA_HasKeyboardFocusPropertyId);
            let _ = req.AddProperty(UIA_IsEnabledPropertyId);
            let _ = req.AddProperty(UIA_ClassNamePropertyId);
            let _ = req.AddProperty(UIA_IsOffscreenPropertyId);
            let _ = req.AddProperty(UIA_IsTextPatternAvailablePropertyId);
            let _ = req.AddProperty(UIA_IsValuePatternAvailablePropertyId);
            Ok(req)
        }
    }

    struct RawExtractedText {
        source: String,
        total_chars: usize,
        visible_chars: usize,
        offscreen_chars: usize,
        total_lines: usize,
        visible_lines: usize,
        preview: String,
        full_text: String,
        visible_text: String,
    }

    fn make_preview(text: &str, max_chars: usize) -> String {
        let cleaned = text.replace('\r', "").replace('\n', " ↵ ");
        let trimmed = cleaned.trim();
        let char_count = trimmed.chars().count();
        let mut prev: String = trimmed.chars().take(max_chars).collect();
        if char_count > max_chars {
            prev.push_str(" …");
        }
        prev
    }

    fn extract_element_text(el: &IUIAutomationElement, role: &str) -> Option<RawExtractedText> {
        unsafe {
            // 1. Try IUIAutomationTextPattern on document/edit/pane/window nodes
            if is_text_bearing_role(role) {
                if let Ok(unk) = el.GetCurrentPattern(UIA_TextPatternId) {
                    if let Ok(tp) = unk.cast::<IUIAutomationTextPattern>() {
                        if let Ok(doc_range) = tp.DocumentRange() {
                            if let Ok(bstr_full) = doc_range.GetText(65536) {
                                let full_text = bstr_full.to_string().trim().to_string();
                                if !full_text.is_empty() {
                                    let mut vis_text = String::new();
                                    if let Ok(vis_ranges) = tp.GetVisibleRanges() {
                                        let len = vis_ranges.Length().unwrap_or(0).min(40);
                                        let mut parts = Vec::new();
                                        for i in 0..len {
                                            if let Ok(r) = vis_ranges.GetElement(i) {
                                                if let Ok(t) = r.GetText(16384) {
                                                    let s = t.to_string();
                                                    if !s.is_empty() {
                                                        parts.push(s);
                                                    }
                                                }
                                            }
                                        }
                                        vis_text = parts.join("\n").trim().to_string();
                                    }

                                    let total_chars = full_text.chars().count();
                                    let visible_chars = vis_text.chars().count();
                                    let offscreen_chars = if visible_chars > 0 {
                                        total_chars.saturating_sub(visible_chars)
                                    } else {
                                        0
                                    };
                                    let total_lines = full_text.lines().count().max(1);
                                    let visible_lines = if vis_text.is_empty() {
                                        0
                                    } else {
                                        vis_text.lines().count().max(1)
                                    };
                                    let preview = make_preview(&full_text, 180);

                                    return Some(RawExtractedText {
                                        source: "TextPattern".to_string(),
                                        total_chars,
                                        visible_chars,
                                        offscreen_chars,
                                        total_lines,
                                        visible_lines,
                                        preview,
                                        full_text,
                                        visible_text: vis_text,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // 2. Try IUIAutomationValuePattern
            if let Ok(unk) = el.GetCurrentPattern(UIA_ValuePatternId) {
                if let Ok(vp) = unk.cast::<IUIAutomationValuePattern>() {
                    if let Ok(bstr_val) = vp.CurrentValue() {
                        let val = bstr_val.to_string().trim().to_string();
                        if !val.is_empty() {
                            let total_chars = val.chars().count();
                            let total_lines = val.lines().count().max(1);
                            let preview = make_preview(&val, 180);
                            return Some(RawExtractedText {
                                source: "ValuePattern".to_string(),
                                total_chars,
                                visible_chars: total_chars,
                                offscreen_chars: 0,
                                total_lines,
                                visible_lines: total_lines,
                                preview,
                                full_text: val.clone(),
                                visible_text: val,
                            });
                        }
                    }
                }
            }
        }
        None
    }

    pub fn inspect_window(
        target_id: &str,
        max_depth: usize,
        max_nodes: usize,
    ) -> Result<InspectResponse, String> {
        let t0 = Instant::now();
        let uia = init_uia()?;
        let walker: IUIAutomationTreeWalker = unsafe {
            uia.ControlViewWalker()
                .map_err(|e| format!("ControlViewWalker failed: {}", e))?
        };
        let cache_req = build_cache_request(&uia)?;

        let win_el_raw = find_window_element(&uia, target_id)
            .ok_or_else(|| format!("Window '{}' not found.", target_id))?;

        let win_el = unsafe {
            win_el_raw
                .BuildUpdatedCache(&cache_req)
                .unwrap_or_else(|_| win_el_raw.clone())
        };

        let win_title = unsafe {
            win_el
                .CachedName()
                .or_else(|_| win_el_raw.CurrentName())
                .unwrap_or_default()
                .to_string()
                .trim()
                .to_string()
        };
        let win_pid = unsafe { win_el_raw.CurrentProcessId().unwrap_or(0) as u32 };
        let win_hwnd = unsafe {
            win_el_raw
                .CurrentNativeWindowHandle()
                .unwrap_or_default()
                .0 as isize
        };
        let win_cls = unsafe {
            win_el
                .CachedClassName()
                .or_else(|_| win_el_raw.CurrentClassName())
                .unwrap_or_default()
                .to_string()
        };
        let win_proc = get_process_name(win_pid);
        let win_offscreen = unsafe {
            win_el
                .CachedIsOffscreen()
                .unwrap_or(BOOL(0))
                .as_bool()
        };

        let mut raw_nodes_visited = 0usize;
        let mut pruned_nodes_count = 0usize;
        let mut text_buffers: Vec<TextBufferInfo> = Vec::new();
        let mut seen_text_sigs: HashSet<(String, usize, String)> = HashSet::new();
        let mut yaml_lines: Vec<String> = Vec::new();

        #[allow(clippy::too_many_arguments)]
        fn walk(
            el: &IUIAutomationElement,
            depth: usize,
            max_depth: usize,
            max_nodes: usize,
            walker: &IUIAutomationTreeWalker,
            cache_req: &IUIAutomationCacheRequest,
            raw_nodes_visited: &mut usize,
            pruned_nodes_count: &mut usize,
            text_buffers: &mut Vec<TextBufferInfo>,
            seen_text_sigs: &mut HashSet<(String, usize, String)>,
            yaml_lines: &mut Vec<String>,
        ) -> Vec<TreeNode> {
            if depth > max_depth || *pruned_nodes_count >= max_nodes {
                return vec![];
            }

            *raw_nodes_visited += 1;

            let ctrl_id = unsafe {
                el.CachedControlType()
                    .or_else(|_| el.CurrentControlType())
                    .map(|c| c.0)
                    .unwrap_or(50025)
            };
            let role = control_type_name(ctrl_id).to_string();
            let name = unsafe {
                el.CachedName()
                    .or_else(|_| el.CurrentName())
                    .unwrap_or_default()
                    .to_string()
                    .trim()
                    .to_string()
            };
            let offscreen = unsafe {
                el.CachedIsOffscreen()
                    .or_else(|_| el.CurrentIsOffscreen())
                    .unwrap_or(BOOL(0))
                    .as_bool()
            };
            let enabled = unsafe {
                el.CachedIsEnabled()
                    .or_else(|_| el.CurrentIsEnabled())
                    .unwrap_or(BOOL(1))
                    .as_bool()
            };
            let has_focus = unsafe {
                el.CachedHasKeyboardFocus()
                    .or_else(|_| el.CurrentHasKeyboardFocus())
                    .unwrap_or(BOOL(0))
                    .as_bool()
            };

            // Skip unnamed offscreen leaf clutter
            if offscreen && name.is_empty() {
                if matches!(
                    role.as_str(),
                    "menu"
                        | "menuitem"
                        | "tooltip"
                        | "image"
                        | "scrollbar"
                        | "thumb"
                        | "separator"
                        | "button"
                        | "listitem"
                ) {
                    return vec![];
                }
            }

            let text_info = extract_element_text(el, &role);
            let is_meaningful = !name.is_empty()
                || text_info.is_some()
                || (!is_container_role(&role) && !offscreen);

            let mut current_node: Option<TreeNode> = None;
            let mut next_depth = depth;

            if is_meaningful {
                *pruned_nodes_count += 1;
                let eid = format!("@e{}", *pruned_nodes_count);
                let indent = "  ".repeat(depth);
                let clean_name = name.replace('\r', "").replace('\n', " ");
                let name_trunc: String = clean_name.chars().take(95).collect();
                let name_part = if name_trunc.is_empty() {
                    String::new()
                } else {
                    format!(" \"{}\"", name_trunc)
                };

                let mut badges = Vec::new();
                if has_focus {
                    badges.push("focused".to_string());
                }
                if !enabled {
                    badges.push("disabled".to_string());
                }
                if offscreen {
                    badges.push("offscreen".to_string());
                }

                if let Some(ref ti) = text_info {
                    if ti.source == "TextPattern" {
                        let off_str = if ti.offscreen_chars > 0 {
                            format!(" [{} off-screen]", ti.offscreen_chars)
                        } else {
                            String::new()
                        };
                        badges.push(format!(
                            "TextPattern: {} chars / {} lines{}",
                            ti.total_chars, ti.total_lines, off_str
                        ));
                    } else {
                        badges.push(format!("{}: {} chars", ti.source, ti.total_chars));
                    }

                    let sig_prev: String = ti.preview.chars().take(80).collect();
                    let sig = (ti.source.clone(), ti.total_chars, sig_prev);
                    if ti.total_chars > 0 && !seen_text_sigs.contains(&sig) {
                        seen_text_sigs.insert(sig);
                        text_buffers.push(TextBufferInfo {
                            id: eid.clone(),
                            role: role.clone(),
                            name: if clean_name.is_empty() {
                                format!("<{}>", role)
                            } else {
                                clean_name.clone()
                            },
                            source: ti.source.clone(),
                            total_chars: ti.total_chars,
                            visible_chars: ti.visible_chars,
                            offscreen_chars: ti.offscreen_chars,
                            total_lines: ti.total_lines,
                            visible_lines: ti.visible_lines,
                            preview: ti.preview.clone(),
                            full_text: ti.full_text.clone(),
                            visible_text: ti.visible_text.clone(),
                        });
                    }
                }

                let badge_str = if badges.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", badges.join(", "))
                };
                yaml_lines.push(format!(
                    "{}- {}{} [{}]{}",
                    indent, role, name_part, eid, badge_str
                ));

                if let Some(ref ti) = text_info {
                    if !ti.preview.is_empty() && ti.preview != clean_name {
                        yaml_lines.push(format!("{}    text: \"{}\"", indent, ti.preview));
                    }
                }

                current_node = Some(TreeNode {
                    id: eid,
                    role,
                    name: clean_name,
                    depth,
                    offscreen,
                    focused: has_focus,
                    enabled,
                    has_text: text_info.is_some(),
                    text_source: text_info.as_ref().map(|t| t.source.clone()),
                    total_chars: text_info.as_ref().map(|t| t.total_chars).unwrap_or(0),
                    offscreen_chars: text_info.as_ref().map(|t| t.offscreen_chars).unwrap_or(0),
                    preview: text_info
                        .as_ref()
                        .map(|t| t.preview.clone())
                        .unwrap_or_default(),
                    children: vec![],
                });
                next_depth = depth + 1;
            }

            let mut children_nodes = Vec::new();
            unsafe {
                let mut child_opt = walker.GetFirstChildElementBuildCache(el, cache_req).ok();
                while let Some(child) = child_opt {
                    if *pruned_nodes_count >= max_nodes {
                        break;
                    }
                    let sub = walk(
                        &child,
                        next_depth,
                        max_depth,
                        max_nodes,
                        walker,
                        cache_req,
                        raw_nodes_visited,
                        pruned_nodes_count,
                        text_buffers,
                        seen_text_sigs,
                        yaml_lines,
                    );
                    children_nodes.extend(sub);
                    child_opt = walker
                        .GetNextSiblingElementBuildCache(&child, cache_req)
                        .ok();
                }
            }

            if let Some(mut node) = current_node {
                node.children = children_nodes;
                vec![node]
            } else {
                children_nodes
            }
        }

        let tree_root = walk(
            &win_el,
            0,
            max_depth,
            max_nodes,
            &walker,
            &cache_req,
            &mut raw_nodes_visited,
            &mut pruned_nodes_count,
            &mut text_buffers,
            &mut seen_text_sigs,
            &mut yaml_lines,
        );

        let elapsed_ms = (t0.elapsed().as_secs_f64() * 1000.0 * 10.0).round() / 10.0;
        let compression_pct = if raw_nodes_visited > 0 {
            ((1.0 - (pruned_nodes_count as f64 / raw_nodes_visited as f64)) * 1000.0).round()
                / 10.0
        } else {
            0.0
        };

        let header = format!(
            "# Semantic GUI-SSH Snapshot (Rust Native Engine)\nwindow: \"{}\" (process: {}, pid: {}, hwnd: {})\nstats: {} semantic nodes (pruned from {} raw UIA nodes, {:.1}% noise removed in {:.1}ms)\nelements:\n",
            win_title, win_proc, win_pid, win_hwnd, pruned_nodes_count, raw_nodes_visited, compression_pct, elapsed_ms
        );
        let llm_yaml = format!("{}{}", header, yaml_lines.join("\n"));
        let total_extracted_chars: usize = text_buffers.iter().map(|b| b.total_chars).sum();
        let offscreen_extracted_chars: usize =
            text_buffers.iter().map(|b| b.offscreen_chars).sum();
        let estimated_tokens = (llm_yaml.len() / 4).max(1);

        Ok(InspectResponse {
            window: InspectWindowMeta {
                hwnd: win_hwnd,
                pid: win_pid,
                process: win_proc,
                title: win_title,
                class_name: win_cls,
                offscreen: win_offscreen,
            },
            stats: InspectStats {
                raw_nodes: raw_nodes_visited,
                semantic_nodes: pruned_nodes_count,
                compression_pct,
                text_buffers_found: text_buffers.len(),
                total_extracted_chars,
                offscreen_extracted_chars,
                scan_time_ms: elapsed_ms,
                estimated_tokens,
            },
            llm_yaml,
            text_buffers,
            tree: tree_root,
        })
    }
}

// ============================================================================
// MACOS & LINUX RUST ADAPTERS (Bridges to AXUIElement / AT-SPI2)
// ============================================================================
#[cfg(target_os = "macos")]
mod mac_impl {
    use super::*;
    use std::process::Command;

    pub fn set_screen_reader_flag(_enabled: bool) -> bool {
        true
    }
    pub fn get_screen_reader_flag() -> bool {
        true
    }
    pub fn list_windows() -> WindowListResponse {
        let out = Command::new("python3")
            .args(["-c", "import json, a11y_macos; print(json.dumps(a11y_macos.MacOSA11yEngine().list_windows()))"])
            .output();
        if let Ok(o) = out {
            if let Ok(res) = serde_json::from_slice::<WindowListResponse>(&o.stdout) {
                return res;
            }
        }
        WindowListResponse {
            platform: "macOS AXUIElement (Rust)".to_string(),
            screen_reader_flag: true,
            scan_time_ms: 0.0,
            count: 0,
            windows: vec![],
        }
    }
    pub fn inspect_window(
        target_id: &str,
        max_depth: usize,
        max_nodes: usize,
    ) -> Result<InspectResponse, String> {
        let script = format!(
            "import json, a11y_macos; print(json.dumps(a11y_macos.MacOSA11yEngine().inspect_window({:?}, {}, {})))",
            target_id, max_depth, max_nodes
        );
        let out = Command::new("python3")
            .args(["-c", &script])
            .output()
            .map_err(|e| e.to_string())?;
        serde_json::from_slice::<InspectResponse>(&out.stdout).map_err(|e| e.to_string())
    }
}

#[cfg(target_os = "linux")]
mod linux_impl {
    use super::*;
    use std::process::Command;

    pub fn set_screen_reader_flag(_enabled: bool) -> bool {
        true
    }
    pub fn get_screen_reader_flag() -> bool {
        true
    }
    pub fn list_windows() -> WindowListResponse {
        let out = Command::new("python3")
            .args(["-c", "import json, a11y_linux; print(json.dumps(a11y_linux.LinuxA11yEngine().list_windows()))"])
            .output();
        if let Ok(o) = out {
            if let Ok(res) = serde_json::from_slice::<WindowListResponse>(&o.stdout) {
                return res;
            }
        }
        WindowListResponse {
            platform: "Linux AT-SPI2 D-Bus (Rust)".to_string(),
            screen_reader_flag: true,
            scan_time_ms: 0.0,
            count: 0,
            windows: vec![],
        }
    }
    pub fn inspect_window(
        target_id: &str,
        max_depth: usize,
        max_nodes: usize,
    ) -> Result<InspectResponse, String> {
        let script = format!(
            "import json, a11y_linux; print(json.dumps(a11y_linux.LinuxA11yEngine().inspect_window({:?}, {}, {})))",
            target_id, max_depth, max_nodes
        );
        let out = Command::new("python3")
            .args(["-c", &script])
            .output()
            .map_err(|e| e.to_string())?;
        serde_json::from_slice::<InspectResponse>(&out.stdout).map_err(|e| e.to_string())
    }
}
