use crate::{changes, engine, model::*, text};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{Mutex, Semaphore},
};

pub async fn bounded_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    max: usize,
) -> std::io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    loop {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Incomplete frame",
                ))
            };
        }
        let count = chunk
            .iter()
            .position(|c| *c == b'\n')
            .map_or(chunk.len(), |p| p + 1);
        if bytes.len() + count > max {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Frame exceeds limit",
            ));
        }
        bytes.extend_from_slice(&chunk[..count]);
        reader.consume(count);
        if bytes.last() == Some(&b'\n') {
            return Ok(Some(bytes));
        }
    }
}

struct Worker {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}
#[derive(Default)]
struct NativeWorker {
    worker: Mutex<Option<Worker>>,
    generation: AtomicU64,
}
impl NativeWorker {
    async fn read(&self, operation: &str, params: Value) -> Result<Value, AccessError> {
        let mut locked = self.worker.lock().await;
        let mut worker = if let Some(worker) = locked.take() {
            worker
        } else {
            let exe = std::env::current_exe()
                .map_err(|e| AccessError::new("worker_start", e.to_string()))?;
            let mut command = Command::new(exe);
            command
                .arg("__worker")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            #[cfg(windows)]
            command.creation_flags(0x08000000);
            let mut child = command
                .spawn()
                .map_err(|e| AccessError::new("worker_start", e.to_string()))?;
            let input = child
                .stdin
                .take()
                .ok_or_else(|| AccessError::new("worker_start", "No worker input"))?;
            let output = BufReader::new(
                child
                    .stdout
                    .take()
                    .ok_or_else(|| AccessError::new("worker_start", "No worker output"))?,
            );
            self.generation.fetch_add(1, Ordering::AcqRel);
            Worker {
                child,
                input,
                output,
            }
        };
        // Owning the active child outside the reusable slot makes cancellation
        // kill_on_drop terminate an unfinished read, never reuse unread bytes.
        let id = opaque_id();
        let request = json!({"id":id,"operation":operation,"params":params});
        let result = tokio::time::timeout(Duration::from_secs(3), async {
            let mut bytes = serde_json::to_vec(&request)?;
            bytes.push(b'\n');
            worker.input.write_all(&bytes).await?;
            worker.input.flush().await?;
            let line = bounded_line(&mut worker.output, 8 * 1024 * 1024)
                .await?
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Worker disconnected")
                })?;
            let value: Value = serde_json::from_slice(&line)?;
            if value["id"] != id {
                return Err("Worker response identity mismatch".into());
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(value)
        })
        .await;
        match result {
            Ok(Ok(value)) => {
                *locked = Some(worker);
                if let Some(error) = value.get("error") {
                    Err(AccessError::native(
                        error.as_str().unwrap_or("Native read failed").into(),
                    ))
                } else {
                    Ok(value["result"].clone())
                }
            }
            other => {
                let _ = worker.child.kill().await;
                let _ = worker.child.wait().await;
                Err(match other{Err(_)=>AccessError::new("timeout","Native read exceeded 3 seconds; isolated worker stopped; request a fresh window inventory"),_=>AccessError::new("backend_unavailable","Native worker disconnected or returned an invalid response")})
            }
        }
    }
}

