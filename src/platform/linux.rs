//! Read-only native AT-SPI2 client. No Python, shell commands, actions, or global
//! accessibility-setting writes are used by this adapter.
use crate::engine::{
    InspectResponse, InspectStats, InspectWindowMeta, TextBufferInfo, TreeNode, WindowBounds,
    WindowInfo, WindowListResponse,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::json;
use std::collections::HashSet;
use std::future::Future;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Condvar, Mutex, OnceLock,
};
use std::time::{Duration, Instant};
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue, Type},
    Connection,
};

const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const ROOT: &str = "/org/a11y/atspi/accessible/root";
const REGISTRY: &str = "org.a11y.atspi.Registry";
const CALL_LIMIT: Duration = Duration::from_millis(350);
// Leave framing/shutdown margin inside the coordinator's 3-second hard deadline.
const OBSERVATION_LIMIT: Duration = Duration::from_millis(2500);
const MAX_WINDOWS: usize = 256;
const MAX_RAW: usize = 5000;
const MAX_TEXT: usize = 64 * 1024;
// org.a11y.atspi.StateType values, not toolkit-local enumeration values.
const ACTIVE: usize = 1;
const ENABLED: usize = 8;
const FOCUSED: usize = 12;
const ICONIFIED: usize = 15;
const SHOWING: usize = 25;
const VISIBLE: usize = 30;

static EVENT_STARTED: OnceLock<()> = OnceLock::new();
static EVENT_REVISION: AtomicU64 = AtomicU64::new(0);
static EVENT_ACTIVE: AtomicBool = AtomicBool::new(false);
static EVENT_ERROR: Mutex<Option<String>> = Mutex::new(None);
static EVENT_WAIT: (Mutex<()>, Condvar) = (Mutex::new(()), Condvar::new());
static OBSERVED_APP_OWNERS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn remember_app(bus: &str, owner: &str) {
    if let Ok(mut owners)=OBSERVED_APP_OWNERS.get_or_init(||Mutex::new(HashSet::new())).lock() {
        if owners.len()>=512 {owners.clear(); event_changed();}
        owners.insert(format!("{bus}/{owner}"));
    }
}

fn event_changed() {
    let _guard = EVENT_WAIT.0.lock().ok();
    EVENT_REVISION.fetch_add(1, Ordering::AcqRel);
    EVENT_WAIT.1.notify_all();
}

fn start_events() {
    EVENT_STARTED.get_or_init(|| {
        let result = std::thread::Builder::new()
            .name("axiaccess-atspi-events".into())
            .spawn(|| {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        set_event_error(format!("Event runtime: {error}"));
                        return;
                    }
                };
                runtime.block_on(async {
                    let mut backoff = 1u64;
                    loop {
                        EVENT_ACTIVE.store(false, Ordering::Release);
                        // Every disconnection/reconnect invalidates event continuity.
                        event_changed();
                        match event_session().await {
                            Ok(()) => set_event_error("AT-SPI event stream ended".into()),
                            Err(error) => set_event_error(error),
                        }
                        tokio::time::sleep(Duration::from_secs(backoff)).await;
                        backoff = (backoff * 2).min(30);
                    }
                });
            });
        if let Err(error) = result {
            set_event_error(format!("Event worker: {error}"));
        }
    });
}

fn set_event_error(error: String) {
    EVENT_ACTIVE.store(false, Ordering::Release);
    if let Ok(mut stored) = EVENT_ERROR.lock() {
        *stored = Some(bounded(&error, 512));
    }
}

