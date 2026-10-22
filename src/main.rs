mod engine;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Parser)]
#[command(
    name = "axiaccess",
    version = "2.0.0",
    about = "AxiAccess: Native Rust Universal Remote Desktop Accessibility Observer & 5-Tier LLM Sidechannel"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// One-shot target window (if no subcommand specified and --cli is passed)
    #[arg(long)]
    cli: Option<String>,

    /// LLM verbosity tier: glance, interactive, compact, text_only, full
    #[arg(short, long, default_value = "compact")]
    verbosity: String,

    /// Output JSON instead of raw Markdown/YAML in CLI mode
    #[arg(long)]
    json: bool,

    /// Run as an MCP JSON-RPC 2.0 server over stdio
    #[arg(long)]
    mcp: bool,

    /// Port for the HTTP + WebSocket server
    #[arg(short, long, default_value_t = 8765)]
    port: u16,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the HTTP REST API, WebSocket event stream, and Web Dashboard (Default)
    Serve {
        #[arg(short, long, default_value_t = 8765)]
        port: u16,
    },
    /// Run as a Model Context Protocol (MCP) JSON-RPC 2.0 server over stdio
    Mcp,
    /// Query a window or desktop directly from the terminal / SSH
    Query {
        /// Window title substring, process name, PID, HWND, or 'all'
        #[arg(default_value = "all")]
        target: String,
        /// Verbosity tier: glance, interactive, compact, text_only, full
        #[arg(short, long, default_value = "glance")]
        verbosity: String,
        /// Output structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Connect to remote host (over SSH/HTTP/local) and convert into coding-agent formats
    Reader {
        /// Target window or 'all'
        #[arg(default_value = "all")]
        target: String,
        /// Remote SSH host (e.g. user@dev-server)
        #[arg(long)]
        ssh: Option<String>,
        /// SSH port (if non-standard 22)
        #[arg(short, long)]
        port: Option<u16>,
        /// Path to SSH private key
        #[arg(short, long)]
        identity: Option<String>,
        /// HTTP URL to remote AxiAccess daemon
        #[arg(long)]
        url: Option<String>,
        /// Output format for coding agent: xml, markdown, diff, json
        #[arg(short, long, default_value = "xml")]
        format: String,
        /// Return only what changed since last inspection
        #[arg(long)]
        diff: bool,
    },
}

#[derive(Deserialize)]
struct LlmQueryParams {
    target: Option<String>,
    verbosity: Option<String>,
    format: Option<String>,
}

#[derive(Deserialize)]
struct InspectQueryParams {
    max_depth: Option<usize>,
    max_nodes: Option<usize>,
}

#[derive(Deserialize)]
struct WakeupBody {
    enabled: Option<bool>,
}

#[derive(Deserialize)]
struct SpawnDemoBody {
    lines: Option<usize>,
}

fn now_hms() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

async fn api_windows() -> impl IntoResponse {
    let res = tokio::task::spawn_blocking(engine::list_windows)
        .await
        .unwrap();
    Json(res)
}

async fn api_llm(Query(params): Query<LlmQueryParams>) -> impl IntoResponse {
    let target = params.target.unwrap_or_else(|| "all".to_string());
    let verbosity = params.verbosity.unwrap_or_else(|| "compact".to_string());
    let fmt = params.format.unwrap_or_else(|| "json".to_string());

    let res = tokio::task::spawn_blocking(move || engine::format_for_llm(&target, &verbosity))
        .await
        .unwrap();

    match res {
        Ok(data) => {
            if matches!(
                fmt.to_lowercase().as_str(),
                "text" | "plain" | "markdown" | "yaml"
            ) {
                (StatusCode::OK, data.llm_text).into_response()
            } else {
                (StatusCode::OK, Json(data)).into_response()
            }
        }
        Err(e) => (StatusCode::NOT_FOUND, Json(json!({ "detail": e }))).into_response(),
    }
}

async fn api_inspect(
    Path(target_id): Path<String>,
    Query(params): Query<InspectQueryParams>,
) -> impl IntoResponse {
    let max_depth = params.max_depth.unwrap_or(9);
    let max_nodes = params.max_nodes.unwrap_or(350);

    let res = tokio::task::spawn_blocking(move || {
        engine::inspect_window(&target_id, max_depth, max_nodes)
    })
    .await
    .unwrap();

    match res {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(json!({ "detail": e }))).into_response(),
    }
}

