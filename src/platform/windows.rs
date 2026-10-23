//! Observation-only Windows UI Automation adapter.
//! COM interfaces never leave the MTA that created them. Native calls are bounded
//! by UIA2 provider timeouts and an outer deadline; abandoned reads retain one of
//! four worker permits until they actually finish (no unbounded stuck threads).
use crate::engine::*;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows::core::{Interface, PWSTR};
use windows::Win32::Foundation::{CloseHandle, BOOL, FILETIME, HANDLE, HWND, LPARAM, RECT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Ole::{
    SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetDim, SafeArrayGetLBound, SafeArrayGetUBound,
    SafeArrayUnaccessData,
};
use windows::Win32::System::StationsAndDesktops::{
    GetProcessWindowStation, GetUserObjectInformationW, UOI_FLAGS, USEROBJECTFLAGS,
};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, SystemParametersInfoW,
    SPI_GETSCREENREADER,
};

const OBSERVATION_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_RAW_NODES: usize = 5_000;
const MAX_DEPTH: usize = 64;
const MAX_NODES: usize = 250;
const MAX_TEXT_CHARS: usize = 16_384;
const MAX_TOTAL_TEXT_CHARS: usize = 65_536;
const MAX_WINDOWS: usize = 512;
static ACTIVE_READS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_APPS: OnceLock<Mutex<HashSet<u32>>> = OnceLock::new();

struct AppPermit(u32);
impl AppPermit {
    fn acquire(pid: u32) -> Result<Self, String> {
        let mut apps = ACTIVE_APPS
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| {
                "backendFailure: Windows per-application read registry poisoned".to_string()
            })?;
        if !apps.insert(pid) {
            return Err("busy: this application already has an active accessibility read".into());
        }
        Ok(Self(pid))
    }
}
impl Drop for AppPermit {
    fn drop(&mut self) {
        if let Ok(mut apps) = ACTIVE_APPS.get_or_init(Mutex::default).lock() {
            apps.remove(&self.0);
        }
    }
}

struct WorkerPermit;
impl Drop for WorkerPermit {
    fn drop(&mut self) {
        ACTIVE_READS.fetch_sub(1, Ordering::AcqRel);
    }
}

fn on_worker<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    #[allow(deprecated)] // Remains compatible with the existing Rust 1.90 toolchain.
    ACTIVE_READS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < 4).then_some(n + 1)
        })
        .map_err(|_| "busy: all bounded Windows observation workers are occupied".to_string())?;
    let (tx, rx) = mpsc::sync_channel(1);
    let spawned = std::thread::Builder::new()
        .name("axiaccess-uia-mta".into())
        .spawn(move || {
            let _permit = WorkerPermit;
            let _ = tx.send(work());
        });
    if let Err(error) = spawned {
        ACTIVE_READS.fetch_sub(1, Ordering::AcqRel);
        return Err(format!(
            "backendUnavailable: cannot start UIA worker: {error}"
        ));
    }
    rx.recv_timeout(OBSERVATION_TIMEOUT).map_err(|error| -> String { match error {
        mpsc::RecvTimeoutError::Timeout => "timeout: Windows observation exceeded three seconds; unfinished native worker remains bounded".into(),
        mpsc::RecvTimeoutError::Disconnected => "backendFailure: Windows observation worker exited unexpectedly".into(),
    } })?
}

struct ComApartment;
impl ComApartment {
    fn new() -> Result<Self, String> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }
            .map_err(|e| format!("backendUnavailable: cannot initialize UIA MTA: {e}"))?;
        Ok(Self)
    }
}
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

fn init_uia() -> Result<IUIAutomation, String> {
    let uia: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| format!("backendUnavailable: cannot create Windows UI Automation: {e}"))?;
    let uia2 = uia
        .cast::<IUIAutomation2>()
        .map_err(|e| format!("unsupportedBackend: UIA2 timeouts are required: {e}"))?;
    unsafe {
        uia2.SetConnectionTimeout(500)
            .map_err(|e| format!("backendFailure: UIA connection timeout: {e}"))?;
        uia2.SetTransactionTimeout(500)
            .map_err(|e| format!("backendFailure: UIA transaction timeout: {e}"))?;
    }
    Ok(uia)
}

pub fn capabilities() -> Value {
    let events = native_event_status();
    json!({"platform":"windows", "backend":"UIAutomation", "native":true,
        "readOnly":true, "inputActions":false, "windowInventory":true,
        "hierarchy":true,"text":true,"passwordFiltering":true,
        "nativeEvents":events["ready"].as_bool().unwrap_or(false), "supportsNativeEvents":true, "events":events,
        "liveReferences":"process-lifetime+HWND; element UIA runtime IDs are lifetime scoped",
        "sessionScope":"current interactive user desktop; no secure-desktop or elevated-access guarantee",
        "limits":{"observationMs":3000,"providerCallMs":500,"rawNodes":MAX_RAW_NODES,
            "depth":MAX_DEPTH,"nodes":MAX_NODES,"textCharsPerElement":MAX_TEXT_CHARS,
            "totalTextChars":MAX_TOTAL_TEXT_CHARS,"activeReads":4},
        "coverage":"Application-exposed accessibility only; no scrolling, focusing or virtualized-item realization"})
}