async fn event_session() -> Result<(), String> {
    use futures_util::StreamExt;
    let client = Client::connect().await?;
    // Subscribe before asking applications to produce events. The queue is
    // bounded; no event payload/text is decoded, retained or logged.
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .build();
    let mut stream = tokio::time::timeout(
        CALL_LIMIT,
        zbus::MessageStream::for_match_rule(rule, &client.bus, Some(64)),
    )
    .await
    .map_err(|_| "timeout: AT-SPI event subscription".to_string())?
    .map_err(|e| format!("event_unavailable: {e}"))?;
    for event in [
        "object:children-changed",
        "object:property-change",
        "object:state-changed",
        "object:text-changed",
        "object:visible-data-changed",
        "window",
        "focus",
    ] {
        let _: () = client
            .call(
                REGISTRY,
                "/org/a11y/atspi/registry",
                REGISTRY,
                "RegisterEvent",
                &(event, Vec::<String>::new(), ""),
            )
            .await?;
    }
    EVENT_ACTIVE.store(true, Ordering::Release);
    if let Ok(mut error) = EVENT_ERROR.lock() {
        *error = None;
    }
    event_changed();
    while let Some(message) = stream.next().await {
        let message = message.map_err(|e| format!("event_stream_gap: {e}"))?;
        let header = message.header();
        if header
            .interface()
            .is_some_and(|name| name.as_str().starts_with("org.a11y.atspi.Event."))
        {
            event_changed();
        }
        // Changes in registry/app ownership also invalidate cached identity.
        if header
            .interface()
            .is_some_and(|name| name.as_str() == "org.freedesktop.DBus")
            && header
                .member()
                .is_some_and(|name| name.as_str() == "NameOwnerChanged")
        {
            // Query connections also create/destroy unique bus names. Those
            // are not desktop changes and must not wake every observation.
            if let Ok((name,old,_new))=message.body().deserialize::<(String,String,String)>() {
                if name==REGISTRY {
                    event_changed();
                    return Err("event_stream_gap: accessibility registry ownership changed".into());
                }
                let known=OBSERVED_APP_OWNERS.get_or_init(||Mutex::new(HashSet::new())).lock()
                    .map(|mut owners|owners.remove(&format!("{}/{}",client.bus_id,old))).unwrap_or(false);
                if known {event_changed();}
            }
        }
    }
    Err("event_stream_gap: accessibility bus disconnected".into())
}