async fn api_wakeup(Json(body): Json<WakeupBody>) -> impl IntoResponse {
    let enabled = body.enabled.unwrap_or(true);
    let state = tokio::task::spawn_blocking(move || engine::set_screen_reader_flag(enabled))
        .await
        .unwrap();
    Json(json!({ "screen_reader_flag": state }))
}

async fn api_spawn_test_pad(Json(body): Json<SpawnDemoBody>) -> impl IntoResponse {
    let total_lines = body.lines.unwrap_or(250);
    let sample_path = std::env::current_dir()
        .unwrap_or_default()
        .join("offscreen_test_doc.txt");

    let mut lines = vec![
        "==========================================================================".to_string(),
        " AXIACCESS RUST NATIVE UNRENDERED / OFF-SCREEN TEXT VERIFICATION DOCUMENT".to_string(),
        "==========================================================================".to_string(),
        "Only the first ~25 lines of this file are painted in the editor's visible".to_string(),
        "viewport. Lines 30 through 250 are completely scrolled off-screen and have".to_string(),
        "NEVER been rendered into the desktop framebuffer.".to_string(),
        "".to_string(),
        "Yet our Rust IUIAutomationTextPattern::DocumentRange extractor retrieves".to_string(),
        "all 250 lines in <20ms without scrolling or moving window focus!".to_string(),
        "--------------------------------------------------------------------------".to_string(),
    ];

    for i in 11..=total_lines {
        let zone = if i <= 25 {
            "VISIBLE VIEWPORT ZONE"
        } else {
            "OFF-SCREEN / UNRENDERED BUFFER ZONE"
        };
        let secret = if i == total_lines.saturating_sub(5) {
            " [SECRET_MARKER_RUST_992]"
        } else {
            ""
        };
        lines.push(format!(
            "Line {:03}/{:03} | [{}] | Semantic payload record #{}{}",
            i,
            total_lines,
            zone,
            i * 137,
            secret
        ));
    }

    let _ = std::fs::write(&sample_path, lines.join("\n"));

    #[cfg(windows)]
    {
        let _ = Command::new("notepad.exe").arg(&sample_path).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open")
            .args(["-a", "TextEdit"])
            .arg(&sample_path)
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("xdg-open").arg(&sample_path).spawn();
    }

    tokio::time::sleep(Duration::from_millis(750)).await;
    Json(json!({
        "status": "spawned",
        "file": sample_path.to_string_lossy(),
        "lines": total_lines
    }))
}

async fn api_architecture() -> impl IntoResponse {
    Json(json!({
        "platforms": [
            {
                "os": "Windows (Rust Native Binary)",
                "status": "Active — Direct Win32 + IUIAutomation COM in Rust",
                "api": "windows-rs 0.58 (UIAutomationCore.dll / IUIAutomation)",
                "tree_walker": "ControlViewWalker + CreateCacheRequest (Zero-overhead Rust COM)",
                "deep_text_api": "IUIAutomationTextPattern::DocumentRange & GetVisibleRanges()",
                "wakeup_signal": "SystemParametersInfoW(SPI_SETSCREENREADER, 1)",
                "session_bridge": "OpenDesktopW('Default') + SetThreadDesktop"
            },
            {
                "os": "macOS",
                "status": "Supported (AXUIElement Bridge)",
                "api": "ApplicationServices / HIServices (AXUIElementRef)",
                "tree_walker": "AXUIElementCopyAttributeValue(kAXChildrenAttribute)",
                "deep_text_api": "kAXValueAttribute + kAXStringForRangeParameterizedAttribute",
                "wakeup_signal": "AXUIElementSetAttributeValue(app, 'AXManualAccessibility', kCFBooleanTrue)",
                "session_bridge": "Quartz CGWindowList + Aqua GUI Session over SSH stdio"
            },
            {
                "os": "Linux",
                "status": "Supported (AT-SPI2 D-Bus Bridge)",
                "api": "AT-SPI2 over D-Bus (org.a11y.atspi)",
                "tree_walker": "org.a11y.atspi.Accessible.GetChildren / GetRoleName",
                "deep_text_api": "org.a11y.atspi.Text.getText(0, -1)",
                "wakeup_signal": "org.a11y.Status.IsEnabled = true & ScreenReaderEnabled = true",
                "session_bridge": "Native D-Bus session bus forwarding over SSH"
            }
        ]
    }))
}