pub fn get_screen_reader_flag() -> bool {
    let mut flag = BOOL(0);
    (unsafe {
        SystemParametersInfoW(
            SPI_GETSCREENREADER,
            0,
            Some(&mut flag as *mut BOOL as *mut c_void),
            Default::default(),
        )
        .is_ok()
    }) && flag.as_bool()
}

// Event callbacks never inspect the sender, retrieve text, follow properties or
// enter the coordinator. They only invalidate retained observations by revision.
static EVENT_REVISION: AtomicU64 = AtomicU64::new(1);
static EVENT_STARTING: AtomicBool = AtomicBool::new(false);
static EVENT_LAST_USE: OnceLock<Mutex<Instant>> = OnceLock::new();
static EVENT_STATUS: OnceLock<Mutex<Value>> = OnceLock::new();
static EVENT_SIGNAL: OnceLock<(Mutex<()>, Condvar)> = OnceLock::new();

fn event_status_cell() -> &'static Mutex<Value> {
    EVENT_STATUS.get_or_init(|| {
        Mutex::new(
            json!({"supported":true,"running":false,"ready":false,"coverage":"provider dependent"}),
        )
    })
}
fn set_event_status(value: Value) {
    if let Ok(mut current) = event_status_cell().lock() {
        *current = value;
    }
}
fn invalidate() {
    let (lock, signal) = EVENT_SIGNAL.get_or_init(|| (Mutex::new(()), Condvar::new()));
    let _guard = lock.lock().ok();
    EVENT_REVISION.fetch_add(1, Ordering::AcqRel);
    signal.notify_all();
}

#[windows::core::implement(
    IUIAutomationFocusChangedEventHandler,
    IUIAutomationStructureChangedEventHandler,
    IUIAutomationEventHandler,
    IUIAutomationPropertyChangedEventHandler
)]
struct InvalidationSink;

impl IUIAutomationFocusChangedEventHandler_Impl for InvalidationSink_Impl {
    fn HandleFocusChangedEvent(
        &self,
        _sender: Option<&IUIAutomationElement>,
    ) -> windows::core::Result<()> {
        invalidate();
        Ok(())
    }
}
impl IUIAutomationStructureChangedEventHandler_Impl for InvalidationSink_Impl {
    fn HandleStructureChangedEvent(
        &self,
        _sender: Option<&IUIAutomationElement>,
        _change: StructureChangeType,
        _runtime_id: *const windows::Win32::System::Com::SAFEARRAY,
    ) -> windows::core::Result<()> {
        invalidate();
        Ok(())
    }
}
impl IUIAutomationEventHandler_Impl for InvalidationSink_Impl {
    fn HandleAutomationEvent(
        &self,
        _sender: Option<&IUIAutomationElement>,
        _event: UIA_EVENT_ID,
    ) -> windows::core::Result<()> {
        invalidate();
        Ok(())
    }
}
impl IUIAutomationPropertyChangedEventHandler_Impl for InvalidationSink_Impl {
    fn HandlePropertyChangedEvent(
        &self,
        _sender: Option<&IUIAutomationElement>,
        _property: UIA_PROPERTY_ID,
        _value: &windows::core::VARIANT,
    ) -> windows::core::Result<()> {
        invalidate();
        Ok(())
    }
}

fn start_native_events() {
    if let Ok(mut last) = EVENT_LAST_USE
        .get_or_init(|| Mutex::new(Instant::now()))
        .lock()
    {
        *last = Instant::now();
    }
    if EVENT_STARTING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let spawned=std::thread::Builder::new().name("axiaccess-uia-events-mta".into()).spawn(|| {
        let result=(|| -> Result<(),String> {
            let _apartment=ComApartment::new()?;
            let uia=init_uia()?;
            let root=unsafe {uia.GetRootElement()}.map_err(|e|format!("backendFailure: event root: {e}"))?;
            let sink:IUIAutomationEventHandler=InvalidationSink.into();
            let focus=sink.cast::<IUIAutomationFocusChangedEventHandler>().map_err(|e|e.to_string())?;
            let structure=sink.cast::<IUIAutomationStructureChangedEventHandler>().map_err(|e|e.to_string())?;
            let property=sink.cast::<IUIAutomationPropertyChangedEventHandler>().map_err(|e|e.to_string())?;
            let mut registrations=0;
            let mut issues=Vec::new();
            macro_rules! register {($name:expr,$call:expr)=>{match unsafe {$call} {Ok(())=>registrations+=1,Err(error)=>issues.push(format!("{}: {}",$name,error))}};}
            register!("focus",uia.AddFocusChangedEventHandler(None::<&IUIAutomationCacheRequest>,&focus));
            register!("structure",uia.AddStructureChangedEventHandler(&root,TreeScope_Subtree,None::<&IUIAutomationCacheRequest>,&structure));
            register!("properties",uia.AddPropertyChangedEventHandlerNativeArray(&root,TreeScope_Subtree,None::<&IUIAutomationCacheRequest>,&property,&[UIA_NamePropertyId,UIA_IsEnabledPropertyId,UIA_IsOffscreenPropertyId,UIA_BoundingRectanglePropertyId]));
            for event in [UIA_Text_TextChangedEventId,UIA_Window_WindowOpenedEventId,UIA_Window_WindowClosedEventId] {
                register!(format!("event{}",event.0),uia.AddAutomationEventHandler(event,&root,TreeScope_Subtree,None::<&IUIAutomationCacheRequest>,&sink));
            }
            set_event_status(json!({"supported":true,"running":registrations>0,"ready":registrations>0,"registrations":registrations,"issues":issues,"coverage":"focus, structure, text, windows and selected properties where providers emit events","completeCoverage":false}));
            if registrations==0 {return Err("backendUnavailable: no Windows UIA event handlers could be registered".into());}
            invalidate(); // A fresh registration scope requires reconciliation.
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let inactive=EVENT_LAST_USE.get().and_then(|last|last.lock().ok().map(|v|v.elapsed()>Duration::from_secs(60))).unwrap_or(true);
                if inactive {break;}
            }
            // Same owner MTA unregisters before dropping COM references. Late
            // callbacks are safe because COM retains its own handler reference.
            unsafe {uia.RemoveAllEventHandlers()}.map_err(|e|format!("backendFailure: remove UIA event handlers: {e}"))?;
            set_event_status(json!({"supported":true,"running":false,"ready":false,"reason":"idle; subscriptions restart lazily","completeCoverage":false}));
            invalidate();
            Ok(())
        })();
        if let Err(error)=result {set_event_status(json!({"supported":true,"running":false,"ready":false,"error":error,"completeCoverage":false}));}
        EVENT_STARTING.store(false,Ordering::Release);
    });
    if let Err(error) = spawned {
        EVENT_STARTING.store(false, Ordering::Release);
        set_event_status(
            json!({"supported":true,"running":false,"ready":false,"error":format!("cannot start UIA event worker: {error}")}),
        );
    }
}