pub fn native_event_revision() -> u64 {
    start_events();
    EVENT_REVISION.load(Ordering::Acquire)
}
pub fn wait_native_events(since: u64, timeout: Duration) -> u64 {
    start_events();
    if let Ok(guard) = EVENT_WAIT.0.lock() {
        let _wait = EVENT_WAIT.1.wait_timeout_while(guard, timeout.min(Duration::from_secs(30)), |_| {
            EVENT_REVISION.load(Ordering::Acquire) <= since
        });
    }
    EVENT_REVISION.load(Ordering::Acquire)
}
pub fn native_event_status() -> serde_json::Value {
    start_events();
    json!({"backend":"AT-SPI2 D-Bus event signals","active":EVENT_ACTIVE.load(Ordering::Acquire),"revision":EVENT_REVISION.load(Ordering::Acquire),"queueLimit":64,"payloadRetained":false,"fullDesktopPolling":false,"error":EVENT_ERROR.lock().ok().and_then(|e|e.clone())})
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
struct Object {
    owner: String,
    path: OwnedObjectPath,
}
struct Client {
    bus: Connection,
    bus_id: String,
    deadline: Instant,
    screen_reader: bool,
}
struct NativeWindow {
    object: Object,
    meta: WindowInfo,
}
struct Inventory {
    windows: Vec<NativeWindow>,
    issues: Vec<String>,
    truncated: bool,
}

/// Match libatspi's desktop discovery order. An X11 desktop may advertise a
/// different accessibility bus than the account's default session D-Bus bus.
/// These Xlib calls only read the existing AT_SPI_BUS root-window property.
fn x11_bus_address() -> Result<Option<String>, String> {
    use std::ffi::{c_char, c_int, c_long, c_uchar, c_ulong, c_void, CString};
    if std::env::var_os("WAYLAND_DISPLAY").is_some() { return Ok(None); }
    let display = match std::env::var("AT_SPI_DISPLAY").or_else(|_|std::env::var("DISPLAY")) {
        Ok(value) if !value.is_empty()=>CString::new(value).map_err(|_|"no_graphical_session: invalid X11 display".to_string())?,
        _=>return Ok(None),
    };
    unsafe {
        // Keep the library loaded for the worker lifetime: XInitThreads' global
        // state must not disappear between two independently owned reads.
        static X11_LIBRARY: OnceLock<Result<libloading::Library, String>> = OnceLock::new();
        let library=X11_LIBRARY.get_or_init(||libloading::Library::new("libX11.so.6")
            .map_err(|e|format!("backend_unavailable: native X11 bus discovery: {e}")))
            .as_ref().map_err(Clone::clone)?;
        let init:libloading::Symbol<unsafe extern "C" fn()->c_int>=library.get(b"XInitThreads\0").map_err(|e|e.to_string())?;
        static X11_THREADS: OnceLock<bool> = OnceLock::new();
        if !*X11_THREADS.get_or_init(||init()!=0) {return Err("backend_unavailable: X11 threading initialization failed".into());}
        let open:libloading::Symbol<unsafe extern "C" fn(*const c_char)->*mut c_void>=library.get(b"XOpenDisplay\0").map_err(|e|e.to_string())?;
        let root:libloading::Symbol<unsafe extern "C" fn(*mut c_void)->c_ulong>=library.get(b"XDefaultRootWindow\0").map_err(|e|e.to_string())?;
        let atom:libloading::Symbol<unsafe extern "C" fn(*mut c_void,*const c_char,c_int)->c_ulong>=library.get(b"XInternAtom\0").map_err(|e|e.to_string())?;
        let get:libloading::Symbol<unsafe extern "C" fn(*mut c_void,c_ulong,c_ulong,c_long,c_long,c_int,c_ulong,*mut c_ulong,*mut c_int,*mut c_ulong,*mut c_ulong,*mut *mut c_uchar)->c_int>=library.get(b"XGetWindowProperty\0").map_err(|e|e.to_string())?;
        let free:libloading::Symbol<unsafe extern "C" fn(*mut c_void)->c_int>=library.get(b"XFree\0").map_err(|e|e.to_string())?;
        let close:libloading::Symbol<unsafe extern "C" fn(*mut c_void)->c_int>=library.get(b"XCloseDisplay\0").map_err(|e|e.to_string())?;
        let connection=open(display.as_ptr());
        if connection.is_null() {return Err("no_graphical_session: unable to read the selected X11 desktop's accessibility bus property".into());}
        let property=atom(connection,b"AT_SPI_BUS\0".as_ptr().cast(),1);
        let mut actual_type=0;let mut format=0;let mut count=0;let mut remaining=0;let mut data=std::ptr::null_mut();
        let status=if property!=0 {get(connection,root(connection),property,0,1024,0,31,&mut actual_type,&mut format,&mut count,&mut remaining,&mut data)} else {0};
        let result=if status!=0 {Err("backend_failure: X11 accessibility bus property read failed".into())}
            else if property==0 || actual_type==0 {Ok(None)}
            else if actual_type!=31 || format!=8 || remaining!=0 || count>4096 {Err("backend_failure: invalid or oversized X11 accessibility bus property".into())}
            else if data.is_null() || count==0 {Ok(None)}
            else {std::str::from_utf8(std::slice::from_raw_parts(data,count as usize)).map(|v|Some(v.trim_end_matches('\0').to_owned())).map_err(|_|"backend_failure: invalid accessibility bus address encoding".into())};
        if !data.is_null() {free(data.cast());}
        close(connection);
        result
    }
}

fn run<T: Send + 'static>(
    f: impl FnOnce() -> std::pin::Pin<Box<dyn Future<Output = Result<T, String>> + Send>>
        + Send
        + 'static,
) -> Result<T, String> {
    // Engine calls can originate on a Tokio runtime. A separate native read
    // thread prevents nested-runtime panics and never touches the GUI thread.
    std::thread::Builder::new()
        .name("axiaccess-atspi-read".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("backend_unavailable: {e}"))?;
            rt.block_on(async {
                tokio::time::timeout(OBSERVATION_LIMIT + Duration::from_millis(100), f())
                    .await
                    .map_err(|_| "timeout: AT-SPI observation deadline exceeded".to_string())?
            })
        })
        .map_err(|e| format!("backend_unavailable: {e}"))?
        .join()
        .map_err(|_| "backend_failure: AT-SPI reader terminated".to_string())?
}

