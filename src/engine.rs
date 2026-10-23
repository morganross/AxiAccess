use serde::{Deserialize, Serialize};

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_id: Option<String>,
    #[serde(default = "default_true")]
    pub bounds_known: bool,
    #[serde(default = "default_true")]
    pub state_known: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowListResponse {
    pub platform: String,
    pub screen_reader_flag: bool,
    pub scan_time_ms: f64,
    pub count: usize,
    pub windows: Vec<WindowInfo>,
    #[serde(default)]
    pub issues: Vec<String>,
    #[serde(default)]
    pub truncated: bool,
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
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub protected: bool,
    #[serde(default = "default_true")]
    pub visibility_known: bool,
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
    #[serde(default)]
    pub protected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_role: Option<String>,
    #[serde(default)]
    pub states: serde_json::Value,
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
    #[serde(default)]
    pub issues: Vec<String>,
    #[serde(default)]
    pub truncated: bool,
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

fn default_true() -> bool {
    true
}

/// Compatibility query; observation never changes session-wide accessibility settings.
pub fn set_screen_reader_flag(_enabled: bool) -> bool {
    get_screen_reader_flag()
}

pub fn get_screen_reader_flag() -> bool {
    #[cfg(windows)]
    {
        return crate::platform::windows::get_screen_reader_flag();
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub fn try_list_windows() -> Result<WindowListResponse, String> {
    crate::platform::list_windows()
}

/// Legacy shape preserves callers while recording native failures explicitly.
/// New protocol callers must use try_list_windows and preserve Result semantics.
pub fn list_windows() -> WindowListResponse {
    try_list_windows().unwrap_or_else(|error| WindowListResponse {
        platform: std::env::consts::OS.to_string(),
        screen_reader_flag: false,
        scan_time_ms: 0.0,
        count: 0,
        windows: vec![],
        issues: vec![error],
        truncated: true,
    })
}

pub fn capabilities() -> serde_json::Value {
    crate::platform::capabilities()
}

pub fn native_event_revision() -> u64 {
    crate::platform::native_event_revision()
}
pub fn native_event_status() -> serde_json::Value {
    crate::platform::native_event_status()
}
pub fn wait_native_events(since: u64, timeout: std::time::Duration) -> u64 {
    crate::platform::wait_native_events(since, timeout)
}

pub fn inspect_window(
    target_id: &str,
    max_depth: usize,
    max_nodes: usize,
) -> Result<InspectResponse, String> {
    crate::platform::inspect_window(target_id, max_depth, max_nodes)
}

pub fn inspect_window_with_options(
    target_id: &str,
    max_depth: usize,
    max_nodes: usize,
    capture_text: bool,
) -> Result<InspectResponse, String> {
    crate::platform::inspect_window_with_options(target_id, max_depth, max_nodes, capture_text)
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
        let win_data = try_list_windows()?;
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