pub fn native_event_revision() -> u64 {
    start_native_events();
    EVENT_REVISION.load(Ordering::Acquire)
}
pub fn wait_native_events(since: u64, timeout: Duration) -> u64 {
    start_native_events();
    let (lock, signal) = EVENT_SIGNAL.get_or_init(|| (Mutex::new(()), Condvar::new()));
    if let Ok(guard) = lock.lock() {
        let _wait = signal.wait_timeout_while(guard, timeout.min(Duration::from_secs(30)), |_| {
            EVENT_REVISION.load(Ordering::Acquire) == since
        });
    }
    EVENT_REVISION.load(Ordering::Acquire)
}
pub fn native_event_status() -> Value {
    start_native_events();
    event_status_cell()
        .lock()
        .map(|v| v.clone())
        .unwrap_or_else(
            |_| json!({"supported":true,"ready":false,"error":"event status unavailable"}),
        )
}

fn process_identity(pid: u32) -> (String, Option<u64>) {
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return (format!("pid:{pid}"), None);
        };
        let mut path = [0u16; 1024];
        let mut length = path.len() as u32;
        let process = if QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut length,
        )
        .is_ok()
        {
            String::from_utf16_lossy(&path[..length as usize])
                .rsplit('\\')
                .next()
                .unwrap_or_default()
                .to_string()
        } else {
            format!("pid:{pid}")
        };
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let creation = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user)
            .ok()
            .map(|_| ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64);
        let _ = CloseHandle(handle);
        (process, creation)
    }
}

struct Enumeration {
    hwnds: Vec<isize>,
    truncated: bool,
}
unsafe extern "system" fn enum_window(hwnd: HWND, context: LPARAM) -> BOOL {
    let state = &mut *(context.0 as *mut Enumeration);
    if state.hwnds.len() >= MAX_WINDOWS {
        state.truncated = true;
        return BOOL(0);
    }
    if IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
        state.hwnds.push(hwnd.0 as isize);
    }
    BOOL(1)
}