impl Client {
    async fn connect() -> Result<Self, String> {
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return Err(
                "no_graphical_session: start AxiAccess in the selected user's graphical session"
                    .into(),
            );
        }
        let deadline = Instant::now() + OBSERVATION_LIMIT;
        let session = tokio::time::timeout(CALL_LIMIT, Connection::session())
            .await
            .map_err(|_| "timeout: session D-Bus connection".to_string())
            .and_then(|v|v.map_err(|e|format!("no_graphical_session: session D-Bus unavailable: {e}")));
        let explicit=std::env::var("AT_SPI_BUS_ADDRESS").ok().filter(|v|!v.is_empty());
        let desktop_address=if explicit.is_some() {explicit} else {x11_bus_address()?};
        let address: String = if let Some(address)=desktop_address {address} else {
            let session=session.as_ref().map_err(Clone::clone)?;
            let reply = tokio::time::timeout(
            CALL_LIMIT,
            session.call_method(
                Some("org.a11y.Bus"),
                "/org/a11y/bus",
                Some("org.a11y.Bus"),
                "GetAddress",
                &(),
            ),
        )
        .await
        .map_err(|_| "timeout: accessibility bus discovery".to_string())?
        .map_err(|e| format!("backend_unavailable: AT-SPI accessibility bus: {e}"))?;
            reply
            .body()
            .deserialize()
            .map_err(|e| format!("backend_failure: invalid accessibility bus address: {e}"))?
        };
        let builder = zbus::connection::Builder::address(address.as_str())
            .map_err(|e| format!("backend_unavailable: {e}"))?;
        let bus = tokio::time::timeout(CALL_LIMIT, builder.build())
            .await
            .map_err(|_| "timeout: accessibility bus connection".to_string())?
            .map_err(|e| format!("backend_unavailable: accessibility bus connection: {e}"))?;
        let mut client = Self {
            bus,
            bus_id: String::new(),
            deadline,
            screen_reader: false,
        };
        client.bus_id = client
            .call(
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "GetId",
                &(),
            )
            .await?;
        if let Ok(session)=session { if let Ok(Ok(reply)) = tokio::time::timeout(
            CALL_LIMIT,
            session.call_method(
                Some("org.a11y.Bus"),
                "/org/a11y/bus",
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &("org.a11y.Status", "ScreenReaderEnabled"),
            ),
        )
        .await
        {
            if let Ok(value) = reply.body().deserialize::<OwnedValue>() {
                client.screen_reader = bool::try_from(value).unwrap_or(false);
            }
        }}
        Ok(client)
    }

    fn remaining(&self) -> Result<Duration, String> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .map(|d| d.min(CALL_LIMIT))
            .ok_or_else(|| "timeout: AT-SPI observation deadline exceeded".into())
    }

    async fn call<R: DeserializeOwned + Type, B: Serialize + Type>(
        &self,
        owner: &str,
        path: &str,
        interface: &str,
        method: &str,
        body: &B,
    ) -> Result<R, String> {
        let reply = tokio::time::timeout(
            self.remaining()?,
            self.bus
                .call_method(Some(owner), path, Some(interface), method, body),
        )
        .await
        .map_err(|_| format!("timeout: AT-SPI {interface}.{method}"))?
        .map_err(|e| format!("backend_failure: AT-SPI {interface}.{method}: {e}"))?;
        reply
            .body()
            .deserialize()
            .map_err(|e| format!("backend_failure: invalid AT-SPI {method} response: {e}"))
    }

    async fn property(
        &self,
        object: &Object,
        interface: &str,
        name: &str,
    ) -> Result<OwnedValue, String> {
        self.call(
            &object.owner,
            object.path.as_str(),
            "org.freedesktop.DBus.Properties",
            "Get",
            &(interface, name),
        )
        .await
    }
    async fn name(&self, object: &Object) -> Result<String, String> {
        String::try_from(self.property(object, ACCESSIBLE, "Name").await?)
            .map(|s| bounded(&s, 1024))
            .map_err(|e| format!("backend_failure: accessible Name: {e}"))
    }
    async fn role(&self, object: &Object) -> Result<(u32, String), String> {
        let role: u32 = self
            .call(
                &object.owner,
                object.path.as_str(),
                ACCESSIBLE,
                "GetRole",
                &(),
            )
            .await?;
        let name: String = self
            .call(
                &object.owner,
                object.path.as_str(),
                ACCESSIBLE,
                "GetRoleName",
                &(),
            )
            .await?;
        Ok((role, bounded(&name, 128).to_lowercase().replace(' ', "_")))
    }
    async fn states(&self, object: &Object) -> Result<Vec<u32>, String> {
        self.call(
            &object.owner,
            object.path.as_str(),
            ACCESSIBLE,
            "GetState",
            &(),
        )
        .await
    }
    async fn child_count(&self, object: &Object) -> Result<usize, String> {
        i32::try_from(self.property(object, ACCESSIBLE, "ChildCount").await?)
            .map(|n| n.max(0) as usize)
            .map_err(|e| format!("backend_failure: ChildCount: {e}"))
    }
    async fn child(&self, object: &Object, index: usize) -> Result<Object, String> {
        let (owner, path): (String, OwnedObjectPath) = self
            .call(
                &object.owner,
                object.path.as_str(),
                ACCESSIBLE,
                "GetChildAtIndex",
                &(index as i32),
            )
            .await?;
        if owner.is_empty() || path.as_str() == "/org/a11y/atspi/null" {
            return Err("stale_target: accessible child disappeared".into());
        }
        let owner = if owner.starts_with(':') {
            owner
        } else {
            self.call(
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "GetNameOwner",
                &(owner.as_str()),
            )
            .await?
        };
        Ok(Object { owner, path })
    }
    fn identity(&self, object: &Object) -> String {
        format!("atspi:{}:{}:{}", self.bus_id, object.owner, object.path)
    }

    async fn inventory(&self) -> Result<Inventory, String> {
        let root = Object {
            owner: REGISTRY.into(),
            path: ROOT
                .try_into()
                .map_err(|e| format!("backend_failure: {e}"))?,
        };
        let count = self.child_count(&root).await?;
        let mut result = Inventory {
            windows: Vec::new(),
            issues: Vec::new(),
            truncated: count > 128,
        };
        'apps: for i in 0..count.min(128) {
            if self.remaining().is_err() {
                result.truncated = true;
                break;
            }
            let app = match self.child(&root, i).await {
                Ok(v) => v,
                Err(e) => {
                    result.issues.push(e);
                    continue;
                }
            };
            remember_app(&self.bus_id, &app.owner);
            let app_name = match self.name(&app).await {
                Ok(name) => name,
                Err(error) => { result.issues.push(error); "unknown-app".into() }
            };
            let pid: u32 = self
                .call(
                    "org.freedesktop.DBus",
                    "/org/freedesktop/DBus",
                    "org.freedesktop.DBus",
                    "GetConnectionUnixProcessID",
                    &(app.owner.as_str()),
                )
                .await
                .unwrap_or(0);
            let children = match self.child_count(&app).await {
                Ok(v) => v,
                Err(e) => {
                    result.issues.push(e);
                    continue;
                }
            };
            if children > 128 {
                result.truncated = true;
            }
            for j in 0..children.min(128) {
                if self.remaining().is_err() || result.windows.len() >= MAX_WINDOWS {
                    result.truncated = true;
                    break 'apps;
                }
                let object = match self.child(&app, j).await {
                    Ok(v) => v,
                    Err(e) => {
                        result.issues.push(e);
                        continue;
                    }
                };
                let (native_role, role) = match self.role(&object).await {
                    Ok(v) => v,
                    Err(e) => {
                        result.issues.push(e);
                        continue;
                    }
                };
                // Applications may export non-window children under their root.
                if !matches!(native_role, 2 | 16 | 19 | 23 | 28 | 69) {
                    continue;
                }
                let title = match self.name(&object).await {
                    Ok(name) => name,
                    Err(error) => { result.issues.push(error); String::new() }
                };
                let states = self.states(&object).await;
                if let Err(error) = &states { result.issues.push(error.clone()); }
                let state_known = states.is_ok();
                let states = states.unwrap_or_default();
                let showing = state(&states, SHOWING) && state(&states, VISIBLE);
                let extents: Result<(i32, i32, i32, i32), String> = if showing {
                    self.call(
                        &object.owner,
                        object.path.as_str(),
                        "org.a11y.atspi.Component",
                        "GetExtents",
                        &(0u32),
                    )
                    .await
                } else {
                    Err("bounds_unknown".into())
                };
                let bounds_known = extents.is_ok();
                let (left, top, width, height) = extents.unwrap_or((0, 0, 0, 0));
                let identity = self.identity(&object);
                let hwnd = alias(&identity);
                if result.windows.iter().any(|w| {
                    w.meta.hwnd == hwnd && w.meta.native_id.as_deref() != Some(identity.as_str())
                }) {
                    return Err(
                        "identity_collision: refusing ambiguous AT-SPI window reference".into(),
                    );
                }
                result.windows.push(NativeWindow {
                    object,
                    meta: WindowInfo {
                        hwnd,
                        pid,
                        process: app_name.clone(),
                        title,
                        class_name: role,
                        offscreen: state_known && !showing,
                        minimized: state(&states, ICONIFIED),
                        focused: state(&states, ACTIVE) || state(&states, FOCUSED),
                        bounds: WindowBounds {
                            left,
                            top,
                            width,
                            height,
                        },
                        native_id: Some(identity),
                        bounds_known,
                        state_known,
                    },
                });
            }
        }
        if !result.issues.is_empty() { result.truncated = true; }
        if result.windows.is_empty() && result.truncated {
            return Err(format!("backend_failure: window inventory incomplete; {}", result.issues.first().cloned().unwrap_or_else(|| "observation limit reached".into())));
        }
        if result.truncated {
            result.issues.push("partial_inventory: node/window/deadline limit reached; omitted windows are unknown".into());
        }
        result.issues.truncate(32);
        Ok(result)
    }
}