pub fn worker_loop() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{BufRead, Read, Write};
    let mut input = std::io::BufReader::new(std::io::stdin());
    let mut output = std::io::stdout();
    loop {
        let mut bytes = Vec::new();
        let count = input
            .by_ref()
            .take((MAX_FRAME + 1) as u64)
            .read_until(b'\n', &mut bytes)?;
        if count == 0 {
            break;
        }
        if bytes.len() > MAX_FRAME {
            return Err("Worker request too large".into());
        }
        let value: Value = serde_json::from_slice(&bytes)?;
        let id = value["id"].clone();
        let result = (|| -> Result<Value, String> {
            match value["operation"].as_str().unwrap_or("") {
                "windows" => {
                    serde_json::to_value(engine::try_list_windows()?).map_err(|e| e.to_string())
                }
                "inspect" => {
                    let p = &value["params"];
                    let start = engine::native_event_revision();
                    let mut data = serde_json::to_value(engine::inspect_window_with_options(
                        p["target"].as_str().ok_or("Missing native target")?,
                        p["maxDepth"].as_u64().unwrap_or(32) as usize,
                        p["maxNodes"].as_u64().unwrap_or(250) as usize,
                        p["captureText"].as_bool().unwrap_or(true),
                    )?)
                    .map_err(|e| e.to_string())?;
                    let end = engine::native_event_revision();
                    data["_nativeEventRevision"] = json!(end);
                    data["_nativeChangedDuringCapture"] = json!(start != end);
                    Ok(data)
                }
                "events" => {
                    let p = &value["params"];
                    let since = p["since"].as_u64().unwrap_or(0);
                    let wait = Duration::from_millis(p["waitMs"].as_u64().unwrap_or(0).min(250));
                    Ok(
                        json!({"revision":engine::wait_native_events(since,wait),"status":engine::native_event_status()}),
                    )
                }
                "capabilities" => Ok(engine::capabilities()),
                _ => Err("Unknown native observation operation".into()),
            }
        })();
        let value = match result {
            Ok(result) => json!({"id":id,"result":result}),
            Err(error) => json!({"id":id,"error":error}),
        };
        serde_json::to_writer(&mut output, &value)?;
        writeln!(&mut output)?;
        output.flush()?;
    }
    Ok(())
}

#[derive(Clone)]
struct WindowRef {
    native: engine::WindowInfo,
    epoch: String,
}
struct Capture {
    owner: String,
    window_ref: String,
    detail: String,
    epoch: String,
    tree: Vec<engine::TreeNode>,
    buffers: HashMap<String, engine::TextBufferInfo>,
    text_digest: String,
    event_revision: u64,
    bytes: usize,
    created: u64,
    complete: bool,
    native_target: String,
    limits: Limits,
    text_created: Option<u64>,
}
#[derive(Default)]
struct Retained {
    windows: HashMap<String, WindowRef>,
    captures: HashMap<String, Capture>,
    order: VecDeque<String>,
    bytes: usize,
    cursors: HashMap<String, (String, String, String, usize)>,
}