async fn ws_events(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_ws_events)
}

async fn handle_ws_events(mut socket: WebSocket) {
    let init_scan = tokio::task::spawn_blocking(engine::list_windows)
        .await
        .unwrap();
    let mut prev_windows: HashMap<isize, engine::WindowInfo> = init_scan
        .windows
        .into_iter()
        .map(|w| (w.hwnd, w))
        .collect();

    let welcome = json!({
        "timestamp": now_hms(),
        "type": "CONNECTED",
        "process": "AxiAccess (Rust)",
        "message": format!("AxiAccess Native Sidechannel attached to WinSta0\\Default ({} windows monitored, read-only).", prev_windows.len())
    });
    if socket
        .send(Message::Text(welcome.to_string()))
        .await
        .is_err()
    {
        return;
    }

    loop {
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let scan = match tokio::task::spawn_blocking(engine::list_windows).await {
            Ok(s) => s,
            Err(_) => break,
        };
        let curr_map: HashMap<isize, engine::WindowInfo> =
            scan.windows.into_iter().map(|w| (w.hwnd, w)).collect();
        let ts = now_hms();

        for (&hwnd, w) in &curr_map {
            if let Some(old_w) = prev_windows.get(&hwnd) {
                if w.title != old_w.title {
                    let ev = json!({
                        "timestamp": ts,
                        "type": "TITLE_CHANGED",
                        "hwnd": hwnd,
                        "process": w.process,
                        "message": format!("Title updated: \"{}\" → \"{}\"", old_w.title, w.title)
                    });
                    if socket.send(Message::Text(ev.to_string())).await.is_err() {
                        return;
                    }
                }
                if w.focused && !old_w.focused {
                    let ev = json!({
                        "timestamp": ts,
                        "type": "FOCUS_CHANGED",
                        "hwnd": hwnd,
                        "process": w.process,
                        "message": format!("Active focus moved to \"{}\" ({})", w.title, w.process)
                    });
                    if socket.send(Message::Text(ev.to_string())).await.is_err() {
                        return;
                    }
                }
            } else {
                let ev = json!({
                    "timestamp": ts,
                    "type": "WINDOW_OPENED",
                    "hwnd": hwnd,
                    "process": w.process,
                    "message": format!("New window opened: \"{}\" (PID {})", w.title, w.pid)
                });
                if socket.send(Message::Text(ev.to_string())).await.is_err() {
                    return;
                }
            }
        }

        for (&hwnd, old_w) in &prev_windows {
            if !curr_map.contains_key(&hwnd) {
                let ev = json!({
                    "timestamp": ts,
                    "type": "WINDOW_CLOSED",
                    "hwnd": hwnd,
                    "process": old_w.process,
                    "message": format!("Window closed: \"{}\" ({})", old_w.title, old_w.process)
                });
                if socket.send(Message::Text(ev.to_string())).await.is_err() {
                    return;
                }
            }
        }

        prev_windows = curr_map;
    }
}

const INDEX_HTML: &str = include_str!("../index.html");

async fn serve_index() -> Html<String> {
    if let Ok(disk_html) = std::fs::read_to_string("index.html") {
        Html(disk_html)
    } else {
        Html(INDEX_HTML.to_string())
    }
}