fn state(states: &[u32], bit: usize) -> bool {
    states
        .get(bit / 32)
        .is_some_and(|word| word & (1u32 << (bit % 32)) != 0)
}
fn semantic_role(native: &str) -> String {
    match native {
        "frame"|"window"|"dialog"|"alert"|"file_chooser"|"internal_frame"=>"window",
        "push_button"|"toggle_button"|"button"=>"button",
        "check_box"|"check_menu_item"=>"checkbox",
        "radio_button"|"radio_menu_item"=>"radiobutton",
        "combo_box"=>"combobox", "entry"=>"edit", "password_text"=>"password",
        "text"|"document_frame"|"document_web"|"document_text"|"terminal"=>"document",
        "paragraph"|"heading"|"label"|"static"=>"text",
        "list_item"=>"listitem", "tree_item"=>"treeitem", "tree_table"=>"table",
        "table_cell"=>"dataitem", "page_tab_list"=>"tab", "page_tab"=>"tabitem",
        "menu_bar"=>"menubar", "menu_item"=>"menuitem", "tool_bar"=>"toolbar",
        "status_bar"=>"statusbar", "progress_bar"=>"progressbar", "scroll_bar"=>"scrollbar",
        "scroll_pane"|"panel"|"viewport"=>"pane", "filler"|"section"|"split_pane"=>"group",
        other=>other,
    }.to_owned()
}
fn bounded(text: &str, bytes: usize) -> String {
    let mut n = text.len().min(bytes);
    while !text.is_char_boundary(n) {
        n -= 1;
    }
    text[..n].to_owned()
}
fn alias(text: &str) -> isize {
    // Legacy display alias, always backed by native_id.
    let mut h = 14695981039346656037u64;
    for b in text.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    ((h & 0x001f_ffff_ffff_ffff).max(1)) as isize
}

