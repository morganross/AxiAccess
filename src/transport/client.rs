use crate::model::{AccessError, Request, Response, MAX_FRAME};
use serde_json::Value;
use std::time::Duration;

/// Deliberate client configuration, never an agent-provided endpoint. The pinned
/// leaf certificate is the only trusted root; normal hostname validation remains.
pub async fn remote(
    url: &str,
    cert_pem: &[u8],
    client_id: &str,
    token: &str,
    operation: &str,
    params: Value,
) -> Result<Value, AccessError> {
    remote_scoped(
        url,
        cert_pem,
        client_id,
        token,
        &crate::model::opaque_id(),
        operation,
        params,
    )
    .await
}
pub async fn remote_scoped(
    url: &str,
    cert_pem: &[u8],
    client_id: &str,
    token: &str,
    scope: &str,
    operation: &str,
    params: Value,
) -> Result<Value, AccessError> {
    let mut endpoint = reqwest::Url::parse(url)
        .map_err(|e| AccessError::new("invalid_endpoint", e.to_string()))?;
    if endpoint.scheme() != "https"
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(AccessError::new(
            "invalid_endpoint",
            "Use a configured HTTPS sender address without credentials/query/fragment",
        ));
    }
    let address_host = endpoint
        .host_str()
        .ok_or_else(|| AccessError::new("invalid_endpoint", "Missing sender host"))?
        .to_owned();
    let address_port = endpoint.port_or_known_default().unwrap_or(443);
    let addresses = tokio::net::lookup_host((address_host.as_str(), address_port))
        .await
        .map_err(|e| AccessError::new("transport", e.to_string()))?
        .collect::<Vec<_>>();
    // The paired certificate's fixed name represents the sender independently
    // of a LAN/VM address. Preserve hostname+signature validation against that
    // name; only the TCP routing address comes from the configured endpoint.
    endpoint
        .set_host(Some("axiaccess.local"))
        .map_err(|_| AccessError::new("invalid_endpoint", "Invalid sender endpoint"))?;
    endpoint
        .set_port(Some(address_port))
        .map_err(|_| AccessError::new("invalid_endpoint", "Invalid sender port"))?;
    endpoint.set_path("/v1/read");
    let certificate = reqwest::Certificate::from_pem(cert_pem)
        .map_err(|e| AccessError::new("trust", e.to_string()))?;
    let client = reqwest::Client::builder()
        .tls_built_in_root_certs(false)
        .add_root_certificate(certificate)
        .resolve_to_addrs("axiaccess.local", &addresses)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(7))
        .build()
        .map_err(|e| AccessError::new("transport", e.to_string()))?;
    let request = Request {
        id: crate::model::opaque_id(),
        client_scope: Some(scope.into()),
        token: None,
        operation: operation.into(),
        params,
    };
    let mut response = client
        .post(endpoint)
        .header("X-AxiAccess-Client", client_id)
        .bearer_auth(token)
        .json(&request)
        .send()
        .await
        .map_err(|e| AccessError::new("transport", e.to_string()))?;
    if response.status()==reqwest::StatusCode::UNAUTHORIZED{return Err(AccessError::new("unauthorized","Sender pairing credential is not authorized"));}
    if response
        .content_length()
        .is_some_and(|n| n > MAX_FRAME as u64)
    {
        return Err(AccessError::new(
            "output_limit",
            "Remote sender response exceeds limit",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| AccessError::new("transport", e.to_string()))?
    {
        if bytes.len() + chunk.len() > MAX_FRAME {
            return Err(AccessError::new(
                "output_limit",
                "Remote sender response exceeds limit",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let envelope: Response = serde_json::from_slice(&bytes)
        .map_err(|_| AccessError::new("protocol", "Remote sender returned malformed JSON"))?;
    if envelope.id != request.id {
        return Err(AccessError::new(
            "protocol",
            "Remote sender response ID mismatch",
        ));
    }
    envelope.result.ok_or_else(|| {
        envelope
            .error
            .unwrap_or_else(|| AccessError::new("protocol", "Missing remote result"))
    })
}

/// Operator-only SSH forwarding to an existing graphical-session sender. It
/// never launches arbitrary remote code and never changes SSH credentials.
pub async fn ssh_tunnel(
    host: &str,
    port: Option<u16>,
    identity: Option<&std::path::Path>,
    remote_port: u16,
) -> Result<(tokio::process::Child, String), AccessError> {
    use std::process::Stdio;
    if host.is_empty()
        || host.len() > 512
        || host.starts_with('-')
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@._-:[]".contains(&b))
    {
        return Err(AccessError::new(
            "invalid_endpoint",
            "Invalid configured SSH destination",
        ));
    }
    let reservation = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|e| AccessError::new("transport", e.to_string()))?;
    let local_port = reservation
        .local_addr()
        .map_err(|e| AccessError::new("transport", e.to_string()))?
        .port();
    drop(reservation);
    let mut command = tokio::process::Command::new("ssh");
    command.args([
        "-N",
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "StrictHostKeyChecking=yes",
        "-o",
        "ExitOnForwardFailure=yes",
        "-L",
        &format!("127.0.0.1:{local_port}:127.0.0.1:{remote_port}"),
    ]);
    if let Some(port) = port {
        command.args(["-p", &port.to_string()]);
    }
    if let Some(identity) = identity {
        command.arg("-i").arg(identity);
    }
    command
        .arg(host)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command
        .spawn()
        .map_err(|e| AccessError::new("transport", e.to_string()))?;
    let start = tokio::time::Instant::now();
    loop {
        if let Some(code) = child
            .try_wait()
            .map_err(|e| AccessError::new("transport", e.to_string()))?
        {
            return Err(AccessError::new(
                "transport",
                format!(
                    "SSH tunnel exited {code}; verify existing host trust and user credentials"
                ),
            ));
        }
        if tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, local_port))
            .await
            .is_ok()
        {
            break;
        }
        if start.elapsed() > Duration::from_secs(8) {
            let _ = child.kill().await;
            return Err(AccessError::new(
                "timeout",
                "SSH sender tunnel did not become ready",
            ));
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Ok((child, format!("https://127.0.0.1:{local_port}")))
}