fn run_mcp_stdio_loop() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    let tools = json!([
        {
            "name": "observe_desktop",
            "description": "Native Rust semantic GUI-SSH observer. Extracts OS accessibility hierarchy and unrendered off-screen document text at 5 verbosity levels.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "Window HWND, PID, process name, or title substring. Use 'all' for a desktop-wide overview.",
                        "default": "all"
                    },
                    "verbosity": {
                        "type": "string",
                        "enum": ["glance", "text_only", "interactive", "compact", "full"],
                        "description": "Token budget / detail level (glance, text_only, interactive, compact, full)",
                        "default": "compact"
                    }
                },
                "required": ["target", "verbosity"]
            }
        }
    ]);

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            _ => continue,
        };
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": { "code": -32700, "message": e.to_string() }
                });
                let _ = writeln!(stdout, "{}", err);
                let _ = stdout.flush();
                continue;
            }
        };

        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let req_id = req.get("id").cloned();

        let resp = match method {
            "initialize" => Some(json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "axiaccess-rust", "version": "2.0.0" }
                }
            })),
            "tools/list" => Some(json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "result": { "tools": tools }
            })),
            "tools/call" => {
                let params = req.get("params").cloned().unwrap_or_else(|| json!({}));
                let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                let target = args
                    .get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or("all");
                let verbosity = args
                    .get("verbosity")
                    .and_then(|v| v.as_str())
                    .unwrap_or("compact");

                match engine::format_for_llm(target, verbosity) {
                    Ok(res) => Some(json!({
                        "jsonrpc": "2.0",
                        "id": req_id,
                        "result": {
                            "content": [{ "type": "text", "text": res.llm_text }],
                            "isError": false
                        }
                    })),
                    Err(e) => Some(json!({
                        "jsonrpc": "2.0",
                        "id": req_id,
                        "result": {
                            "content": [{ "type": "text", "text": format!("Error: {}", e) }],
                            "isError": true
                        }
                    })),
                }
            }
            _ => req_id.map(|id| {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32601, "message": format!("Method '{}' not found", method) }
                })
            }),
        };

        if let Some(r) = resp {
            let _ = writeln!(stdout, "{}", r);
            let _ = stdout.flush();
        }
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if cli.mcp || matches!(cli.command, Some(Commands::Mcp)) {
        run_mcp_stdio_loop();
        return;
    }

    if let Some(target) = cli.cli {
        match engine::format_for_llm(&target, &cli.verbosity) {
            Ok(res) => {
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&res).unwrap());
                } else {
                    println!("{}", res.llm_text);
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    match cli.command {
        Some(Commands::Query {
            target,
            verbosity,
            json,
        }) => {
            match engine::format_for_llm(&target, &verbosity) {
                Ok(res) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&res).unwrap());
                    } else {
                        println!("{}", res.llm_text);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
            return;
        }
        Some(Commands::Reader {
            target,
            ssh,
            port,
            identity,
            url,
            format,
            diff,
        }) => {
            let mut args = vec!["axiaccess_reader.py".to_string(), target, "-f".to_string(), format];
            if let Some(s) = ssh {
                args.push("--ssh".to_string());
                args.push(s);
            }
            if let Some(p) = port {
                args.push("-p".to_string());
                args.push(p.to_string());
            }
            if let Some(i) = identity {
                args.push("-i".to_string());
                args.push(i);
            }
            if let Some(u) = url {
                args.push("--url".to_string());
                args.push(u);
            }
            if diff {
                args.push("--diff".to_string());
            }

            let status = Command::new("python")
                .args(&args)
                .status()
                .or_else(|_| Command::new("python3").args(&args).status());

            match status {
                Ok(s) => {
                    if !s.success() {
                        std::process::exit(s.code().unwrap_or(1));
                    }
                }
                Err(e) => {
                    eprintln!("Failed to launch axiaccess_reader.py: {}", e);
                    std::process::exit(1);
                }
            }
            return;
        }
        Some(Commands::Mcp) => {
            run_mcp_stdio_loop();
            return;
        }
        Some(Commands::Serve { port }) => {
            start_http_server(port).await;
        }
        None => {
            start_http_server(cli.port).await;
        }
    }
}

async fn start_http_server(port: u16) {
    let app = Router::new()
        .route("/", get(serve_index))
        .route("/api/windows", get(api_windows))
        .route("/api/llm", get(api_llm))
        .route("/api/inspect/:target_id", get(api_inspect))
        .route("/api/wakeup", post(api_wakeup))
        .route("/api/spawn-test-pad", post(api_spawn_test_pad))
        .route("/api/architecture", get(api_architecture))
        .route("/ws/events", get(ws_events));

    let addr = format!("127.0.0.1:{}", port);
    println!("🚀 AxiAccess Native Rust Server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind port");
    axum::serve(listener, app).await.unwrap();
}