pub fn list_windows() -> Result<WindowListResponse, String> {
    start_events();
    run(|| {
        Box::pin(async {
            let start = Instant::now();
            let client = Client::connect().await?;
            let inventory = client.inventory().await?;
            Ok(WindowListResponse {
                platform: "Linux AT-SPI2 / native Rust D-Bus".into(),
                screen_reader_flag: client.screen_reader,
                scan_time_ms: start.elapsed().as_secs_f64() * 1000.0,
                count: inventory.windows.len(),
                windows: inventory.windows.into_iter().map(|w| w.meta).collect(),
                issues: inventory.issues,
                truncated: inventory.truncated,
            })
        })
    })
}

pub fn inspect_window(
    target: &str,
    max_depth: usize,
    max_nodes: usize,
) -> Result<InspectResponse, String> {
    inspect_window_with_options(target, max_depth, max_nodes, true)
}

pub fn inspect_window_with_options(
    target: &str,
    max_depth: usize,
    max_nodes: usize,
    capture_text: bool,
) -> Result<InspectResponse, String> {
    start_events();
    let target = target.to_owned();
    run(move || {
        Box::pin(async move {
            let start = Instant::now();
            let client = Client::connect().await?;
            let inventory = client.inventory().await?;
            let exact: Vec<_> = inventory
                .windows
                .into_iter()
                .filter(|w| {
                    w.meta.hwnd.to_string() == target
                        || w.meta.native_id.as_deref() == Some(target.as_str())
                })
                .collect();
            if exact.len() != 1 {
                return Err("stale_target: select one exact window reference from access_windows; title and PID matches are not identities".into());
            }
            let window = exact.into_iter().next().ok_or("stale_target")?;
            let mut issues = inventory.issues;
            let mut truncated = inventory.truncated;
            let mut pending = vec![(window.object, 0usize, None)];
            let mut visited = HashSet::new();
            let mut flat: Vec<(TreeNode, Option<usize>)> = Vec::new();
            let mut buffers = Vec::new();
            let mut total_text = 0usize;
            let mut raw = 0usize;
            let limit = max_nodes.clamp(1, 5000);
            let depth_limit = max_depth.min(64);
            while let Some((object, depth, parent)) = pending.pop() {
                if raw >= MAX_RAW || flat.len() >= limit || client.remaining().is_err() {
                    truncated = true;
                    break;
                }
                if !visited.insert(object.clone()) {
                    continue;
                }
                raw += 1;
                let (native_role, role) = match client.role(&object).await {
                    Ok(v) => v,
                    Err(e) => {
                        issues.push(e);
                        continue;
                    }
                };
                let native_role_name=Some(format!("{}:{}",native_role,role.replace('_'," ")));
                let role=semantic_role(&role);
                // Check the native password role BEFORE Name, Text, Value, attributes
                // or children are read. Protected controls cannot enter text caches.
                let protected = native_role == 40;
                let name = if protected {
                    String::new()
                } else {
                    client.name(&object).await.unwrap_or_default()
                };
                let states = client.states(&object).await;
                let state_known = states.is_ok();
                if let Err(e) = &states {
                    issues.push(e.clone())
                }
                let states = states.unwrap_or_default();
                let native_id = client.identity(&object);
                let id = format!("@a{}", alias(&native_id));
                let mut text = None;
                if !protected && total_text < MAX_TEXT {
                    let interfaces: Result<Vec<String>, String> = client
                        .call(
                            &object.owner,
                            object.path.as_str(),
                            ACCESSIBLE,
                            "GetInterfaces",
                            &(),
                        )
                        .await;
                    if let Err(error) = &interfaces { issues.push(error.clone()); truncated = true; }
                    if interfaces
                        .as_ref()
                        .is_ok_and(|v| v.iter().any(|s| s == "org.a11y.atspi.Text"))
                    {
                        let count = client
                            .property(&object, "org.a11y.atspi.Text", "CharacterCount")
                            .await
                            .and_then(|v| {
                                i32::try_from(v)
                                    .map_err(|e| format!("backend_failure: character count: {e}"))
                            });
                        if let Ok(count) = count {
                            if count > 0 {
                                let requested =
                                    (count as usize).min(if capture_text {16 * 1024} else {180}).min(MAX_TEXT - total_text)
                                        as i32;
                                let full: Result<String, String> = client
                                    .call(
                                        &object.owner,
                                        object.path.as_str(),
                                        "org.a11y.atspi.Text",
                                        "GetText",
                                        &(0i32, requested),
                                    )
                                    .await;
                                match full {
                                    Ok(full) => {
                                        let full = bounded(&full, MAX_TEXT - total_text);
                                        total_text += full.len();
                                        let native_total = count as usize;
                                        let obtained = full.chars().count();
                                        let is_partial = !capture_text || obtained < native_total;
                                        if is_partial && capture_text {
                                            truncated = true;
                                            issues.push("text_truncated: native buffer exceeds bounded captured prefix; visibility coverage unknown".into());
                                        }
                                        text=Some(TextBufferInfo{id:id.clone(),role:role.clone(),name:name.clone(),source:"AT-SPI Text (Unicode character offsets; visible range unknown)".into(),
                                total_chars:native_total,visible_chars:0,offscreen_chars:0,total_lines:if is_partial {0} else {full.lines().count()},visible_lines:0,preview:bounded(&full,180),full_text:if capture_text {full} else {String::new()},visible_text:String::new(),
                                truncated:is_partial,protected:false,visibility_known:false});
                                    }
                                    Err(e) => issues.push(e),
                                }
                            }
                        } else if let Err(e) = count {
                            issues.push(e)
                        }
                    }
                }
                let index = flat.len();
                let native_state = if state_known {json!({
                    "nativeWords":states,"enabled":state(&states,ENABLED),"sensitive":state(&states,24),
                    "focused":state(&states,FOCUSED),"focusable":state(&states,11),"showing":state(&states,SHOWING),
                    "visible":state(&states,VISIBLE),"editable":state(&states,7),
                    "readOnly":if state(&states,43){Some(true)}else if state(&states,7){Some(false)}else{None},
                    "checked":if matches!(native_role,7|8|44|45|62)||state(&states,41){Some(state(&states,4))}else{None},
                    "selected":if state(&states,22){Some(state(&states,23))}else{None},
                    "expanded":if state(&states,9){Some(state(&states,10))}else{None},
                    "busy":state(&states,3),"defunct":state(&states,6),"protected":protected
                })}else{serde_json::Value::Null};
                let node = TreeNode {
                    id: id.clone(),
                    role,
                    name,
                    depth,
                    offscreen: !state(&states, SHOWING),
                    focused: state(&states, FOCUSED),
                    enabled: state(&states, ENABLED),
                    has_text: text.is_some(),
                    text_source: text.as_ref().map(|t| t.source.clone()),
                    total_chars: text.as_ref().map_or(0, |t| t.total_chars),
                    offscreen_chars: 0,
                    preview: text
                        .as_ref()
                        .map_or_else(String::new, |t| t.preview.clone()),
                    children: Vec::new(),
                    protected,
                    native_id: Some(native_id),
                    native_role: native_role_name,
                    states: native_state,
                };
                if let Some(buffer) = text {
                    buffers.push(buffer)
                }
                flat.push((node, parent));
                if protected {
                    continue;
                }
                let count = match client.child_count(&object).await {
                    Ok(v) => v,
                    Err(e) => {
                        issues.push(e);
                        continue;
                    }
                };
                if depth >= depth_limit {
                    if count > 0 {
                        truncated = true;
                    }
                    continue;
                }
                let room = limit
                    .saturating_sub(flat.len() + pending.len())
                    .min(MAX_RAW.saturating_sub(raw + pending.len()));
                if count > room {
                    truncated = true
                }
                for i in (0..count.min(room)).rev() {
                    match client.child(&object, i).await {
                        Ok(child) => pending.push((child, depth + 1, Some(index))),
                        Err(e) => issues.push(e),
                    }
                }
            }
            if flat.is_empty() {
                return Err(
                    "backend_failure: selected window has no readable accessible root".into(),
                );
            }
            if !issues.is_empty() { truncated = true; }
            if truncated {
                issues.push(
                    "partial_observation: applied native node/depth/text/deadline limits".into(),
                )
            }
            issues.truncate(32);
            let semantic = flat.len();
            let mut tree = Vec::new();
            for i in (0..flat.len()).rev() {
                let node = flat[i].0.clone();
                if let Some(parent) = flat[i].1 {
                    flat[parent].0.children.insert(0, node)
                } else {
                    tree.insert(0, node)
                }
            }
            let llm_yaml = serde_json::to_string(&tree)
                .map_err(|e| format!("backend_failure: tree serialization: {e}"))?;
            Ok(InspectResponse {
                window: InspectWindowMeta {
                    hwnd: window.meta.hwnd,
                    pid: window.meta.pid,
                    process: window.meta.process,
                    title: window.meta.title,
                    class_name: window.meta.class_name,
                    offscreen: window.meta.offscreen,
                },
                stats: InspectStats {
                    raw_nodes: raw,
                    semantic_nodes: semantic,
                    compression_pct: 0.0,
                    text_buffers_found: buffers.len(),
                    total_extracted_chars: buffers
                        .iter()
                        .map(|b| b.full_text.chars().count())
                        .sum(),
                    offscreen_extracted_chars: 0,
                    scan_time_ms: start.elapsed().as_secs_f64() * 1000.0,
                    estimated_tokens: llm_yaml.len() / 4,
                    issues,
                    truncated,
                },
                llm_yaml,
                text_buffers: buffers,
                tree,
            })
        })
    })
}

pub fn capabilities() -> serde_json::Value {
    json!({"backend":"atspi2-native-dbus","readOnly":true,"pythonRequired":false,"requiresGraphicalSession":true,"activation":"explicit-only; observation does not change org.a11y.Status","windowIdentity":"accessibility-bus-id + unique D-Bus owner + object path","geometry":"AT-SPI Component.GetExtents when visible and showing; otherwise unknown","textOffsets":"Unicode characters","textVisibility":"unknown","protectedRoles":["password_text"],"nativeEventSubscriptions":true,"events":native_event_status(),"changeTracking":"native signal invalidation plus bounded explicit reconciliation","limits":{"observationMs":2500,"callMs":350,"rawNodes":5000,"depth":64,"textBytes":65536},"actions":[]})
}