pub struct Observer {
    pub sender_id: String,
    epoch: Mutex<String>,
    native: NativeWorker,
    state: Mutex<Retained>,
    queue: Semaphore,
    active: Semaphore,
}
impl Observer {
    pub fn new(sender_id: String) -> Arc<Self> {
        Arc::new(Self {
            sender_id,
            epoch: Mutex::new(opaque_id()),
            native: NativeWorker::default(),
            state: Mutex::new(Retained::default()),
            queue: Semaphore::new(32),
            active: Semaphore::new(1),
        })
    }
    pub async fn ready(&self) -> Value {
        json!({"type":"ready","protocolVersion":PROTOCOL_VERSION,"senderId":self.sender_id,"epoch":self.epoch.lock().await.clone(),"version":env!("CARGO_PKG_VERSION"),"sourceRevision":option_env!("AXIACCESS_SOURCE_REVISION").unwrap_or("unrecorded"),"platform":std::env::consts::OS,"pid":std::process::id()})
    }
    async fn rotate(&self) {
        *self.epoch.lock().await = opaque_id();
        let mut state = self.state.lock().await;
        state.windows.clear();
        state.cursors.clear();
    }
    async fn native(&self, op: &str, p: Value) -> Result<Value, AccessError> {
        let before = self.native.generation.load(Ordering::Acquire);
        let result = self.native.read(op, p).await;
        let restarted = before != self.native.generation.load(Ordering::Acquire);
        if restarted
            || result
                .as_ref()
                .is_err_and(|e| matches!(e.code.as_str(), "timeout" | "backend_unavailable"))
        {
            self.rotate().await;
        }
        if restarted && before > 0 && op == "inspect" {
            return Err(AccessError::new(
                "stale_reference",
                "Native worker restarted; list windows again",
            ));
        }
        result
    }
    pub async fn execute(
        &self,
        owner: &str,
        operation: &str,
        params: Value,
    ) -> Result<Value, AccessError> {
        let _queued = self
            .queue
            .try_acquire()
            .map_err(|_| AccessError::new("busy", "Observation queue is full"))?;
        if operation == "changes" {
            return self.changes(owner, params).await;
        }
        let _active = tokio::time::timeout(Duration::from_secs(3), self.active.acquire())
            .await
            .map_err(|_| AccessError::new("busy", "Observation worker is busy; retry later"))?
            .map_err(|_| AccessError::new("stopped", "Sender is stopping"))?;
        let value = match operation {
            "capabilities" => {
                let _: Empty = decode(params)?;
                let native = self.native("capabilities", json!({})).await?;
                let mut ready = self.ready().await;
                ready["native"] = native;
                ready["observationOnly"] = json!(true);
                ready["limits"] = json!({"observationTimeoutMs":3000,"maxDepth":64,"maxNodes":1000,"defaultNodes":250,"textPageBytes":MAX_TEXT_PAGE,"responseBytes":MAX_FRAME,"retainedBytes":MAX_RETAINED_BYTES});
                Ok(ready)
            }
            "windows" => self.windows(params).await,
            "inspect" => self.inspect(owner, params, false).await,
            "read_text" => self.read_text(owner, params).await,
            _ => Err(AccessError::new(
                "unknown_operation",
                "Only capabilities, windows, inspect, read_text and changes are supported",
            )),
        }?;
        if serde_json::to_vec(&value)
            .map_err(|e| AccessError::new("serialization", e.to_string()))?
            .len()
            > MAX_FRAME - 1024
        {
            return Err(AccessError::new(
                "output_limit",
                "Result exceeds 256KiB; request fewer nodes or a smaller text page",
            ));
        }
        Ok(value)
    }
    async fn windows(&self, params: Value) -> Result<Value, AccessError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Params {
            #[serde(default)]
            max_windows: Option<usize>,
        }
        let params: Params = decode(params)?;
        let limit = params.max_windows.unwrap_or(100).clamp(1, 250);
        let data: engine::WindowListResponse = decode(self.native("windows", json!({})).await?)?;
        let epoch = self.epoch.lock().await.clone();
        let mut windows = Vec::new();
        let mut refs = HashMap::new();
        for window in data.windows.into_iter().take(250) {
            let native_key = window
                .native_id
                .clone()
                .unwrap_or_else(|| format!("{}:{}", window.pid, window.hwnd));
            let digest = Sha256::digest(format!("{}:{epoch}:{native_key}", self.sender_id));
            let key = format!("w-{:x}", digest);
            if windows.len() < limit {
                windows.push(json!({"windowRef":key,"native":window}));
            }
            refs.insert(
                key,
                WindowRef {
                    native: window,
                    epoch: epoch.clone(),
                },
            );
        }
        self.state.lock().await.windows = refs;
        Ok(
            json!({"senderId":self.sender_id,"epoch":epoch,"observationId":opaque_id(),"observedAtMs":now_ms(),"windows":windows,"complete":!data.truncated&&data.count<=limit,"issues":data.issues,"untrustedAppData":true}),
        )
    }
    async fn inspect(
        &self,
        owner: &str,
        params: Value,
        force_text: bool,
    ) -> Result<Value, AccessError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Params {
            window_ref: String,
            #[serde(default = "compact")]
            detail: String,
            #[serde(default)]
            max_nodes: Option<usize>,
            #[serde(default)]
            max_depth: Option<usize>,
        }
        let p: Params = decode(params)?;
        if !["compact", "full", "interactive", "text_only"].contains(&p.detail.as_str()) {
            return Err(AccessError::new(
                "invalid_arguments",
                "Unsupported observation detail",
            ));
        }
        let limits = Limits {
            max_nodes: p.max_nodes.unwrap_or(250),
            max_depth: p.max_depth.unwrap_or(32),
        }
        .checked()?;
        let reference = self
            .state
            .lock()
            .await
            .windows
            .get(&p.window_ref)
            .cloned()
            .ok_or_else(|| {
                AccessError::new(
                    "stale_reference",
                    "Unknown or stale windowRef; list windows again",
                )
            })?;
        let epoch = self.epoch.lock().await.clone();
        if reference.epoch != epoch {
            return Err(AccessError::new(
                "stale_reference",
                "Desktop worker epoch changed",
            ));
        }
        let native_target=reference.native.native_id.clone().ok_or_else(||AccessError::new("unsupported_identity","Native window lifetime identity is unavailable; refusing an unsafe handle fallback"))?;
        let capture_text = force_text || p.detail == "full" || p.detail == "text_only";
        let raw=self.native("inspect",json!({"target":native_target,"maxNodes":limits.max_nodes,"maxDepth":limits.max_depth,"captureText":capture_text})).await?;
        let event_revision = raw["_nativeEventRevision"].as_u64().unwrap_or(0);
        let during = raw["_nativeChangedDuringCapture"]
            .as_bool()
            .unwrap_or(false);
        let mut data: engine::InspectResponse = decode(raw)?;
        if data.window.pid != reference.native.pid || data.window.hwnd != reference.native.hwnd {
            return Err(AccessError::new(
                "stale_reference",
                "Native identity changed; returned data was discarded",
            ));
        }
        let observation = opaque_id();
        let mut buffers = HashMap::new();
        let mut descriptions = Vec::new();
        let mut budget = MAX_CAPTURE_BYTES;
        let mut complete = !data.stats.truncated;
        let mut issues = data.stats.issues.clone();
        if during {
            issues.push("native_events_during_capture".into());
        }
        for mut b in data.text_buffers.drain(..) {
            if b.protected {
                b.full_text.clear();
                b.visible_text.clear();
                b.preview.clear();
                continue;
            }
            b.truncated |= text::truncate_utf8(&mut b.full_text, budget.min(65_536));
            budget = budget.saturating_sub(b.full_text.len());
            if capture_text {
                complete &= !b.truncated;
            }
            if budget == 0 {
                issues.push("capture_text_budget".into());
                complete = false;
            }
            let id = opaque_id();
            descriptions.push(json!({"bufferRef":id,"name":b.name,"role":b.role,"source":b.source,"totalChars":b.total_chars,"capturedBytes":b.full_text.len(),"truncated":b.truncated,"visibilityKnown":b.visibility_known}));
            buffers.insert(id, b);
        }
        let bytes = serde_json::to_vec(&buffers).map_or(0, |v| v.len())
            + serde_json::to_vec(&data.tree).map_or(0, |v| v.len());
        let text_digest=text_fingerprint(&data.tree,&buffers);
        let tree = if p.detail == "text_only" {
            Vec::new()
        } else {
            data.tree.clone()
        };
        let created = now_ms();
        let result = json!({"senderId":self.sender_id,"epoch":epoch,"observationId":observation,"observedAtMs":created,"windowRef":p.window_ref,"window":data.window,"detail":p.detail,"tree":tree,"textBuffers":descriptions,"complete":complete,"textDeferred":!capture_text,"issues":issues,"eventRevision":event_revision,"limits":limits,"untrustedAppData":true});
        let capture = Capture {
            owner: owner.into(),
            window_ref: p.window_ref,
            detail: p.detail,
            epoch,
            tree: data.tree,
            buffers,
            text_digest,
            event_revision,
            bytes,
            created,
            complete,
            native_target,
            limits,
            text_created: capture_text.then_some(created),
        };
        let mut state = self.state.lock().await;
        state.bytes += bytes;
        state.order.push_back(observation.clone());
        state.captures.insert(observation, capture);
        while state.bytes > MAX_RETAINED_BYTES || state.order.len() > MAX_CAPTURE_COUNT {
            if let Some(old) = state.order.pop_front() {
                if let Some(removed) = state.captures.remove(&old) {
                    state.bytes = state.bytes.saturating_sub(removed.bytes);
                }
                state.cursors.retain(|_, v| v.1 != old);
            } else {
                break;
            }
        }
        Ok(result)
    }
    async fn hydrate_text(
        &self,
        owner: &str,
        observation: &str,
        requested: &str,
    ) -> Result<(), AccessError> {
        let (target, limits, epoch, original, window_ref) = {
            let state = self.state.lock().await;
            let capture = state
                .captures
                .get(observation)
                .filter(|c| c.owner == owner)
                .ok_or_else(|| {
                    AccessError::new("expired_observation", "Observation unavailable")
                })?;
            if !capture.buffers.contains_key(requested) {
                return Err(AccessError::new(
                    "invalid_buffer",
                    "Buffer does not belong to observation",
                ));
            }
            if capture.text_created.is_some() {
                return Ok(());
            }
            (
                capture.native_target.clone(),
                capture.limits.clone(),
                capture.epoch.clone(),
                buffer_native_ids(&capture.tree, &capture.buffers),
                capture.window_ref.clone(),
            )
        };
        let selected = original
            .get(requested)
            .ok_or_else(|| {
                AccessError::new(
                    "resync_required",
                    "Deferred buffer has no native identity; inspect full text explicitly",
                )
            })?
            .clone();
        if epoch != *self.epoch.lock().await {
            return Err(AccessError::new(
                "stale_reference",
                "Worker changed before text was captured",
            ));
        }
        let raw=self.native("inspect",json!({"target":target,"maxNodes":limits.max_nodes,"maxDepth":limits.max_depth,"captureText":true})).await?;
        let data: engine::InspectResponse = decode(raw)?;
        let new_buffers: HashMap<String, engine::TextBufferInfo> = data
            .text_buffers
            .into_iter()
            .filter(|b| !b.protected)
            .map(|b| (b.id.clone(), b))
            .collect();
        let fresh_ids = buffer_native_ids(&data.tree, &new_buffers);
        let keyed: HashMap<_, _> = new_buffers
            .into_iter()
            .filter_map(|(id, buffer)| fresh_ids.get(&id).cloned().map(|native| (native, buffer)))
            .collect();
        if !keyed.contains_key(&selected) {
            return Err(AccessError::new(
                "resync_required",
                "Deferred text source disappeared; capture the window again",
            ));
        }
        let mut state = self.state.lock().await;
        let capture = state
            .captures
            .get_mut(observation)
            .filter(|c| c.owner == owner && c.window_ref == window_ref && c.epoch == epoch)
            .ok_or_else(|| {
                AccessError::new(
                    "expired_observation",
                    "Observation expired during text capture",
                )
            })?;
        let old_bytes = capture.bytes;
        let mut remaining = MAX_CAPTURE_BYTES;
        for (key, old) in capture.buffers.iter_mut() {
            if let Some(native) = original.get(key) {
                if let Some(fresh) = keyed.get(native) {
                    let mut b = fresh.clone();
                    b.truncated |= text::truncate_utf8(&mut b.full_text, remaining.min(65_536));
                    remaining = remaining.saturating_sub(b.full_text.len());
                    *old = b;
                } else {
                    old.full_text.clear();
                    old.truncated = true;
                }
            }
        }
        capture.text_created = Some(now_ms());
        capture.bytes = serde_json::to_vec(&capture.buffers).map_or(0, |v| v.len())
            + serde_json::to_vec(&capture.tree).map_or(0, |v| v.len());
        let new_bytes = capture.bytes;
        state.bytes = state.bytes.saturating_sub(old_bytes) + new_bytes;
        while state.bytes > MAX_RETAINED_BYTES {
            if let Some(old) = state.order.pop_front() {
                if let Some(removed) = state.captures.remove(&old) {
                    state.bytes = state.bytes.saturating_sub(removed.bytes);
                }
                state.cursors.retain(|_, v| v.1 != old);
            } else {
                break;
            }
        }
        Ok(())
    }
    async fn read_text(&self, owner: &str, params: Value) -> Result<Value, AccessError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Params {
            observation_id: String,
            buffer_ref: String,
            #[serde(default)]
            cursor: Option<String>,
            #[serde(default)]
            max_bytes: Option<usize>,
        }
        let p: Params = decode(params)?;
        self.hydrate_text(owner, &p.observation_id, &p.buffer_ref)
            .await?;
        let mut state = self.state.lock().await;
        let offset = if let Some(cursor) = &p.cursor {
            let entry = state
                .cursors
                .get(cursor)
                .ok_or_else(|| AccessError::new("expired_cursor", "Text cursor expired"))?;
            if entry.0 != owner || entry.1 != p.observation_id || entry.2 != p.buffer_ref {
                return Err(AccessError::new(
                    "invalid_cursor",
                    "Text cursor belongs to another observation or client",
                ));
            }
            entry.3
        } else {
            0
        };
        let capture = state
            .captures
            .get(&p.observation_id)
            .filter(|c| c.owner == owner)
            .ok_or_else(|| {
                AccessError::new(
                    "expired_observation",
                    "Observation is unavailable; capture again",
                )
            })?;
        let buffer = capture.buffers.get(&p.buffer_ref).ok_or_else(|| {
            AccessError::new("invalid_buffer", "Buffer does not belong to observation")
        })?;
        let (page, next) = text::page(
            &buffer.full_text,
            offset,
            p.max_bytes.unwrap_or(MAX_TEXT_PAGE),
        )?;
        let page = page.to_string();
        let total = buffer.full_text.len();
        let truncated = buffer.truncated;
        let epoch = capture.epoch.clone();
        let observed = capture.created;
        let text_created = capture.text_created;
        let next_cursor = next.map(|offset| {
            let key = opaque_id();
            state.cursors.insert(
                key.clone(),
                (
                    owner.into(),
                    p.observation_id.clone(),
                    p.buffer_ref.clone(),
                    offset,
                ),
            );
            key
        });
        if state.cursors.len() > 4096 {
            state.cursors.clear();
            return Err(AccessError::new(
                "expired_cursor",
                "Cursor budget reached; begin the retained text again",
            ));
        }
        Ok(
            json!({"senderId":self.sender_id,"epoch":epoch,"observationId":p.observation_id,"bufferRef":p.buffer_ref,"text":page,"range":{"unit":"utf8Bytes","start":offset,"end":offset+page.len()},"capturedBytes":total,"observedAtMs":observed,"textCapturedAtMs":text_created,"nextCursor":next_cursor,"truncated":truncated,"immutable":true,"untrustedAppData":true}),
        )
    }
    async fn changes(&self, owner: &str, params: Value) -> Result<Value, AccessError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Params {
            window_ref: String,
            #[serde(default = "compact")]
            detail: String,
            #[serde(default)]
            baseline: Option<String>,
            #[serde(default)]
            wait_ms: u64,
        }
        let p: Params = decode(params)?;
        if p.wait_ms > 30_000 {
            return Err(AccessError::new(
                "invalid_arguments",
                "waitMs must be0..30000",
            ));
        }
        let prior_revision = {
            let state = self.state.lock().await;
            p.baseline
                .as_ref()
                .and_then(|key| state.captures.get(key))
                .filter(|c| {
                    c.owner == owner && c.window_ref == p.window_ref && c.detail == p.detail
                })
                .map(|c| c.event_revision)
        };
        if let Some(since) = prior_revision.filter(|_| p.wait_ms > 0) {
            let deadline = tokio::time::Instant::now() + Duration::from_millis(p.wait_ms);
            loop {
                let remain = deadline.saturating_duration_since(tokio::time::Instant::now());
                if remain.is_zero() {
                    break;
                }
                let events = self
                    .native(
                        "events",
                        json!({"since":since,"waitMs":remain.as_millis().min(250)}),
                    )
                    .await?;
                if events["revision"].as_u64().unwrap_or(0) != since {
                    break;
                }
                tokio::task::yield_now().await;
            }
        }
        let fresh = self
            .inspect(
                owner,
                json!({"windowRef":p.window_ref,"detail":p.detail}),
                true,
            )
            .await?;
        let id = fresh["observationId"]
            .as_str()
            .expect("observation identity");
        let state = self.state.lock().await;
        let after = state.captures.get(id).ok_or_else(||AccessError::new("expired_observation","Fresh change observation was evicted under concurrent load; capture again"))?;
        let after_map = changes::indexed(&after.tree);
        let before = p
            .baseline
            .as_ref()
            .and_then(|id| state.captures.get(id))
            .filter(|c| {
                c.owner == owner
                    && c.window_ref == after.window_ref
                    && c.detail == after.detail
                    && c.epoch == after.epoch
                    && c.complete
                    && after.complete
            });
        let mut result = if let (Some(before), Some(after_map)) = (before, after_map) {
            if let Some(before_map) = changes::indexed(&before.tree) {
                let mut d = changes::diff(&before_map, &after_map);
                d["resyncRequired"] = json!(false);
                d["textChanged"] = json!(before.text_digest != after.text_digest);
                d
            } else {
                json!({"resyncRequired":true,"reason":"native_identity_unavailable"})
            }
        } else {
            json!({"resyncRequired":true,"reason":"baseline_missing_expired_partial_or_different_scope"})
        };
        result["baseline"] = json!(id);
        result["observation"] = fresh;
        if serde_json::to_vec(&result)
            .map_err(|e| AccessError::new("serialization", e.to_string()))?
            .len()
            > MAX_FRAME - 1024
        {
            return Err(AccessError::new(
                "output_limit",
                "Change result exceeds256KiB; inspect a smaller tree",
            ));
        }
        Ok(result)
    }
}
fn compact() -> String {
    "compact".into()
}
fn text_fingerprint(tree:&[engine::TreeNode],buffers:&HashMap<String,engine::TextBufferInfo>)->String{
    let native_ids=buffer_native_ids(tree,buffers);let mut records:Vec<_>=buffers.iter().map(|(key,b)|format!("{}\0{}\0{}\0{}\0{:x}",native_ids.get(key).map(String::as_str).unwrap_or(&b.id),b.role,b.name,b.source,Sha256::digest(b.full_text.as_bytes()))).collect();records.sort();format!("{:x}",Sha256::digest(records.join("\0").as_bytes()))
}
fn buffer_native_ids(
    tree: &[engine::TreeNode],
    buffers: &HashMap<String, engine::TextBufferInfo>,
) -> HashMap<String, String> {
    let mut ids = HashMap::new();
    let mut stack: Vec<_> = tree.iter().collect();
    while let Some(node) = stack.pop() {
        if let Some(native) = &node.native_id {
            ids.insert(node.id.clone(), native.clone());
        }
        stack.extend(node.children.iter());
    }
    buffers
        .iter()
        .filter_map(|(key, b)| ids.get(&b.id).cloned().map(|native| (key.clone(), native)))
        .collect()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, AccessError> {
    serde_json::from_value(if value.is_null() { json!({}) } else { value })
        .map_err(|e| AccessError::new("invalid_arguments", e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn buffer() -> engine::TextBufferInfo {
        engine::TextBufferInfo {
            id: "node".into(),
            role: "document".into(),
            name: "test".into(),
            source: "fixture".into(),
            total_chars: 3,
            visible_chars: 0,
            offscreen_chars: 0,
            total_lines: 1,
            visible_lines: 0,
            preview: String::new(),
            full_text: "a🦉é".into(),
            visible_text: String::new(),
            truncated: false,
            protected: false,
            visibility_known: false,
        }
    }
    #[tokio::test]
    async fn immutable_pages_are_bound_to_client_and_capture() {
        let observer = Observer::new("sender".into());
        {
            let mut state = observer.state.lock().await;
            state.captures.insert(
                "capture".into(),
                Capture {
                    owner: "alice".into(),
                    window_ref: "w".into(),
                    detail: "full".into(),
                    epoch: "epoch".into(),
                    tree: vec![],
                    buffers: HashMap::from([("buffer".into(), buffer())]),
                    text_digest: String::new(),
                    event_revision: 0,
                    bytes: 100,
                    created: 1,
                    complete: true,
                    native_target: String::new(),
                    limits: Limits::default(),
                    text_created: Some(1),
                },
            );
        }
        assert_eq!(
            observer
                .read_text(
                    "bob",
                    json!({"observationId":"capture","bufferRef":"buffer"})
                )
                .await
                .unwrap_err()
                .code,
            "expired_observation"
        );
        let first = observer
            .read_text(
                "alice",
                json!({"observationId":"capture","bufferRef":"buffer","maxBytes":4}),
            )
            .await
            .unwrap();
        assert_eq!(first["text"], "a");
        let cursor = first["nextCursor"].clone();
        let second=observer.read_text("alice",json!({"observationId":"capture","bufferRef":"buffer","cursor":cursor,"maxBytes":4})).await.unwrap();
        assert_eq!(second["text"], "🦉");
        assert!(observer.read_text("bob",json!({"observationId":"capture","bufferRef":"buffer","cursor":first["nextCursor"]})).await.is_err());
    }
    #[tokio::test]
    async fn framing_rejects_partial_and_oversized() {
        let mut partial = BufReader::new(std::io::Cursor::new(b"{}"));
        assert!(bounded_line(&mut partial, 100).await.is_err());
        let mut large = BufReader::new(std::io::Cursor::new(b"12345\n"));
        assert!(bounded_line(&mut large, 4).await.is_err());
    }
    #[test]
    fn caps_and_request_fields_are_strict() {
        assert!(Limits {
            max_nodes: 1001,
            max_depth: 64
        }
        .checked()
        .is_err());
        assert!(serde_json::from_value::<Request>(
            json!({"id":"x","operation":"click","unexpected":true})
        )
        .is_err());
        assert!(secure_equal("same", "same"));
        assert!(!secure_equal("same", "different"));
    }
    #[test]fn equal_prefix_buffers_cannot_exchange_history_undetected(){
        let node=|id:&str|serde_json::from_value::<engine::TreeNode>(json!({"id":id,"native_id":format!("native-{id}"),"role":"document","name":"same","depth":0,"offscreen":false,"focused":false,"enabled":true,"has_text":true,"text_source":"fixture","total_chars":201,"offscreen_chars":0,"preview":"same","children":[]})).unwrap();
        let tree=vec![node("a"),node("b")];let mut a=buffer();a.id="a".into();a.full_text=format!("{}A","x".repeat(200));let mut b=a.clone();b.id="b".into();b.full_text=format!("{}B","x".repeat(200));let before=HashMap::from([("first".into(),a.clone()),("second".into(),b.clone())]);std::mem::swap(&mut a.full_text,&mut b.full_text);let after=HashMap::from([("first".into(),a),("second".into(),b)]);assert_ne!(text_fingerprint(&tree,&before),text_fingerprint(&tree,&after));
    }
}