fn native_window(hwnd_val: isize, focused: isize) -> Option<WindowInfo> {
    unsafe {
        let hwnd = HWND(hwnd_val as *mut c_void);
        if !IsWindow(hwnd).as_bool() {
            return None;
        }
        let mut name = [0u16; 4096];
        let name_len = GetWindowTextW(hwnd, &mut name).max(0) as usize;
        let title = String::from_utf16_lossy(&name[..name_len]);
        let mut class = [0u16; 256];
        let class_len = GetClassNameW(hwnd, &mut class).max(0) as usize;
        let class_name = String::from_utf16_lossy(&class[..class_len]);
        if title.is_empty()
            || matches!(
                class_name.as_str(),
                "Progman" | "WorkerW" | "IME" | "MSCTFIME UI"
            )
        {
            return None;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let (process, created) = process_identity(pid);
        let mut rect = RECT::default();
        let bounds_known = GetWindowRect(hwnd, &mut rect).is_ok();
        let minimized = IsIconic(hwnd).as_bool();
        Some(WindowInfo {
            hwnd: hwnd_val,
            pid,
            process,
            title,
            class_name,
            offscreen: !IsWindowVisible(hwnd).as_bool(),
            minimized,
            focused: hwnd_val == focused,
            bounds: WindowBounds {
                left: rect.left,
                top: rect.top,
                width: rect.right.saturating_sub(rect.left).max(0),
                height: rect.bottom.saturating_sub(rect.top).max(0),
            },
            bounds_known,
            state_known: false, // UIA off-screen status is resolved separately.
            native_id: created.map(|birth| format!("win:{hwnd_val}:{pid}:{birth}")),
        })
    }
}

fn enumerate() -> Result<(Vec<WindowInfo>, bool), String> {
    unsafe {
        // Borrow the current process station; no desktop switching or opened
        // desktop handles are needed for observation in the logged-in session.
        let station = GetProcessWindowStation()
            .map_err(|e| format!("sessionUnavailable: current window station unavailable: {e}"))?;
        let mut flags = USEROBJECTFLAGS::default();
        GetUserObjectInformationW(
            HANDLE(station.0),
            UOI_FLAGS,
            Some(&mut flags as *mut USEROBJECTFLAGS as *mut c_void),
            std::mem::size_of::<USEROBJECTFLAGS>() as u32,
            None,
        )
        .map_err(|e| format!("sessionUnavailable: window station status unavailable: {e}"))?;
        if flags.dwFlags & 1 == 0 {
            // WSF_VISIBLE
            return Err(
                "sessionUnavailable: sender is not in an interactive window station".into(),
            );
        }
    }
    let mut state = Enumeration {
        hwnds: vec![],
        truncated: false,
    };
    let result = unsafe {
        EnumWindows(
            Some(enum_window),
            LPARAM(&mut state as *mut Enumeration as isize),
        )
    };
    if result.is_err() && !state.truncated {
        return Err(format!(
            "sessionUnavailable: cannot enumerate current desktop: {}",
            result.unwrap_err()
        ));
    }
    let focus = unsafe { GetForegroundWindow().0 as isize };
    let mut windows: Vec<_> = state
        .hwnds
        .into_iter()
        .filter_map(|h| native_window(h, focus))
        .collect();
    windows.sort_by(|a, b| {
        (!a.focused)
            .cmp(&(!b.focused))
            .then(a.minimized.cmp(&b.minimized))
            .then(a.process.to_lowercase().cmp(&b.process.to_lowercase()))
    });
    Ok((windows, state.truncated))
}

pub fn list_windows() -> Result<WindowListResponse, String> {
    on_worker(|| {
        let start = Instant::now();
        let _apartment = ComApartment::new()?;
        let uia = init_uia()?;
        let (mut windows, mut truncated) = enumerate()?;
        let mut issues = Vec::new();
        for window in &mut windows {
            if start.elapsed() >= Duration::from_millis(2200) {
                truncated = true;
                issues.push(
                    "identityDeadline: remaining native window identities were not resolved".into(),
                );
                break;
            }
            let base = window.native_id.take();
            let identity = unsafe { uia.ElementFromHandle(HWND(window.hwnd as *mut c_void)) }
                .ok()
                .and_then(|element| {
                    if let Ok(offscreen) = unsafe { element.CurrentIsOffscreen() } {
                        window.offscreen = offscreen.as_bool();
                        window.state_known = true;
                    }
                    runtime_id(&element)
                });
            window.native_id = match (base, identity) {
                (Some(base), Some(identity)) => Some(format!("{base}/root:{identity}")),
                _ => {
                    truncated = true;
                    if !issues.iter().any(|issue| issue == "identityUnavailable: some window references cannot be safely inspected") {
                        issues.push("identityUnavailable: some window references cannot be safely inspected".into());
                    }
                    None
                }
            };
        }
        // Entries beyond the identity deadline cannot retain a weaker HWND-only
        // reference and accidentally be treated as lifetime-bound observations.
        for window in &mut windows {
            if window
                .native_id
                .as_ref()
                .is_some_and(|id| !id.contains("/root:"))
            {
                window.native_id = None;
            }
        }
        if truncated && issues.is_empty() {
            issues.push("windowLimit: inventory exceeded 512 windows".into());
        }
        Ok(WindowListResponse {
            platform: "Windows native window inventory / UI Automation".into(),
            screen_reader_flag: get_screen_reader_flag(),
            scan_time_ms: start.elapsed().as_secs_f64() * 1000.0,
            count: windows.len(),
            windows,
            issues,
            truncated,
        })
    })
}

fn choose_window(target: &str, windows: &[WindowInfo]) -> Result<WindowInfo, String> {
    let target = target.trim();
    if target.is_empty() {
        return Err("invalidTarget: select an exact window".into());
    }
    if let Some(window) = windows
        .iter()
        .find(|w| w.native_id.as_deref() == Some(target))
    {
        return Ok(window.clone());
    }
    if target.starts_with("win:") {
        return Err(
            "staleReference: native Windows process/window identity no longer exists".into(),
        );
    }
    if let Ok(hwnd) = target.parse::<isize>() {
        if let Some(window) = windows.iter().find(|w| w.hwnd == hwnd) {
            return Ok(window.clone());
        }
    }
    let lower = target.to_lowercase();
    let exact: Vec<_> = windows
        .iter()
        .filter(|w| {
            w.pid.to_string() == target
                || w.title.eq_ignore_ascii_case(target)
                || w.process.eq_ignore_ascii_case(target)
        })
        .collect();
    let candidates: Vec<_> = if exact.is_empty() {
        windows
            .iter()
            .filter(|w| {
                w.title.to_lowercase().contains(&lower)
                    || w.process.to_lowercase().contains(&lower)
                    || w.class_name.to_lowercase().contains(&lower)
            })
            .collect()
    } else {
        exact
    };
    match candidates.as_slice() {
        [window] => Ok((*window).clone()),
        [] => Err("targetNotFound: no matching window on this desktop".into()),
        _ => Err(format!(
            "ambiguousTarget: {} matching windows; use an exact returned window reference",
            candidates.len()
        )),
    }
}

fn cache_request(uia: &IUIAutomation) -> Result<IUIAutomationCacheRequest, String> {
    unsafe {
        let req = uia
            .CreateCacheRequest()
            .map_err(|e| format!("backendFailure: cache request: {e}"))?;
        // Do not cache Name/Value/Text before the protected-field check.
        for property in [
            UIA_ControlTypePropertyId,
            UIA_IsPasswordPropertyId,
            UIA_IsOffscreenPropertyId,
            UIA_IsEnabledPropertyId,
            UIA_HasKeyboardFocusPropertyId,
        ] {
            req.AddProperty(property)
                .map_err(|e| format!("backendFailure: cache property: {e}"))?;
        }
        Ok(req)
    }
}

fn runtime_id(element: &IUIAutomationElement) -> Option<String> {
    unsafe {
        let array = element.GetRuntimeId().ok()?;
        if array.is_null() {
            return None;
        }
        let answer = (|| {
            if SafeArrayGetDim(array) != 1 {
                return None;
            }
            let lower = SafeArrayGetLBound(array, 1).ok()?;
            let upper = SafeArrayGetUBound(array, 1).ok()?;
            let count = upper.checked_sub(lower)?.checked_add(1)? as usize;
            if count == 0 || count > 128 {
                return None;
            }
            let mut data = std::ptr::null_mut();
            SafeArrayAccessData(array, &mut data).ok()?;
            let result = if data.is_null() {
                None
            } else {
                Some(
                    std::slice::from_raw_parts(data as *const i32, count)
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join("."),
                )
            };
            let _ = SafeArrayUnaccessData(array);
            result
        })();
        let _ = SafeArrayDestroy(array);
        answer
    }
}

fn role(id: i32) -> &'static str {
    match id {
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

fn preview(text: &str) -> String {
    text.chars()
        .take(180)
        .collect::<String>()
        .replace('\r', "")
        .replace('\n', " ↵ ")
}
fn bounded_text(text: String, limit: usize) -> (String, bool) {
    let truncated = text.chars().count() > limit;
    (text.chars().take(limit).collect(), truncated)
}

fn read_text(
    element: &IUIAutomationElement,
    role: &str,
    limit: usize,
    id: &str,
    name: &str,
    offscreen: bool,
) -> Option<TextBufferInfo> {
    if limit == 0 {
        return None;
    }
    unsafe {
        let mut visibility_known = false;
        let mut visible_text = String::new();
        let mut source = "ValuePattern";
        let mut full = None;
        if matches!(role, "document" | "edit") {
            if let Ok(pattern) = element
                .GetCurrentPattern(UIA_TextPatternId)
                .and_then(|p| p.cast::<IUIAutomationTextPattern>())
            {
                if let Ok(range) = pattern.DocumentRange() {
                    if let Ok(text) = range.GetText((limit + 1) as i32) {
                        full = Some(text.to_string());
                        source = "TextPattern";
                    }
                }
                if let Ok(ranges) = pattern.GetVisibleRanges() {
                    visibility_known = true;
                    let count = ranges.Length().unwrap_or(0).min(8);
                    for index in 0..count {
                        if let Ok(range) = ranges.GetElement(index) {
                            match range.GetText((limit + 1) as i32) {
                                Ok(text) => {
                                    if !visible_text.is_empty() {
                                        visible_text.push('\n');
                                    }
                                    visible_text.push_str(&text.to_string());
                                }
                                Err(_) => visibility_known = false,
                            }
                        }
                        if visible_text.chars().count() >= limit {
                            break;
                        }
                    }
                    visible_text = bounded_text(visible_text, limit).0;
                }
            }
        }
        if full.is_none() {
            if let Ok(pattern) = element
                .GetCurrentPattern(UIA_ValuePatternId)
                .and_then(|p| p.cast::<IUIAutomationValuePattern>())
            {
                if let Ok(value) = pattern.CurrentValue() {
                    full = Some(value.to_string());
                }
            }
        }
        let (full_text, truncated) = bounded_text(full?, limit);
        if full_text.is_empty() {
            return None;
        }
        let total_chars = full_text.chars().count();
        let visible_chars = if visibility_known {
            visible_text.chars().count().min(total_chars)
        } else {
            0
        };
        let offscreen_chars = if visibility_known {
            total_chars.saturating_sub(visible_chars)
        } else if offscreen {
            total_chars
        } else {
            0
        };
        Some(TextBufferInfo {
            id: id.into(),
            role: role.into(),
            name: name.into(),
            source: source.into(),
            total_chars,
            visible_chars,
            offscreen_chars,
            total_lines: full_text.lines().count(),
            visible_lines: visible_text.lines().count(),
            preview: preview(&full_text),
            full_text,
            visible_text,
            truncated,
            protected: false,
            visibility_known,
        })
    }
}

struct Walk<'a> {
    walker: &'a IUIAutomationTreeWalker,
    cache: &'a IUIAutomationCacheRequest,
    deadline: Instant,
    max_depth: usize,
    max_nodes: usize,
    raw: usize,
    retained: usize,
    text_chars: usize,
    buffers: Vec<TextBufferInfo>,
    seen: HashSet<String>,
    issues: Vec<String>,
    truncated: bool,
    window_native: Option<String>,
    capture_text: bool,
}

fn readable_states(element: &IUIAutomationElement, role: &str, protected: bool) -> Value {
    let mut states = json!({"enabled":null,"focused":null,"offscreen":null,"readOnly":null,"selected":null,"checked":null,"expanded":null,"value":null,"minimum":null,"maximum":null,"protected":protected});
    unsafe {
        states["enabled"] = element
            .CachedIsEnabled()
            .or_else(|_| element.CurrentIsEnabled())
            .map(|v| json!(v.as_bool()))
            .unwrap_or(Value::Null);
        states["focused"] = element
            .CachedHasKeyboardFocus()
            .or_else(|_| element.CurrentHasKeyboardFocus())
            .map(|v| json!(v.as_bool()))
            .unwrap_or(Value::Null);
        states["offscreen"] = element
            .CachedIsOffscreen()
            .or_else(|_| element.CurrentIsOffscreen())
            .map(|v| json!(v.as_bool()))
            .unwrap_or(Value::Null);
        if protected {
            return states;
        }
        if matches!(role, "edit" | "document") {
            if let Ok(pattern) = element
                .GetCurrentPattern(UIA_ValuePatternId)
                .and_then(|v| v.cast::<IUIAutomationValuePattern>())
            {
                states["readOnly"] = pattern
                    .CurrentIsReadOnly()
                    .map(|v| json!(v.as_bool()))
                    .unwrap_or(Value::Null);
            }
        }
        if matches!(role, "listitem" | "tabitem" | "treeitem" | "radiobutton") {
            if let Ok(pattern) = element
                .GetCurrentPattern(UIA_SelectionItemPatternId)
                .and_then(|v| v.cast::<IUIAutomationSelectionItemPattern>())
            {
                states["selected"] = pattern
                    .CurrentIsSelected()
                    .map(|v| json!(v.as_bool()))
                    .unwrap_or(Value::Null);
            }
        }
        if role == "checkbox" {
            if let Ok(pattern) = element
                .GetCurrentPattern(UIA_TogglePatternId)
                .and_then(|v| v.cast::<IUIAutomationTogglePattern>())
            {
                states["checked"] = pattern
                    .CurrentToggleState()
                    .map(|v| match v.0 {
                        0 => json!(false),
                        1 => json!(true),
                        2 => json!("mixed"),
                        _ => Value::Null,
                    })
                    .unwrap_or(Value::Null);
            }
        }
        if matches!(role, "treeitem" | "combobox") {
            if let Ok(pattern) = element
                .GetCurrentPattern(UIA_ExpandCollapsePatternId)
                .and_then(|v| v.cast::<IUIAutomationExpandCollapsePattern>())
            {
                states["expanded"] = pattern
                    .CurrentExpandCollapseState()
                    .map(|v| match v.0 {
                        0 => json!(false),
                        1 => json!(true),
                        2 => json!("partial"),
                        _ => Value::Null,
                    })
                    .unwrap_or(Value::Null);
            }
        }
        if matches!(role, "slider" | "progressbar" | "spinner") {
            if let Ok(pattern) = element
                .GetCurrentPattern(UIA_RangeValuePatternId)
                .and_then(|v| v.cast::<IUIAutomationRangeValuePattern>())
            {
                states["readOnly"] = pattern
                    .CurrentIsReadOnly()
                    .map(|v| json!(v.as_bool()))
                    .unwrap_or(Value::Null);
                states["value"] = pattern
                    .CurrentValue()
                    .map(|v| json!(v))
                    .unwrap_or(Value::Null);
                states["minimum"] = pattern
                    .CurrentMinimum()
                    .map(|v| json!(v))
                    .unwrap_or(Value::Null);
                states["maximum"] = pattern
                    .CurrentMaximum()
                    .map(|v| json!(v))
                    .unwrap_or(Value::Null);
            }
        }
    }
    states
}
impl Walk<'_> {
    fn limit(&mut self, reason: &str) {
        self.truncated = true;
        if !self.issues.iter().any(|v| v == reason) {
            self.issues.push(reason.into());
        }
    }
    fn stopped(&mut self) -> bool {
        if Instant::now() >= self.deadline {
            self.limit("deadline: observation was truncated");
            return true;
        }
        if self.raw >= MAX_RAW_NODES {
            self.limit("rawNodeLimit: observation was truncated");
            return true;
        }
        if self.retained >= self.max_nodes {
            self.limit("nodeLimit: observation was truncated");
            return true;
        }
        false
    }
    fn visit(
        &mut self,
        element: &IUIAutomationElement,
        raw_depth: usize,
        display_depth: usize,
    ) -> Vec<TreeNode> {
        if self.stopped() {
            return vec![];
        }
        self.raw += 1;
        if raw_depth > self.max_depth {
            self.limit("depthLimit: actual accessibility depth was truncated");
            return vec![];
        }
        let native_id = runtime_id(element).and_then(|id| {
            self.window_native
                .as_ref()
                .map(|window| format!("{window}/uia:{id}"))
        });
        if let Some(ref id) = native_id {
            if !self.seen.insert(id.clone()) {
                self.limit("cycle: repeated native accessibility reference");
                return vec![];
            }
        }
        let ctrl = unsafe {
            element
                .CachedControlType()
                .or_else(|_| element.CurrentControlType())
                .map(|v| v.0)
                .unwrap_or(50025)
        };
        let role = role(ctrl);
        // Unknown password status is filtered conservatively. No Name/Value/Text
        // has been requested/cached at this point.
        let protected = unsafe {
            element
                .CachedIsPassword()
                .or_else(|_| element.CurrentIsPassword())
                .map(|v| v.as_bool())
                .unwrap_or(true)
        };
        let name = if protected {
            String::new()
        } else {
            unsafe {
                element
                    .CurrentName()
                    .map(|n| bounded_text(n.to_string(), 512).0)
                    .unwrap_or_default()
            }
        };
        let offscreen = unsafe {
            element
                .CachedIsOffscreen()
                .or_else(|_| element.CurrentIsOffscreen())
                .map(|v| v.as_bool())
                .unwrap_or(false)
        };
        let enabled = unsafe {
            element
                .CachedIsEnabled()
                .or_else(|_| element.CurrentIsEnabled())
                .map(|v| v.as_bool())
                .unwrap_or(false)
        };
        let focused = unsafe {
            element
                .CachedHasKeyboardFocus()
                .or_else(|_| element.CurrentHasKeyboardFocus())
                .map(|v| v.as_bool())
                .unwrap_or(false)
        };
        let id = format!("@e{}", self.raw);
        let text = if protected {
            None
        } else {
            read_text(
                element,
                role,
                (if self.capture_text {
                    MAX_TEXT_CHARS
                } else {
                    180
                })
                .min(MAX_TOTAL_TEXT_CHARS.saturating_sub(self.text_chars)),
                &id,
                &name,
                offscreen,
            )
        };
        let mut text = text;
        if !self.capture_text {
            if let Some(ref mut buffer) = text {
                buffer.truncated = true;
            }
        }
        let meaningful = protected
            || !name.is_empty()
            || text.is_some()
            || !matches!(
                role,
                "pane" | "group" | "custom" | "separator" | "image" | "tooltip"
            );
        let node = meaningful.then(|| {
            self.retained += 1;
            TreeNode {
                id: id.clone(),
                role: role.into(),
                name,
                depth: display_depth,
                offscreen,
                focused,
                enabled,
                has_text: text.is_some(),
                text_source: text.as_ref().map(|t| t.source.clone()),
                total_chars: text.as_ref().map_or(0, |t| t.total_chars),
                offscreen_chars: text.as_ref().map_or(0, |t| t.offscreen_chars),
                preview: text.as_ref().map(|t| t.preview.clone()).unwrap_or_default(),
                children: vec![],
                protected,
                native_id,
                native_role: Some(format!("UIA.ControlType:{ctrl}")),
                states: readable_states(element, role, protected),
            }
        });
        if let Some(text) = text {
            self.text_chars += text.total_chars;
            if text.truncated && self.capture_text {
                self.limit("textLimit: one or more text buffers were truncated");
            }
            self.buffers.push(text);
        }
        let mut children = Vec::new();
        // Secure controls are leaves even if a provider exposes child objects.
        if !protected && !self.stopped() {
            let mut next = unsafe {
                self.walker
                    .GetFirstChildElementBuildCache(element, self.cache)
                    .ok()
            };
            while let Some(child) = next {
                children.extend(self.visit(
                    &child,
                    raw_depth + 1,
                    display_depth + usize::from(meaningful),
                ));
                if self.stopped() {
                    break;
                }
                next = unsafe {
                    self.walker
                        .GetNextSiblingElementBuildCache(&child, self.cache)
                        .ok()
                };
            }
        }
        if let Some(mut node) = node {
            node.children = children;
            vec![node]
        } else {
            children
        }
    }
}

