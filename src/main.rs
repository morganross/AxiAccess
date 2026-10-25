use axiaccess::{model::*, observe::Observer, reader, transport};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use std::{net::SocketAddr, path::PathBuf};

#[derive(Parser)]
#[command(
    name = "axiaccess",
    version,
    about = "Native desktop accessibility observer, TLS/SSH sender/reader, MCP server, and low-level programmatic control toolkit"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    #[arg(long)]
    cli: Option<String>,
    #[arg(short, long, default_value = "compact")]
    verbosity: String,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    mcp: bool,
    #[arg(short, long)]
    port: Option<u16>,
}
#[derive(Subcommand)]
enum Commands {
    /// Initialize persistent sender identity and private pairing credentials.
    Setup {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        quiet: bool,
    },
    /// Display public sender identity without captured application text or secrets.
    Status {
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Persist the selected local/LAN/VM listening address for user-session startup.
    Configure{#[arg(long)]config:Option<PathBuf>,#[arg(long)]listen:SocketAddr},
    /// Issue/revoke an AxiAccess client credential.
    Pair {
        client_id: String,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        revoke: bool,
    },
    /// Observation-only sender. Private stdio or authenticated TLS, never plaintext.
    Sender {
        #[arg(long)]
        stdio: bool,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        listen: Option<SocketAddr>,
    },
    /// Compatibility name for authenticated TLS sender (run setup first).
    Serve {
        #[arg(short, long)]
        port: Option<u16>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Standards-based read-only MCP stdio server in the current user's session.
    Mcp,
    Query {
        #[arg(default_value = "all")]
        target: String,
        #[arg(short, long, default_value = "glance")]
        verbosity: String,
        #[arg(long)]
        json: bool,
    },
    /// Native structured reader, with deliberately configured optional TLS remote.
    Reader {
        #[arg(default_value = "all")]
        target: String,
        #[arg(short, long, default_value = "xml")]
        format: String,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        certificate: Option<PathBuf>,
        #[arg(long, default_value = "owner")]
        client_id: String,
        #[arg(long, default_value = "AXIACCESS_TOKEN")]
        token_env: String,
        #[arg(long)]
        ssh: Option<String>,
        #[arg(short, long)]
        port: Option<u16>,
        #[arg(short, long)]
        identity: Option<PathBuf>,
        #[arg(long, default_value_t = 8765)]
        sender_port: u16,
        #[arg(long)]
        diff: bool,
        #[arg(long)]
        baseline: Option<String>,
        #[arg(long)]
        scope: Option<String>,
    },
    #[command(hide = true, name = "__worker")]
    Worker,
    /// Focus-free background window & control manipulation (UIA Patterns + Win32 SendMessage/PostMessage)
    Control {
        /// Target window (hwnd, pid, process, title, or "focused")
        target: String,
        /// Action: invoke | set_value | send_keys | click_xy | toggle | select | expand | collapse | scroll | menu | window_state
        action: String,
        /// Element selector (@e5, element name substring, or menu item name)
        #[arg(short, long)]
        selector: Option<String>,
        /// Role filter (button, edit, menuitem, checkbox, etc.)
        #[arg(short, long)]
        role: Option<String>,
        /// Value / text / keys payload for set_value, send_keys, menu, or window_state
        #[arg(short = 'V', long)]
        value: Option<String>,
        /// Client X coordinate (for click_xy) or horizontal scroll delta
        #[arg(short, long)]
        x: Option<i32>,
        /// Client Y coordinate (for click_xy) or vertical scroll delta
        #[arg(short, long)]
        y: Option<i32>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Occluded DWM window capture (PrintWindow PW_RENDERFULLCONTENT) + hardware Windows.Media.Ocr
    Ocr {
        /// Target window (hwnd, pid, process, title, or "focused")
        #[arg(default_value = "focused")]
        target: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Kernel Wait Chain Traversal (WCT), deadlock/hang diagnostics, child tree, I/O & TCP sockets
    Telemetry {
        /// Target window (hwnd, pid, process, title, or "focused")
        #[arg(default_value = "focused")]
        target: String,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Discover live app debug & IPC channels (CDP WebSocket ports, .NET diagnostic pipes, COM ROT)
    DebugChannels {
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Direct Win32 Console Screen Buffer reader & headless keystroke injector (ReadConsoleOutputCharacterW / WriteConsoleInputW)
    Console {
        /// Target console window or PID (cmd, powershell, python, node, cargo, or "focused")
        #[arg(default_value = "focused")]
        target: String,
        /// Optional input string to inject into the console input buffer (WriteConsoleInputW)
        #[arg(short, long)]
        write: Option<String>,
        /// Append Enter keystroke when writing
        #[arg(long, default_value_t = true)]
        enter: bool,
        /// Maximum console lines to read
        #[arg(short, long, default_value_t = 500)]
        max_lines: usize,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Isolated Kernel Sandbox Desktop manager (CreateDesktopW / OpenDesktopW / EnumDesktopsW / CreateProcessW)
    Sandbox {
        /// Action: list | create | spawn
        #[arg(default_value = "list")]
        action: String,
        /// Sandbox desktop name (defaults to "AxiAccessSandbox")
        #[arg(short, long)]
        name: Option<String>,
        /// Command line to spawn on the sandbox desktop (when action = "spawn")
        #[arg(short, long)]
        command: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },
}
async fn query(
    target: String,
    verbosity: String,
    as_json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let observer = Observer::new(opaque_id());
    let result = axiaccess::mcp::compatibility(
        &observer,
        "cli",
        json!({"target":target,"verbosity":verbosity}),
    )
    .await?;
    println!(
        "{}",
        if as_json {
            reader::render(&result, "json")?
        } else {
            reader::render(&result, "markdown")?
        }
    );
    Ok(())
}
#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("AxiAccess: {error}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    if cli.mcp {
        return axiaccess::mcp::serve().await;
    }
    if let Some(target) = cli.cli {
        return query(target, cli.verbosity, cli.json).await;
    }
    match cli.command {
        Some(Commands::Worker) => axiaccess::observe::worker_loop(),
        Some(Commands::Setup { config, quiet }) => {
            let value = transport::config::setup(
                &config.unwrap_or_else(transport::config::default_path),
                quiet,
            )?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        Some(Commands::Status { config }) => {
            let path = config.unwrap_or_else(transport::config::default_path);
            let c = transport::config::read(&path)?;
            println!(
                "{}",
                json!({"configured":true,"configPath":path,"senderId":c.sender_id,"version":env!("CARGO_PKG_VERSION"),"certificateSha256":c.certificate_sha256,"listen":c.listen,"pairedClients":c.clients.keys().collect::<Vec<_>>(),"note":"Configuration status does not claim a running sender or accessibility permission"})
            );
            Ok(())
        }
        Some(Commands::Configure{config,listen})=>{println!("{}",transport::config::configure_listen(&config.unwrap_or_else(transport::config::default_path),listen)?);Ok(())},
        Some(Commands::Pair {
            client_id,
            config,
            revoke,
        }) => {
            println!(
                "{}",
                transport::config::edit_client(
                    &config.unwrap_or_else(transport::config::default_path),
                    &client_id,
                    revoke
                )?
            );
            Ok(())
        }
        Some(Commands::Sender {
            stdio,
            config,
            listen,
        }) => {
            if stdio {
                transport::sender::stdio().await
            } else {
                let path=config.unwrap_or_else(transport::config::default_path);let address=match listen{Some(value)=>value,None=>transport::config::read(&path)?.listen};transport::sender::serve(&path,address).await
            }
        }
        Some(Commands::Serve { port, config }) => {let path=config.unwrap_or_else(transport::config::default_path);let address=port.map(|port|SocketAddr::from(([127,0,0,1],port))).unwrap_or(transport::config::read(&path)?.listen);transport::sender::serve(&path,address).await},
        Some(Commands::Mcp) => axiaccess::mcp::serve().await,
        Some(Commands::Query {
            target,
            verbosity,
            json,
        }) => query(target, verbosity, json).await,
        Some(Commands::Reader {
            target,
            format,
            url,
            certificate,
            client_id,
            token_env,
            ssh,
            port,
            identity,
            sender_port,
            diff,
            baseline,
            scope,
        }) => {
            if ssh.is_some() && url.is_some() {
                return Err("Choose --ssh or --url, not both".into());
            }
            if baseline.is_some() && scope.is_none() {
                return Err("Resuming a reader baseline requires the same explicit --scope".into());
            }
            let (_tunnel, url) = if let Some(host) = ssh {
                let (child, url) =
                    transport::client::ssh_tunnel(&host, port, identity.as_deref(), sender_port)
                        .await?;
                (Some(child), Some(url))
            } else {
                (None, url)
            };
            let scope = scope.unwrap_or_else(opaque_id);
            let value: Value = if let Some(url) = url {
                let cert =
                    std::fs::read(certificate.ok_or(
                        "Remote reader requires --certificate PEM path from sender pairing",
                    )?)?;
                let token = std::env::var(token_env)
                    .map_err(|_| "Remote reader token environment is absent")?;
                let inventory = transport::client::remote_scoped(
                    &url,
                    &cert,
                    &client_id,
                    &token,
                    &scope,
                    "windows",
                    json!({"maxWindows":250}),
                )
                .await?;
                if target == "all" || target == "desktop" {
                    inventory
                } else {
                    let reference = select_window(&inventory, &target)?;
                    let operation = if diff { "changes" } else { "inspect" };
                    let params = if diff {
                        json!({"windowRef":reference,"detail":"compact","baseline":baseline})
                    } else {
                        json!({"windowRef":reference,"detail":"compact"})
                    };
                    let mut value = transport::client::remote_scoped(
                        &url, &cert, &client_id, &token, &scope, operation, params,
                    )
                    .await?;
                    if !diff {
                        let buffers = value["textBuffers"].as_array().cloned().unwrap_or_default();
                        let mut pages = Vec::new();
                        let mut total = 0;
                        for buffer in buffers {
                            let mut cursor: Option<String> = None;
                            loop {
                                let page=transport::client::remote_scoped(&url,&cert,&client_id,&token,&scope,"read_text",json!({"observationId":value["observationId"],"bufferRef":buffer["bufferRef"],"cursor":cursor})).await?;
                                total += page["text"].as_str().map_or(0, str::len);
                                cursor = page["nextCursor"].as_str().map(str::to_owned);
                                pages.push(page);
                                if cursor.is_none() || total >= MAX_CAPTURE_BYTES {
                                    break;
                                }
                            }
                            if total >= MAX_CAPTURE_BYTES {
                                value["readerTextTruncated"] = json!(true);
                                break;
                            }
                        }
                        value["textPages"] = json!(pages);
                    }
                    value
                }
            } else {
                let observer = Observer::new(opaque_id());
                if diff {
                    let inventory = observer
                        .execute(&scope, "windows", json!({"maxWindows":250}))
                        .await?;
                    let reference = select_window(&inventory, &target)?;
                    observer
                        .execute(
                            &scope,
                            "changes",
                            json!({"windowRef":reference,"detail":"compact","baseline":baseline}),
                        )
                        .await?
                } else {
                    axiaccess::mcp::compatibility(
                        &observer,
                        &scope,
                        json!({"target":target,"verbosity":"full"}),
                    )
                    .await?
                }
            };
            let format = if format == "diff" { "json" } else { &format };
            println!("{}", reader::render(&value, format)?);
            Ok(())
        }
        Some(Commands::Control {
            target,
            action,
            selector,
            role,
            value,
            x,
            y,
            json: as_json,
        }) => {
            let req = axiaccess::lowlevel::ControlActionRequest {
                target,
                action,
                selector,
                role,
                value,
                x,
                y,
            };
            let res = axiaccess::lowlevel::control_window(req)?;
            if as_json || cli.json {
                println!("{}", serde_json::to_string_pretty(&res)?);
            } else {
                println!(
                    "[OK={} | {:.1}ms] {} on \"{}\" ({}, hwnd:{}) via {}\n  Details: {}",
                    res.success,
                    res.execution_time_ms,
                    res.action,
                    res.target_title,
                    res.target_process,
                    res.target_hwnd,
                    res.method_used,
                    res.details
                );
            }
            Ok(())
        }
        Some(Commands::Ocr {
            target,
            json: as_json,
        }) => {
            let res = axiaccess::lowlevel::ocr_window(&target)?;
            if as_json || cli.json {
                println!("{}", serde_json::to_string_pretty(&res)?);
            } else {
                println!(
                    "# Occluded DWM + Hardware OCR — \"{}\" ({}, pid:{}, hwnd:{})\n# Capture: {} | {}x{} px | {} lines, {} words, {} chars | {:.1}ms\n\n{}",
                    res.title,
                    res.process,
                    res.pid,
                    res.hwnd,
                    res.capture_method,
                    res.window_width,
                    res.window_height,
                    res.line_count,
                    res.word_count,
                    res.char_count,
                    res.scan_time_ms,
                    res.full_text
                );
            }
            Ok(())
        }
        Some(Commands::Telemetry {
            target,
            json: as_json,
        }) => {
            let res = axiaccess::lowlevel::inspect_telemetry(&target)?;
            if as_json || cli.json {
                println!("{}", serde_json::to_string_pretty(&res)?);
            } else {
                println!(
                    "# Process Telemetry & Wait Chain — \"{}\" ({}, pid:{}, hwnd:{}, ui_tid:{})\n- Status: {} (hung={}, deadlocked={})\n- Memory: {:.1} MB working set, {:.1} MB private | Handles: {} (GDI: {}, USER: {})\n- I/O: {} read ops ({} B), {} write ops ({} B)\n- Children: {} | Active TCP Sockets: {} | Menu Items: {} | Scanned in {:.1}ms",
                    res.title,
                    res.process,
                    res.pid,
                    res.hwnd,
                    res.ui_thread_id,
                    res.wait_chain_status,
                    res.is_hung_window,
                    res.is_deadlocked,
                    res.working_set_mb,
                    res.private_bytes_mb,
                    res.handle_count,
                    res.gdi_objects,
                    res.user_objects,
                    res.io_read_ops,
                    res.io_read_bytes,
                    res.io_write_ops,
                    res.io_write_bytes,
                    res.child_processes.len(),
                    res.tcp_connections.len(),
                    res.menu_items.len(),
                    res.scan_time_ms
                );
            }
            Ok(())
        }
        Some(Commands::DebugChannels { json: as_json }) => {
            let res = axiaccess::lowlevel::discover_debug_channels();
            if as_json || cli.json {
                println!("{}", serde_json::to_string_pretty(&res)?);
            } else {
                println!(
                    "# Live Debug & IPC Channels (scanned in {:.1}ms)\n- CDP Targets: {}\n- .NET Diagnostic Pipes: {}\n- COM Running Objects: {}",
                    res.scan_time_ms,
                    res.cdp_targets.len(),
                    res.dotnet_diagnostic_pipes.len(),
                    res.com_running_objects.len()
                );
                for c in &res.cdp_targets {
                    println!(
                        "  * [CDP :{}] {} (pid:{}) \"{}\" -> {}",
                        c.port, c.process, c.pid, c.title, c.websocket_debugger_url
                    );
                }
                for p in &res.dotnet_diagnostic_pipes {
                    println!("  * [.NET Pipe] {} (pid:{}) -> {}", p.process, p.pid, p.pipe_path);
                }
                for r in &res.com_running_objects {
                    println!("  * [COM ROT: {}] {}", r.category, r.display_name);
                }
            }
            Ok(())
        }
        Some(Commands::Console {
            target,
            write,
            enter,
            max_lines: _,
            json: as_json,
        }) => {
            if let Some(input_text) = write {
                let res = axiaccess::lowlevel::write_console_input(&target, &input_text, enter)?;
                if as_json || cli.json {
                    println!("{}", serde_json::to_string_pretty(&res)?);
                } else {
                    println!("{}", res);
                }
            } else {
                let res = axiaccess::lowlevel::read_console_buffer(&target)?;
                if as_json || cli.json {
                    println!("{}", serde_json::to_string_pretty(&res)?);
                } else {
                    println!(
                        "# Console Buffer — {} (pid:{}) | Grid: {}x{} | Cursor: ({},{}) | {:.1}ms\n",
                        res.process,
                        res.pid,
                        res.buffer_width,
                        res.buffer_height,
                        res.cursor_x,
                        res.cursor_y,
                        res.scan_time_ms
                    );
                    println!("{}", res.text);
                }
            }
            Ok(())
        }
        Some(Commands::Sandbox {
            action,
            name,
            command,
            json: as_json,
        }) => {
            let desk = name.as_deref().unwrap_or("AxiAccessSandbox");
            let res =
                axiaccess::lowlevel::manage_sandbox_desktop(&action, desk, command.as_deref())?;
            if as_json || cli.json {
                println!("{}", serde_json::to_string_pretty(&res)?);
            } else {
                println!(
                    "[Sandbox '{}' | {}] {}\n  Available Desktops: {:?}\n  Sandbox Windows: {:?}",
                    res.desktop_name,
                    res.action,
                    res.status,
                    res.available_desktops,
                    res.sandbox_windows
                );
            }
            Ok(())
        }
        None => {let path=transport::config::default_path();let address=cli.port.map(|port|SocketAddr::from(([127,0,0,1],port))).unwrap_or(transport::config::read(&path)?.listen);transport::sender::serve(&path,address).await},
    }
}

fn select_window(inventory: &Value, target: &str) -> Result<String, Box<dyn std::error::Error>> {
    let target_lower = target.to_lowercase();
    let matches: Vec<_> = inventory["windows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|w| {
            w["windowRef"] == target
                || w["native"]["native_id"] == target
                || w["native"]["hwnd"].to_string() == target
                || w["native"]["pid"].to_string() == target
                || w["native"]["title"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&target_lower)
                || w["native"]["process"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&target_lower)
        })
        .collect();
    if matches.len() != 1 {
        return Err(format!(
            "Reader target matched{} windows; select one exact windowRef",
            matches.len()
        )
        .into());
    }
    Ok(matches[0]["windowRef"]
        .as_str()
        .ok_or("Missing windowRef")?
        .into())
}
