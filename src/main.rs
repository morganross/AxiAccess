use axiaccess::{model::*, observe::Observer, reader, transport};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use std::{net::SocketAddr, path::PathBuf};

#[derive(Parser)]
#[command(
    name = "axiaccess",
    version,
    about = "Read-only native desktop accessibility sender and MCP tools"
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