pub fn inspect_window(
    target_id: &str,
    max_depth: usize,
    max_nodes: usize,
) -> Result<InspectResponse, String> {
    inspect_window_with_options(target_id, max_depth, max_nodes, true)
}

pub fn inspect_window_with_options(
    target_id: &str,
    max_depth: usize,
    max_nodes: usize,
    capture_text: bool,
) -> Result<InspectResponse, String> {
    if max_nodes == 0 {
        return Err("invalidLimits: max_nodes must be positive".into());
    }
    let target = target_id.to_owned();
    on_worker(move || {
        let start = Instant::now();
        let _apartment = ComApartment::new()?;
        let uia = init_uia()?;
        let (windows, _) = enumerate()?;
        let (selector, expected_runtime) = target
            .rsplit_once("/root:")
            .map_or((target.as_str(), None), |(base, runtime)| {
                (base, Some(runtime))
            });
        let mut window = choose_window(selector, &windows)?;
        let _application = AppPermit::acquire(window.pid)?;
        let element = unsafe { uia.ElementFromHandle(HWND(window.hwnd as *mut c_void)) }
            .map_err(|e| format!("targetUnavailable: selected window could not be read: {e}"))?;
        let actual_pid = unsafe { element.CurrentProcessId() }.map_err(|e| {
            format!("targetUnavailable: selected window process could not be read: {e}")
        })?;
        let current_window =
            native_window(window.hwnd, unsafe { GetForegroundWindow().0 as isize }).ok_or_else(
                || "staleReference: selected window disappeared during observation".to_string(),
            )?;
        if actual_pid as u32 != window.pid || current_window.native_id != window.native_id {
            return Err(
                "staleReference: selected window/process identity changed during observation"
                    .into(),
            );
        }
        let actual_runtime = runtime_id(&element);
        if let Some(expected) = expected_runtime {
            if actual_runtime.as_deref() != Some(expected) {
                return Err("staleReference: native accessibility window identity changed".into());
            }
        } else if target.starts_with("win:") {
            return Err("staleReference: native Windows references must include the returned root accessibility identity".into());
        }
        window.native_id = window
            .native_id
            .zip(actual_runtime)
            .map(|(base, runtime)| format!("{base}/root:{runtime}"));
        let walker = unsafe { uia.RawViewWalker() }
            .map_err(|e| format!("backendFailure: raw tree walker: {e}"))?;
        let cache = cache_request(&uia)?;
        let element = unsafe { element.BuildUpdatedCache(&cache) }
            .map_err(|e| format!("targetUnavailable: selected window cache: {e}"))?;
        if let Ok(offscreen) = unsafe {
            element
                .CachedIsOffscreen()
                .or_else(|_| element.CurrentIsOffscreen())
        } {
            window.offscreen = offscreen.as_bool();
        }
        let mut walk = Walk {
            walker: &walker,
            cache: &cache,
            deadline: start + OBSERVATION_TIMEOUT,
            max_depth: max_depth.min(MAX_DEPTH),
            max_nodes: max_nodes.min(MAX_NODES),
            raw: 0,
            retained: 0,
            text_chars: 0,
            buffers: vec![],
            seen: HashSet::new(),
            issues: vec![],
            truncated: false,
            window_native: window.native_id.clone(),
            capture_text,
        };
        let tree = walk.visit(&element, 0, 0);
        let mut yaml = vec![format!(
            "# AxiAccess observation — {}",
            serde_json::to_string(&window.title).unwrap_or_default()
        )];
        fn render(nodes: &[TreeNode], lines: &mut Vec<String>) {
            for node in nodes {
                lines.push(format!(
                    "{}- {} {} [{}]{}",
                    "  ".repeat(node.depth),
                    node.role,
                    serde_json::to_string(&node.name).unwrap_or_default(),
                    node.id,
                    if node.protected {
                        " (protected: text omitted)"
                    } else {
                        ""
                    }
                ));
                render(&node.children, lines);
            }
        }
        render(&tree, &mut yaml);
        let llm_yaml = yaml.join("\n");
        let offscreen_extracted_chars = walk.buffers.iter().map(|b| b.offscreen_chars).sum();
        Ok(InspectResponse {
            window: InspectWindowMeta {
                hwnd: window.hwnd,
                pid: window.pid,
                process: window.process,
                title: window.title,
                class_name: window.class_name,
                offscreen: window.offscreen,
            },
            stats: InspectStats {
                raw_nodes: walk.raw,
                semantic_nodes: walk.retained,
                compression_pct: if walk.raw == 0 {
                    0.0
                } else {
                    100.0 * (1.0 - walk.retained as f64 / walk.raw as f64)
                },
                text_buffers_found: walk.buffers.len(),
                total_extracted_chars: walk.text_chars,
                offscreen_extracted_chars,
                scan_time_ms: start.elapsed().as_secs_f64() * 1000.0,
                estimated_tokens: llm_yaml.len() / 4,
                issues: walk.issues,
                truncated: walk.truncated,
            },
            llm_yaml,
            text_buffers: walk.buffers,
            tree,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn window(hwnd: isize, pid: u32, title: &str) -> WindowInfo {
        WindowInfo {
            hwnd,
            pid,
            process: "fixture.exe".into(),
            title: title.into(),
            class_name: "Fixture".into(),
            offscreen: false,
            minimized: false,
            focused: false,
            bounds: WindowBounds {
                left: 0,
                top: 0,
                width: 1,
                height: 1,
            },
            native_id: Some(format!("win:{hwnd}:{pid}:123")),
            bounds_known: true,
            state_known: true,
        }
    }
    #[test]
    fn target_must_not_choose_first_ambiguous_match() {
        let windows = vec![window(10, 1, "Editor A"), window(11, 1, "Editor B")];
        assert!(choose_window("Editor", &windows)
            .unwrap_err()
            .starts_with("ambiguousTarget"));
        assert!(choose_window("1", &windows)
            .unwrap_err()
            .starts_with("ambiguousTarget"));
        assert_eq!(choose_window("10", &windows).unwrap().hwnd, 10);
    }
    #[test]
    fn stale_native_reference_does_not_fall_back() {
        assert!(choose_window("win:10:1:99", &[window(10, 1, "Editor")])
            .unwrap_err()
            .starts_with("staleReference"));
    }
    #[test]
    fn text_limits_keep_unicode_intact() {
        let (text, truncated) = bounded_text("a🦉é文".into(), 3);
        assert_eq!(text, "a🦉é");
        assert!(truncated);
    }
}
