use crate::{
    model::*,
    observe::{bounded_line, Observer},
    transport::config::{self, Config},
};
use axum::{
    extract::{DefaultBodyLimit, State, Request as HttpRequest},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use std::{net::SocketAddr, path::Path, sync::Arc};
use tokio::io::{AsyncWriteExt, BufReader};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Bootstrap {
    r#type: String,
    protocol_version: u32,
    token: String,
    client_id: String,
}
pub async fn stdio() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = BufReader::new(tokio::io::stdin());
    let mut output = tokio::io::stdout();
    let first = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        bounded_line(&mut input, MAX_FRAME),
    )
    .await??
    .ok_or("Bootstrap required")?;
    let b: Bootstrap = serde_json::from_slice(&first)?;
    if b.r#type != "bootstrap"
        || b.protocol_version != PROTOCOL_VERSION
        || b.token.len() < 48
        || b.token.len() > 256
        || b.client_id.is_empty()
        || b.client_id.len() > 128
    {
        return Err("Invalid private sender bootstrap".into());
    }
    let observer = Observer::new(opaque_id());
    write_json(&mut output, &observer.ready().await).await?;
    while let Some(line) = bounded_line(&mut input, MAX_FRAME).await? {
        let response = match serde_json::from_slice::<Request>(&line) {
            Ok(request) => {
                let result = if request
                    .token
                    .as_ref()
                    .is_none_or(|token| !secure_equal(token, &b.token))
                {
                    Err(AccessError::new(
                        "unauthorized",
                        "Sender capability is invalid",
                    ))
                } else {
                    dispatch(&observer, &b.client_id, &request).await
                };
                Response::from_result(request.id, result)
            }
            Err(_) => Response::from_result(
                String::new(),
                Err(AccessError::new(
                    "invalid_request",
                    "Malformed bounded sender request",
                )),
            ),
        };
        write_json(&mut output, &response).await?;
    }
    Ok(())
}
async fn write_json<W: tokio::io::AsyncWrite + Unpin, T: serde::Serialize>(
    writer: &mut W,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_FRAME {
        return Err("Sender response exceeds limit".into());
    }
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn dispatch(
    observer: &Observer,
    client: &str,
    r: &Request,
) -> Result<Value, AccessError> {
    if r.id.is_empty() || r.id.len() > 128 || r.operation.len() > 32 {
        return Err(AccessError::new(
            "invalid_request",
            "Request identity/operation exceeds limits",
        ));
    }
    let scope = r.client_scope.as_deref().unwrap_or("default");
    if scope.is_empty() || scope.len() > 128 {
        return Err(AccessError::new(
            "invalid_arguments",
            "clientScope must contain1..128 bytes",
        ));
    }
    let owner = format!("{client}\0{scope}");
    observer
        .execute(&owner, &r.operation, r.params.clone())
        .await
}
#[derive(Clone)]
struct HttpState {
    observer: Arc<Observer>,
    config: Arc<Config>,
    config_path: std::path::PathBuf,
    requests: Arc<tokio::sync::Semaphore>,
}
async fn read(
    State(state): State<HttpState>,
    incoming: HttpRequest,
) -> impl IntoResponse {
    let response_error=|status,error| (status,Json(Response::from_result(String::new(),Err(error))));
    let Ok(_slot)=state.requests.clone().try_acquire_owned()else{return response_error(StatusCode::SERVICE_UNAVAILABLE,AccessError::new("busy","Sender request limit reached"));};
    let headers=incoming.headers();
    if headers.contains_key("origin"){return response_error(StatusCode::UNAUTHORIZED,AccessError::new("unauthorized","Browser-origin requests are not supported"));}
    if headers.contains_key("transfer-encoding"){return response_error(StatusCode::BAD_REQUEST,AccessError::new("invalid_request","Chunked observation requests are not supported"));}
    if headers.get("content-length").and_then(|v|v.to_str().ok()).and_then(|v|v.parse::<u64>().ok()).is_some_and(|length|length>MAX_FRAME as u64){return response_error(StatusCode::PAYLOAD_TOO_LARGE,AccessError::new("input_limit","Sender request exceeds256KiB"));}
    let client = headers
        .get("x-axiaccess-client")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("").to_owned();
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("").to_owned();
    let path = state.config_path.clone();
    let latest = tokio::task::spawn_blocking(move || config::read(&path).map_err(|_| ())).await;
    let authorized=match latest {
            Ok(Ok(current))
                if current.sender_id == state.config.sender_id
                    && current.certificate_sha256 == state.config.certificate_sha256 =>
            {
                config::authorize(&current,&client,&token)
            }
            _ => Err(AccessError::new(
                "unauthorized",
                "Sender configuration changed or is unavailable",
            )),
    };
    if let Err(error)=authorized{return response_error(StatusCode::UNAUTHORIZED,error);}
    let bytes=match tokio::time::timeout(std::time::Duration::from_secs(3),axum::body::to_bytes(incoming.into_body(),MAX_FRAME)).await{Ok(Ok(bytes))=>bytes,_=>return response_error(StatusCode::BAD_REQUEST,AccessError::new("input_limit","Incomplete, oversized or slow sender request"))};
    let request=match serde_json::from_slice::<Request>(&bytes){Ok(request)=>request,Err(_)=>return response_error(StatusCode::BAD_REQUEST,AccessError::new("invalid_request","Malformed sender request"))};
    let result=dispatch(&state.observer,&client,&request).await;
    (StatusCode::OK, Json(Response::from_result(request.id, result)))
}
pub async fn serve(path: &Path, listen: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let config = config::read(path)?;
    let tls = axum_server::tls_rustls::RustlsConfig::from_pem(
        config.certificate_pem.as_bytes().to_vec(),
        config.private_key_pem.as_bytes().to_vec(),
    )
    .await?;
    let observer = Observer::new(config.sender_id.clone());
    let app = Router::new()
        .route("/v1/read", post(read))
        .layer(DefaultBodyLimit::max(MAX_FRAME))
        .with_state(HttpState {
            observer,
            config: Arc::new(config),
            config_path: path.to_path_buf(),
            requests: Arc::new(tokio::sync::Semaphore::new(64)),
        });
    // No /action, wakeup, test-document, raw file, or arbitrary command routes.
    let handle = axum_server::Handle::new();
    let shutdown = handle.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        shutdown.graceful_shutdown(Some(std::time::Duration::from_secs(3)));
    });
    eprintln!("AxiAccess observation-only TLS sender listening on {listen}");
    axum_server::bind_rustls(listen, tls)
        .handle(handle)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}
