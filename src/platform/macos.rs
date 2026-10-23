//! Observation-only macOS accessibility backend. There are deliberately no AX
//! setters/actions, event injection, application launches, or focus operations.
//! All copied CF objects are owned, AX calls have a messaging timeout, and the
//! caller's process worker provides the hard boundary for a stuck AX provider.
use crate::engine::{
    InspectResponse, InspectStats, InspectWindowMeta, TextBufferInfo, TreeNode, WindowBounds,
    WindowInfo, WindowListResponse,
};
use serde_json::{json, Value};
use std::{
    collections::{HashSet, VecDeque},
    ffi::{c_char, c_void, CString},
    ptr,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, SyncSender},
        Condvar, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

type Ref = *const c_void;
type Index = isize;
// Leave time for the last bounded AX call and serialization before the sender's
// hard three-second worker-process deadline.
const NATIVE_DEADLINE: Duration = Duration::from_millis(2400);
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Point {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Size {
    width: f64,
    height: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Range {
    location: Index,
    length: Index,
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRetain(value: Ref) -> Ref;
    fn CFRelease(value: Ref);
    fn CFGetTypeID(value: Ref) -> usize;
    fn CFEqual(a: Ref, b: Ref) -> bool;
    fn CFHash(value: Ref) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFStringCreateWithCString(allocator: Ref, text: *const c_char, encoding: u32) -> Ref;
    fn CFStringGetLength(value: Ref) -> Index;
    fn CFStringGetCharacters(value: Ref, range: Range, output: *mut u16);
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(value: Ref) -> bool;
    fn CFNumberGetTypeID() -> usize;
    fn CFNumberGetValue(value: Ref, kind: i32, output: *mut c_void) -> bool;
    fn CFArrayGetTypeID() -> usize;
    fn CFArrayCreate(allocator: Ref, values: *const Ref, count: Index, callbacks: Ref) -> Ref;
    fn CFArrayGetCount(array: Ref) -> Index;
    fn CFArrayGetValueAtIndex(array: Ref, index: Index) -> Ref;
    fn CFRunLoopGetCurrent() -> Ref;
    fn CFRunLoopAddSource(run_loop: Ref, source: Ref, mode: Ref);
    fn CFRunLoopRemoveSource(run_loop: Ref, source: Ref, mode: Ref);
    fn CFRunLoopRunInMode(mode: Ref, seconds: f64, return_after_source_handled: bool) -> i32;
    static kCFRunLoopDefaultMode: Ref;
}
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrustedWithOptions(options: Ref) -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> Ref;
    fn AXUIElementCreateSystemWide() -> Ref;
    fn AXUIElementGetTypeID() -> usize;
    fn AXUIElementSetMessagingTimeout(element: Ref, seconds: f32) -> i32;
    fn AXUIElementCopyAttributeValue(element: Ref, attribute: Ref, output: *mut Ref) -> i32;
    fn AXUIElementCopyAttributeValues(
        element: Ref,
        attribute: Ref,
        index: Index,
        count: Index,
        output: *mut Ref,
    ) -> i32;
    fn AXUIElementGetAttributeValueCount(element: Ref, attribute: Ref, output: *mut Index) -> i32;
    fn AXUIElementCopyMultipleAttributeValues(
        element: Ref,
        attributes: Ref,
        options: u32,
        output: *mut Ref,
    ) -> i32;
    fn AXUIElementCopyParameterizedAttributeValue(
        element: Ref,
        attribute: Ref,
        parameter: Ref,
        output: *mut Ref,
    ) -> i32;
    fn AXUIElementIsAttributeSettable(element: Ref, attribute: Ref, output: *mut bool) -> i32;
    fn AXValueGetTypeID() -> usize;
    fn AXValueGetType(value: Ref) -> i32;
    fn AXValueGetValue(value: Ref, kind: i32, output: *mut c_void) -> bool;
    fn AXValueCreate(kind: i32, value: *const c_void) -> Ref;
    fn AXObserverCreate(
        pid: i32,
        callback: unsafe extern "C" fn(Ref, Ref, Ref, *mut c_void),
        observer: *mut Ref,
    ) -> i32;
    fn AXObserverGetRunLoopSource(observer: Ref) -> Ref;
    fn AXObserverAddNotification(
        observer: Ref,
        element: Ref,
        notification: Ref,
        context: *mut c_void,
    ) -> i32;
}
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}
#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> *mut c_void;
    fn sel_registerName(name: *const c_char) -> *mut c_void;
    fn objc_msgSend();
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

struct Owned(Ref);
impl Owned {
    unsafe fn from_copy(value: Ref) -> Option<Self> {
        (!value.is_null()).then_some(Self(value))
    }
    unsafe fn retain(value: Ref) -> Option<Self> {
        if value.is_null() {
            None
        } else {
            Some(Self(CFRetain(value)))
        }
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.0);
        }
    }
}
// AXUIElement objects support cross-thread messaging. Registry operations and
// reads are serialized; no AppKit object crosses a thread or is retained here.
unsafe impl Send for Owned {}
struct Pool(*mut c_void);
impl Pool {
    fn new() -> Self {
        unsafe { Self(objc_autoreleasePoolPush()) }
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        unsafe {
            objc_autoreleasePoolPop(self.0);
        }
    }
}

fn cfstr(value: &str) -> Owned {
    let text = CString::new(value).expect("constant AX attribute has no NUL");
    unsafe {
        Owned::from_copy(CFStringCreateWithCString(
            ptr::null(),
            text.as_ptr(),
            0x08000100,
        ))
        .expect("CF string allocation")
    }
}
fn error_name(code: i32) -> &'static str {
    match code {
        -25200 => "failure",
        -25201 => "illegalArgument",
        -25202 => "invalidElement",
        -25203 => "invalidObserver",
        -25204 => "cannotComplete",
        -25205 => "attributeUnsupported",
        -25206 => "actionUnsupported",
        -25207 => "notificationUnsupported",
        -25208 => "notImplemented",
        -25209 => "notificationAlreadyRegistered",
        -25210 => "notificationNotRegistered",
        -25211 => "apiDisabled",
        -25212 => "noValue",
        -25213 => "parameterizedAttributeUnsupported",
        -25214 => "notEnoughPrecision",
        _ => "unknownAXError",
    }
}
fn ax_error(operation: &str, code: i32) -> String {
    format!("macOS AX {operation}: {} ({code})", error_name(code))
}
fn attr(element: Ref, name: &str) -> Result<Option<Owned>, String> {
    let key = cfstr(name);
    let mut value = ptr::null();
    let result = unsafe { AXUIElementCopyAttributeValue(element, key.0, &mut value) };
    if result == 0 {
        Ok(unsafe { Owned::from_copy(value) })
    } else if matches!(result, -25205 | -25212) {
        Ok(None)
    } else {
        Err(ax_error(name, result))
    }
}
fn string(value: Ref, max_units: usize) -> Option<(String, bool)> {
    if value.is_null() || unsafe { CFGetTypeID(value) != CFStringGetTypeID() } {
        return None;
    }
    let total = unsafe { CFStringGetLength(value) }.max(0) as usize;
    let count = total.min(max_units);
    let mut units = vec![0u16; count];
    unsafe {
        CFStringGetCharacters(
            value,
            Range {
                location: 0,
                length: count as Index,
            },
            units.as_mut_ptr(),
        );
    }
    if count < total && units.last().is_some_and(|v| (0xd800..=0xdbff).contains(v)) {
        units.pop();
    }
    Some((String::from_utf16_lossy(&units), count < total))
}
fn boolean(value: Ref) -> Option<bool> {
    if !value.is_null() && unsafe { CFGetTypeID(value) == CFBooleanGetTypeID() } {
        Some(unsafe { CFBooleanGetValue(value) })
    } else {
        None
    }
}
fn integer(value: Ref) -> Option<i64> {
    if value.is_null() || unsafe { CFGetTypeID(value) != CFNumberGetTypeID() } {
        return None;
    }
    let mut number = 0i64;
    unsafe { CFNumberGetValue(value, 4, (&mut number as *mut i64).cast()) }.then_some(number)
}
fn decimal(value: Ref) -> Option<f64> {
    if value.is_null() || unsafe { CFGetTypeID(value) != CFNumberGetTypeID() } {
        return None;
    }
    let mut number = 0f64;
    if unsafe { CFNumberGetValue(value, 13, (&mut number as *mut f64).cast()) }
        && number.is_finite()
    {
        Some(number)
    } else {
        None
    }
}
fn ax_struct<T: Default>(value: Ref, kind: i32) -> Option<T> {
    if value.is_null()
        || unsafe { CFGetTypeID(value) != AXValueGetTypeID() || AXValueGetType(value) != kind }
    {
        return None;
    }
    let mut output = T::default();
    unsafe { AXValueGetValue(value, kind, (&mut output as *mut T).cast()) }.then_some(output)
}

// Bulk metadata deliberately excludes AXValue. Secure fields are identified
// from their native subrole BEFORE any text or value read can be performed.
const METADATA: [&str; 18] = [
    "AXRole",
    "AXSubrole",
    "AXTitle",
    "AXDescription",
    "AXEnabled",
    "AXFocused",
    "AXNumberOfCharacters",
    "AXIdentifier",
    "AXMinimized",
    "AXPosition",
    "AXSize",
    "AXVisibleCharacterRange",
    "AXSelected",
    "AXExpanded",
    "AXMinValue",
    "AXMaxValue",
    "AXReadOnly",
    "AXDisclosing",
];
struct Metadata {
    array: Owned,
}
impl Metadata {
    fn read(element: Ref) -> Result<Self, String> {
        let keys: Vec<_> = METADATA.iter().map(|name| cfstr(name)).collect();
        let pointers: Vec<_> = keys.iter().map(|key| key.0).collect();
        let attributes = unsafe {
            Owned::from_copy(CFArrayCreate(
                ptr::null(),
                pointers.as_ptr(),
                pointers.len() as Index,
                ptr::null(),
            ))
            .unwrap()
        };
        let mut output = ptr::null();
        let code = unsafe {
            AXUIElementCopyMultipleAttributeValues(element, attributes.0, 0, &mut output)
        };
        if code != 0 {
            return Err(ax_error("bulk metadata", code));
        }
        let array = unsafe { Owned::from_copy(output) }
            .ok_or_else(|| "macOS AX bulk metadata returned no array".to_string())?;
        if unsafe { CFGetTypeID(array.0) != CFArrayGetTypeID() } {
            return Err("macOS AX bulk metadata returned a non-array".into());
        }
        Ok(Self { array })
    }
    fn get(&self, index: usize) -> Ref {
        if index >= unsafe { CFArrayGetCount(self.array.0) }.max(0) as usize {
            ptr::null()
        } else {
            unsafe { CFArrayGetValueAtIndex(self.array.0, index as Index) }
        }
    }
    fn text(&self, index: usize, cap: usize) -> String {
        string(self.get(index), cap)
            .map(|v| v.0)
            .unwrap_or_default()
    }
    fn boolean(&self, index: usize) -> Option<bool> {
        boolean(self.get(index))
    }
}
fn page(
    element: Ref,
    name: &str,
    offset: usize,
    maximum: usize,
) -> Result<(Vec<Owned>, usize), String> {
    let key = cfstr(name);
    let mut total: Index = 0;
    let result = unsafe { AXUIElementGetAttributeValueCount(element, key.0, &mut total) };
    if matches!(result, -25205 | -25212) {
        return Ok((Vec::new(), 0));
    }
    if result != 0 {
        return Err(ax_error(&format!("{name} count"), result));
    }
    let total = total.max(0) as usize;
    if offset >= total || maximum == 0 {
        return Ok((Vec::new(), total));
    }
    let mut value = ptr::null();
    let result = unsafe {
        AXUIElementCopyAttributeValues(
            element,
            key.0,
            offset as Index,
            maximum.min(total - offset) as Index,
            &mut value,
        )
    };
    if result != 0 {
        return Err(ax_error(&format!("{name} page"), result));
    }
    let array = unsafe { Owned::from_copy(value) }
        .ok_or_else(|| "macOS AX child page returned no array".to_string())?;
    if unsafe { CFGetTypeID(array.0) != CFArrayGetTypeID() } {
        return Err("macOS AX child page is not an array".into());
    }
    let mut values = Vec::new();
    for index in 0..unsafe { CFArrayGetCount(array.0) } {
        let value = unsafe { CFArrayGetValueAtIndex(array.0, index) };
        if value.is_null() || unsafe { CFGetTypeID(value) != AXUIElementGetTypeID() } {
            return Err(format!("macOS AX {name} returned a non-element child"));
        }
        if let Some(value) = unsafe { Owned::retain(value) } {
            values.push(value);
        }
    }
    Ok((values, total))
}

unsafe fn object(receiver: *mut c_void, selector: &str) -> *mut c_void {
    let name = CString::new(selector).unwrap();
    let sel = sel_registerName(name.as_ptr());
    let send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
        std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
    send(receiver, sel)
}
unsafe fn unsigned(receiver: *mut c_void, selector: &str) -> usize {
    let name = CString::new(selector).unwrap();
    let sel = sel_registerName(name.as_ptr());
    let send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize =
        std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
    send(receiver, sel)
}
unsafe fn indexed(receiver: *mut c_void, index: usize) -> *mut c_void {
    let sel = sel_registerName(c"objectAtIndex:".as_ptr());
    let send: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void =
        std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
    send(receiver, sel, index)
}
unsafe fn number(receiver: *mut c_void, selector: &str) -> f64 {
    let name = CString::new(selector).unwrap();
    let sel = sel_registerName(name.as_ptr());
    let send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> f64 =
        std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
    send(receiver, sel)
}
#[derive(Clone)]
struct App {
    pid: u32,
    name: String,
    launch: u64,
    focused: bool,
}
fn running_apps() -> Result<Vec<App>, String> {
    let _pool = Pool::new();
    unsafe {
        let class = objc_getClass(c"NSWorkspace".as_ptr());
        if class.is_null() {
            return Err("macOS AppKit NSWorkspace is unavailable".into());
        }
        let workspace = object(class, "sharedWorkspace");
        let applications = object(workspace, "runningApplications");
        if applications.is_null() {
            return Err("macOS graphical session application inventory unavailable".into());
        }
        let front = object(workspace, "frontmostApplication");
        let front_pid = if front.is_null() {
            0
        } else {
            unsigned(front, "processIdentifier") as u32
        };
        let mut result = Vec::new();
        for i in 0..unsigned(applications, "count").min(512) {
            let app = indexed(applications, i);
            let pid = unsigned(app, "processIdentifier") as u32;
            if pid == 0 || unsigned(app, "isTerminated") != 0 {
                continue;
            }
            // Background-only applications cannot own an ordinary desktop window.
            if unsigned(app, "activationPolicy") == 2 {
                continue;
            }
            let name = string(object(app, "localizedName").cast(), 256)
                .map(|v| v.0)
                .unwrap_or_else(|| format!("pid-{pid}"));
            let launch = object(app, "launchDate");
            let launch = if launch.is_null() {
                0
            } else {
                number(launch, "timeIntervalSince1970").to_bits()
            };
            result.push(App {
                pid,
                name,
                launch,
                focused: pid == front_pid,
            });
        }
        // The foreground application is useful even under a bounded inventory.
        result.sort_by_key(|app| (!app.focused, app.pid));
        Ok(result)
    }
}

static EVENT_REVISION: AtomicU64 = AtomicU64::new(1);
static EVENT_APPS: AtomicUsize = AtomicUsize::new(0);
static EVENT_REGISTRATIONS: AtomicUsize = AtomicUsize::new(0);
static EVENT_DROPPED: AtomicUsize = AtomicUsize::new(0);
static EVENT_ERRORS: AtomicUsize = AtomicUsize::new(0);
fn event_wake() -> &'static (Mutex<()>, Condvar) {
    static VALUE: OnceLock<(Mutex<()>, Condvar)> = OnceLock::new();
    VALUE.get_or_init(|| (Mutex::new(()), Condvar::new()))
}
enum EventCommand {
    Applications(Vec<App>),
    Watch { pid: u32, element: Owned },
}
struct Observer {
    pid: u32,
    launch: u64,
    observer: Owned,
    _application: Owned,
    watched: HashSet<usize>,
}
unsafe extern "C" fn observed_change(_: Ref, _: Ref, _: Ref, _: *mut c_void) {
    // Notifications are invalidation hints only. No app text is captured in a
    // callback, and no full tree is traversed when a notification arrives.
    EVENT_REVISION.fetch_add(1, Ordering::Relaxed);
    let (mutex, signal) = event_wake();
    if let Ok(_guard) = mutex.lock() {
        signal.notify_all();
    }
}
fn event_channel() -> &'static SyncSender<EventCommand> {
    static CHANNEL: OnceLock<SyncSender<EventCommand>> = OnceLock::new();
    CHANNEL.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel::<EventCommand>(64);
        std::thread::Builder::new()
            .name("axiaccess-ax-observer".into())
            .spawn(move || {
                let _pool = Pool::new();
                let run_loop = unsafe { CFRunLoopGetCurrent() };
                let mut observers: Vec<Observer> = Vec::new();
                loop {
                    for _ in 0..64 {
                        let command = match receiver.try_recv() {
                            Ok(command) => command,
                            Err(mpsc::TryRecvError::Empty) => break,
                            Err(mpsc::TryRecvError::Disconnected) => return,
                        };
                        match command {
                            EventCommand::Applications(apps) => {
                                observers.retain(|observer| {
                                    let retain = apps.iter().any(|app| {
                                        app.pid == observer.pid && app.launch == observer.launch
                                    });
                                    if !retain {
                                        unsafe {
                                            CFRunLoopRemoveSource(
                                                run_loop,
                                                AXObserverGetRunLoopSource(observer.observer.0),
                                                kCFRunLoopDefaultMode,
                                            );
                                        }
                                    }
                                    retain
                                });
                                for app in apps.iter().take(64) {
                                    if observers.iter().any(|observer| {
                                        observer.pid == app.pid && observer.launch == app.launch
                                    }) {
                                        continue;
                                    }
                                    let mut observer = ptr::null();
                                    let code = unsafe {
                                        AXObserverCreate(
                                            app.pid as i32,
                                            observed_change,
                                            &mut observer,
                                        )
                                    };
                                    if code != 0 {
                                        EVENT_ERRORS.fetch_add(1, Ordering::Relaxed);
                                        continue;
                                    }
                                    let observer = match unsafe { Owned::from_copy(observer) } {
                                        Some(value) => value,
                                        None => continue,
                                    };
                                    let application = match unsafe {
                                        Owned::from_copy(AXUIElementCreateApplication(
                                            app.pid as i32,
                                        ))
                                    } {
                                        Some(value) => value,
                                        None => continue,
                                    };
                                    unsafe {
                                        AXUIElementSetMessagingTimeout(application.0, 0.25);
                                    }
                                    for name in [
                                        "AXFocusedUIElementChanged",
                                        "AXFocusedWindowChanged",
                                        "AXWindowCreated",
                                        "AXUIElementDestroyed",
                                        "AXApplicationShown",
                                        "AXApplicationHidden",
                                    ] {
                                        register_notification(observer.0, application.0, name);
                                    }
                                    unsafe {
                                        CFRunLoopAddSource(
                                            run_loop,
                                            AXObserverGetRunLoopSource(observer.0),
                                            kCFRunLoopDefaultMode,
                                        );
                                    }
                                    observers.push(Observer {
                                        pid: app.pid,
                                        launch: app.launch,
                                        observer,
                                        _application: application,
                                        watched: HashSet::new(),
                                    });
                                }
                                EVENT_APPS.store(observers.len(), Ordering::Relaxed);
                            }
                            EventCommand::Watch { pid, element } => {
                                if let Some(observer) =
                                    observers.iter_mut().find(|observer| observer.pid == pid)
                                {
                                    let hash = unsafe { CFHash(element.0) };
                                    if observer.watched.len() >= 256
                                        || !observer.watched.insert(hash)
                                    {
                                        continue;
                                    }
                                    for name in [
                                        "AXValueChanged",
                                        "AXSelectedTextChanged",
                                        "AXTitleChanged",
                                        "AXMoved",
                                        "AXResized",
                                        "AXUIElementDestroyed",
                                    ] {
                                        register_notification(observer.observer.0, element.0, name);
                                    }
                                }
                            }
                        }
                    }
                    unsafe {
                        CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.25, true);
                    }
                }
            })
            .expect("native AX observer worker can start");
        sender
    })
}
fn register_notification(observer: Ref, element: Ref, name: &str) {
    let key = cfstr(name);
    let code = unsafe { AXObserverAddNotification(observer, element, key.0, ptr::null_mut()) };
    if code == 0 {
        EVENT_REGISTRATIONS.fetch_add(1, Ordering::Relaxed);
    } else if !matches!(code, -25207 | -25209 | -25212) {
        EVENT_ERRORS.fetch_add(1, Ordering::Relaxed);
    }
}
fn watch_node(pid: u32, element: Ref) {
    if let Some(element) = unsafe { Owned::retain(element) } {
        if event_channel()
            .try_send(EventCommand::Watch { pid, element })
            .is_err()
        {
            EVENT_DROPPED.fetch_add(1, Ordering::Relaxed);
        }
    }
}
pub fn native_event_revision() -> u64 {
    static INITIAL: OnceLock<()> = OnceLock::new();
    if unsafe { AXIsProcessTrustedWithOptions(ptr::null()) } && configure_timeout().is_ok() {
        INITIAL.get_or_init(|| {
            if let Ok(apps) = running_apps() {
                let _ = event_channel().try_send(EventCommand::Applications(apps));
            }
        });
    }
    EVENT_REVISION.load(Ordering::Relaxed)
}
pub fn wait_native_events(since: u64, timeout: Duration) -> u64 {
    let _ = native_event_revision();
    let (mutex, signal) = event_wake();
    if let Ok(guard) = mutex.lock() {
        let _guard = signal.wait_timeout_while(guard, timeout.min(Duration::from_secs(30)), |_| {
            EVENT_REVISION.load(Ordering::Relaxed) == since
        });
    }
    EVENT_REVISION.load(Ordering::Relaxed)
}
pub fn native_event_status() -> Value {
    json!({"mode":"AXObserver", "revision":native_event_revision(), "subscribedApplications":EVENT_APPS.load(Ordering::Relaxed),
        "successfulRegistrations":EVENT_REGISTRATIONS.load(Ordering::Relaxed),"registrationErrors":EVENT_ERRORS.load(Ordering::Relaxed),
        "droppedSubscriptionRequests":EVENT_DROPPED.load(Ordering::Relaxed),"coverage":"application notifications and inspected elements; unsupported notifications require bounded reconciliation",
        "maximumApplications":64,"maximumWatchedElementsPerApplication":256,"fullTreePolling":false})
}
struct Window {
    id: isize,
    app: App,
    element: Owned,
    info: WindowInfo,
}
#[derive(Default)]
struct Registry {
    next: isize,
    windows: Vec<Window>,
}
fn registry() -> &'static Mutex<Registry> {
    static VALUE: OnceLock<Mutex<Registry>> = OnceLock::new();
    VALUE.get_or_init(|| Mutex::new(Registry::default()))
}
struct ElementIdentity {
    window: isize,
    hash: usize,
    reference: Owned,
    id: u64,
}
#[derive(Default)]
struct ElementIdentities {
    next: u64,
    elements: VecDeque<ElementIdentity>,
}
fn element_identity(window: isize, element: Ref) -> Option<String> {
    static IDENTITIES: OnceLock<Mutex<ElementIdentities>> = OnceLock::new();
    let mut identities = IDENTITIES
        .get_or_init(|| Mutex::new(ElementIdentities::default()))
        .lock()
        .ok()?;
    let hash = unsafe { CFHash(element) };
    if let Some(index) = identities.elements.iter().position(|entry| {
        entry.window == window
            && entry.hash == hash
            && unsafe { CFEqual(entry.reference.0, element) }
    }) {
        let entry = identities.elements.remove(index)?;
        let id = entry.id;
        identities.elements.push_back(entry);
        return Some(format!("ax-element:{window}:{id}"));
    }
    identities.next = identities.next.checked_add(1)?;
    let id = identities.next;
    if identities.elements.len() >= 4096 {
        identities.elements.pop_front();
    }
    identities.elements.push_back(ElementIdentity {
        window,
        hash,
        reference: unsafe { Owned::retain(element) }?,
        id,
    });
    Some(format!("ax-element:{window}:{id}"))
}
fn trust() -> Result<(), String> {
    if unsafe { AXIsProcessTrustedWithOptions(ptr::null()) } {
        configure_timeout()
    } else {
        Err("permissionDenied: Grant Accessibility permission to the installed AxiAccess Sender executable in System Settings > Privacy & Security > Accessibility; permission is not requested or changed by observation".into())
    }
}
fn configure_timeout() -> Result<(), String> {
    static RESULT: OnceLock<Result<(), String>> = OnceLock::new();
    RESULT
        .get_or_init(|| {
            let system = unsafe { Owned::from_copy(AXUIElementCreateSystemWide()) }
                .ok_or_else(|| "macOS AX system object allocation failed".to_string())?;
            // Apple's SDK specifies that a timeout on an individual object does not
            // apply to equal objects or descendants. The system-wide object changes
            // only this observing process's default, never another app's settings.
            let code = unsafe { AXUIElementSetMessagingTimeout(system.0, 0.25) };
            if code == 0 {
                Ok(())
            } else {
                Err(ax_error("process-wide messaging timeout", code))
            }
        })
        .clone()
}
fn bounds(metadata: &Metadata) -> (WindowBounds, bool) {
    let position: Option<Point> = ax_struct(metadata.get(9), 1);
    let size: Option<Size> = ax_struct(metadata.get(10), 2);
    if let (Some(position), Some(size)) = (position, size) {
        if [position.x, position.y, size.width, size.height]
            .iter()
            .all(|v| v.is_finite())
        {
            return (
                WindowBounds {
                    left: position.x.round() as i32,
                    top: position.y.round() as i32,
                    width: size.width.max(0.0).round() as i32,
                    height: size.height.max(0.0).round() as i32,
                },
                true,
            );
        }
    }
    (
        WindowBounds {
            left: 0,
            top: 0,
            width: 0,
            height: 0,
        },
        false,
    )
}

pub fn list_windows() -> Result<WindowListResponse, String> {
    trust()?;
    let started = Instant::now();
    let apps = running_apps()?;
    if event_channel()
        .try_send(EventCommand::Applications(apps.clone()))
        .is_err()
    {
        EVENT_DROPPED.fetch_add(1, Ordering::Relaxed);
    }
    let mut registry = registry()
        .lock()
        .map_err(|_| "macOS AX window registry unavailable")?;
    let old = std::mem::take(&mut registry.windows);
    let mut next = Vec::new();
    let mut issues = Vec::new();
    let mut truncated = apps.len() >= 512;
    for app in apps {
        if started.elapsed() >= NATIVE_DEADLINE || next.len() >= 512 {
            truncated = true;
            break;
        }
        if app.launch == 0 {
            issues.push(format!("pid {}: application launch identity is unavailable; window references were not issued", app.pid));
            continue;
        }
        let root = unsafe { Owned::from_copy(AXUIElementCreateApplication(app.pid as i32)) }
            .ok_or_else(|| "macOS AX application allocation failed".to_string())?;
        let result = unsafe { AXUIElementSetMessagingTimeout(root.0, 0.25) };
        if result != 0 {
            issues.push(ax_error("messaging timeout", result));
            continue;
        }
        let focused_window = if app.focused {
            attr(root.0, "AXFocusedWindow").ok().flatten()
        } else {
            None
        };
        let mut offset = 0;
        loop {
            if started.elapsed() >= NATIVE_DEADLINE || next.len() >= 512 {
                truncated = true;
                break;
            }
            let (windows, total) = match page(root.0, "AXWindows", offset, 32) {
                Ok(value) => value,
                Err(error) => {
                    issues.push(format!("pid {}: {error}", app.pid));
                    break;
                }
            };
            if windows.is_empty() {
                break;
            }
            offset += windows.len();
            for element in windows {
                if started.elapsed() >= NATIVE_DEADLINE || next.len() >= 512 {
                    truncated = true;
                    break;
                }
                let result = unsafe { AXUIElementSetMessagingTimeout(element.0, 0.25) };
                if result != 0 {
                    issues.push(ax_error("window messaging timeout", result));
                    continue;
                }
                let meta = match Metadata::read(element.0) {
                    Ok(value) => value,
                    Err(error) => {
                        issues.push(format!("pid {}: {error}", app.pid));
                        continue;
                    }
                };
                let id = old
                    .iter()
                    .find(|window| {
                        window.app.pid == app.pid
                            && window.app.launch == app.launch
                            && unsafe { CFEqual(window.element.0, element.0) }
                    })
                    .map(|window| window.id)
                    .unwrap_or_else(|| {
                        registry.next = registry.next.checked_add(1).unwrap_or(1);
                        registry.next
                    });
                let (bounds, bounds_known) = bounds(&meta);
                let minimized = meta.boolean(8);
                let info = WindowInfo {
                    hwnd: id,
                    pid: app.pid,
                    process: app.name.clone(),
                    title: meta.text(2, 512),
                    class_name: meta.text(0, 80),
                    offscreen: false,
                    minimized: minimized.unwrap_or(false),
                    focused: app.focused
                        && focused_window
                            .as_ref()
                            .map(|focused| unsafe { CFEqual(focused.0, element.0) })
                            .unwrap_or_else(|| meta.boolean(5).unwrap_or(false)),
                    bounds,
                    native_id: Some(format!("ax-window:{}:{:x}:{id}", app.pid, app.launch)),
                    bounds_known,
                    // AX does not provide generic, reliable occlusion/viewport state.
                    state_known: false,
                };
                next.push(Window {
                    id,
                    app: app.clone(),
                    element,
                    info,
                });
            }
            if offset >= total {
                break;
            }
        }
    }
    let windows = next.iter().map(|window| window.info.clone()).collect();
    registry.windows = next;
    truncated |= !issues.is_empty();
    if issues.len() > 32 {
        issues.truncate(32);
        issues.push("Additional per-application AX issues omitted".into());
    }
    Ok(WindowListResponse {
        platform: "macOS (native AXUIElement)".into(),
        // macOS AX trust is reported separately. Do not pretend VoiceOver or a
        // Windows-style global screen-reader flag is enabled on this platform.
        screen_reader_flag: false,
        scan_time_ms: started.elapsed().as_secs_f64() * 1000.0,
        count: registry.windows.len(),
        windows,
        issues,
        truncated,
    })
}

fn normalized_role(native: &str) -> String {
    match native {
        "AXWindow" | "AXSheet" => "window",
        "AXApplication" => "application",
        "AXButton" => "button",
        "AXPopUpButton" | "AXComboBox" => "combobox",
        "AXCheckBox" => "checkbox",
        "AXRadioButton" => "radiobutton",
        "AXTextField" => "edit",
        "AXTextArea" | "AXWebArea" => "document",
        "AXStaticText" => "text",
        "AXLink" => "link",
        "AXImage" => "image",
        "AXList" => "list",
        "AXOutline" => "tree",
        "AXRow" => "treeitem",
        "AXTable" => "table",
        "AXTabGroup" => "tab",
        "AXMenuBar" => "menubar",
        "AXMenuBarItem" | "AXMenuItem" => "menuitem",
        "AXMenu" => "menu",
        "AXToolbar" => "toolbar",
        "AXScrollArea" | "AXLayoutArea" | "AXDrawer" => "pane",
        "AXGroup" | "AXSplitGroup" => "group",
        "AXSlider" | "AXValueIndicator" => "slider",
        "AXProgressIndicator" => "progressbar",
        "AXScrollBar" => "scrollbar",
        "AXSplitter" => "separator",
        other => return other.strip_prefix("AX").unwrap_or(other).to_lowercase(),
    }
    .into()
}
struct Walk {
    started: Instant,
    window_id: isize,
    pid: u32,
    capture_text: bool,
    max_depth: usize,
    max_nodes: usize,
    raw: usize,
    retained: usize,
    text_budget: usize,
    buffers: Vec<TextBufferInfo>,
    seen: HashSet<usize>,
    issues: Vec<String>,
    truncated: bool,
}
impl Walk {
    fn states(&mut self, element: Ref, meta: &Metadata, role: &str, protected: bool) -> Value {
        if self.started.elapsed() >= NATIVE_DEADLINE {
            self.truncated = true;
            return json!({"protected":protected,"available":false});
        }
        let mut read_only = meta.boolean(16);
        if read_only.is_none() && !protected && matches!(role, "edit" | "document") {
            let key = cfstr("AXValue");
            let mut settable = false;
            let code = unsafe { AXUIElementIsAttributeSettable(element, key.0, &mut settable) };
            if code == 0 {
                read_only = Some(!settable);
            } else if !matches!(code, -25205 | -25212) {
                self.issues.push(ax_error("AXValue settable query", code));
            }
        }
        let mut checked = Value::Null;
        let mut numeric = None;
        if self.started.elapsed() < NATIVE_DEADLINE
            && !protected
            && matches!(role, "checkbox" | "radiobutton" | "slider" | "progressbar")
        {
            match attr(element, "AXValue") {
                Ok(Some(value)) => {
                    if matches!(role, "checkbox" | "radiobutton") {
                        checked = if let Some(value) = boolean(value.0) {
                            json!(value)
                        } else {
                            match integer(value.0) {
                                Some(0) => json!(false),
                                Some(1) => json!(true),
                                Some(2) => json!("mixed"),
                                _ => Value::Null,
                            }
                        };
                    } else {
                        numeric = decimal(value.0);
                    }
                }
                Err(error) => self.issues.push(error),
                _ => (),
            }
        }
        json!({"enabled":meta.boolean(4),"focused":meta.boolean(5),"selected":meta.boolean(12),
            "expanded":meta.boolean(13).or_else(|| meta.boolean(17)),"minimized":meta.boolean(8),"offscreen":Value::Null,
            "readOnly":read_only,"checked":checked,"value":numeric,"minimum":decimal(meta.get(14)),"maximum":decimal(meta.get(15)),
            "protected":protected,"nativeSubrole":meta.text(1,80)})
    }
    fn stop(&mut self, depth: usize) -> bool {
        let stop = depth > self.max_depth
            || self.raw >= 5000
            || self.retained >= self.max_nodes
            || self.started.elapsed() >= NATIVE_DEADLINE;
        self.truncated |= stop;
        stop
    }
    fn read_text(
        &mut self,
        element: Ref,
        meta: &Metadata,
        role: &str,
        id: &str,
        name: &str,
    ) -> Option<TextBufferInfo> {
        if self.started.elapsed() >= NATIVE_DEADLINE {
            self.truncated = true;
            return None;
        }
        if self.text_budget == 0 {
            self.truncated = true;
            return None;
        }
        let max_units = self
            .text_budget
            .min(if self.capture_text { 8192 } else { 180 });
        let native_total = integer(meta.get(6))
            .filter(|value| *value >= 0)
            .map(|value| value as usize);
        let mut text = None;
        if let Some(total) = native_total.filter(|value| *value > 0) {
            let range = Range {
                location: 0,
                length: total.min(max_units) as Index,
            };
            let parameter =
                unsafe { Owned::from_copy(AXValueCreate(4, (&range as *const Range).cast())) }?;
            let key = cfstr("AXStringForRange");
            let mut value = ptr::null();
            let code = unsafe {
                AXUIElementCopyParameterizedAttributeValue(element, key.0, parameter.0, &mut value)
            };
            if code == 0 {
                if let Some(value) = unsafe { Owned::from_copy(value) } {
                    text = string(value.0, max_units)
                        .map(|(value, cut)| (value, cut || total > max_units, "AXStringForRange"));
                }
            } else if !matches!(code, -25213 | -25205 | -25212) {
                self.issues.push(ax_error("AXStringForRange", code));
            }
        }
        if self.started.elapsed() < NATIVE_DEADLINE
            && text.is_none()
            && (self.capture_text && matches!(role, "document" | "edit" | "text") || role == "text")
        {
            match attr(element, "AXValue") {
                Ok(Some(value)) => {
                    text = string(value.0, max_units).map(|(value, cut)| (value, cut, "AXValue"));
                }
                Err(error) => self.issues.push(error),
                _ => (),
            }
        }
        if text.is_none() && !self.capture_text && matches!(role, "document" | "edit") {
            text = Some((String::new(), true, "AXDeferredText"));
        }
        let (mut full_text, mut truncated, source) = text?;
        truncated |= !self.capture_text;
        if full_text.is_empty() && self.capture_text {
            return None;
        }
        while full_text.len() > self.text_budget {
            full_text.pop();
            truncated = true;
        }
        self.text_budget = self.text_budget.saturating_sub(full_text.len());
        let visible_range: Option<Range> = ax_struct(meta.get(11), 4);
        let (visible_text, visibility_known) = if let Some(range) =
            visible_range.filter(|range| range.location >= 0 && range.length >= 0)
        {
            let start = range.location as usize;
            // AX ranges use UTF-16 units. Restrict to the captured prefix and
            // decode independently; never treat character indices as UTF-8 bytes.
            let units: Vec<u16> = full_text.encode_utf16().collect();
            let end = start.saturating_add(range.length as usize).min(units.len());
            if start <= end {
                (String::from_utf16_lossy(&units[start..end]), true)
            } else {
                (String::new(), true)
            }
        } else {
            (String::new(), false)
        };
        let total_chars = full_text.chars().count();
        let visible_chars = visible_text.chars().count();
        let preview = full_text.chars().take(180).collect::<String>();
        Some(TextBufferInfo {
            id: id.into(),
            role: role.into(),
            name: name.into(),
            source: source.into(),
            total_chars,
            visible_chars,
            offscreen_chars: if visibility_known {
                total_chars.saturating_sub(visible_chars)
            } else {
                0
            },
            total_lines: full_text.lines().count(),
            visible_lines: visible_text.lines().count(),
            preview,
            full_text,
            visible_text,
            truncated,
            protected: false,
            visibility_known,
        })
    }
    fn node(&mut self, element: Ref, depth: usize, path: &str) -> Option<TreeNode> {
        if self.stop(depth) {
            return None;
        }
        self.raw += 1;
        let hash = unsafe { CFHash(element) };
        if !self.seen.insert(hash) {
            self.issues.push(
                "AX repeated/cyclic node omitted; native hash is not used as a selectable identity"
                    .into(),
            );
            self.truncated = true;
            return None;
        }
        let meta = match Metadata::read(element) {
            Ok(value) => value,
            Err(error) => {
                self.issues.push(error);
                self.truncated = true;
                return None;
            }
        };
        let native_role = meta.text(0, 80);
        let subrole = meta.text(1, 80);
        let role = normalized_role(&native_role);
        let protected = subrole == "AXSecureTextField" || native_role == "AXSecureTextField";
        let name = if protected {
            "[protected]".into()
        } else {
            let title = meta.text(2, 256);
            if title.is_empty() {
                meta.text(3, 256)
            } else {
                title
            }
        };
        self.retained += 1;
        // Native AXIdentifiers are app-defined and can repeat. Retain and compare
        // exact native AX objects instead; bounded LRU eviction yields a new ID
        // and therefore an explicit stale buffer rather than an unrelated match.
        let native_id = element_identity(self.window_id, element);
        let id = native_id
            .clone()
            .unwrap_or_else(|| format!("ax:{}:{path}", self.window_id));
        let text = if protected {
            None
        } else {
            self.read_text(element, &meta, &role, &id, &name)
        };
        let states = self.states(element, &meta, &role, protected);
        let mut node = TreeNode {
            id,
            role,
            name,
            depth,
            offscreen: false,
            focused: meta.boolean(5).unwrap_or(false),
            enabled: meta.boolean(4).unwrap_or(false),
            has_text: text.is_some(),
            text_source: text.as_ref().map(|value| value.source.clone()),
            total_chars: text.as_ref().map(|value| value.total_chars).unwrap_or(0),
            offscreen_chars: text
                .as_ref()
                .map(|value| value.offscreen_chars)
                .unwrap_or(0),
            preview: text
                .as_ref()
                .map(|value| value.preview.clone())
                .unwrap_or_default(),
            children: Vec::new(),
            protected,
            native_id,
            native_role: (!native_role.is_empty()).then_some(native_role),
            states,
        };
        if let Some(text) = text {
            self.buffers.push(text);
        }
        // A secure subtree is not visited, even if the provider exposes children.
        if protected {
            return Some(node);
        }
        watch_node(self.pid, element);
        let mut offset = 0;
        loop {
            if self.stop(depth + 1) {
                break;
            }
            let (children, total) = match page(element, "AXChildren", offset, 32) {
                Ok(value) => value,
                Err(error) => {
                    self.issues.push(error);
                    self.truncated = true;
                    break;
                }
            };
            if children.is_empty() {
                break;
            }
            for (index, child) in children.iter().enumerate() {
                if let Some(child) =
                    self.node(child.0, depth + 1, &format!("{path}.{}", offset + index))
                {
                    node.children.push(child);
                }
                if self.stop(depth + 1) {
                    break;
                }
            }
            offset += children.len();
            if offset >= total {
                break;
            }
        }
        Some(node)
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
    trust()?;
    let started = Instant::now();
    let registry = registry()
        .lock()
        .map_err(|_| "macOS AX window registry unavailable")?;
    let numeric = target_id
        .strip_prefix("hwnd:")
        .unwrap_or(target_id)
        .parse::<isize>()
        .ok();
    let window = registry
        .windows
        .iter()
        .find(|window| Some(window.id) == numeric || window.info.native_id.as_deref() == Some(target_id))
        .ok_or_else(|| {
            "staleTarget: macOS inspection requires the exact current window reference from access_windows; title/PID matching is not used".to_string()
        })?;
    let id = window.id;
    let applications = running_apps()?;
    if !applications
        .iter()
        .any(|app| app.pid == window.app.pid && app.launch == window.app.launch)
    {
        return Err(
            "staleTarget: the macOS application lifetime changed; refresh access_windows".into(),
        );
    }
    // Validate the retained exact native object. Never replace it by first window
    // or a title match if the app closed it or reused its PID.
    let _ = Metadata::read(window.element.0)?;
    let mut walk = Walk {
        started,
        window_id: id,
        pid: window.app.pid,
        capture_text,
        max_depth: max_depth.min(64),
        max_nodes: max_nodes.clamp(1, 250),
        raw: 0,
        retained: 0,
        text_budget: 48 * 1024,
        buffers: Vec::new(),
        seen: HashSet::new(),
        issues: Vec::new(),
        truncated: false,
    };
    let tree: Vec<_> = walk.node(window.element.0, 0, "0").into_iter().collect();
    if tree.is_empty() {
        return Err(walk
            .issues
            .first()
            .cloned()
            .unwrap_or_else(|| "macOS AX inspection produced no root element".into()));
    }
    // JSON is YAML-compatible and uses a real serializer for all untrusted text.
    let llm_yaml = serde_json::to_string(&tree).map_err(|error| error.to_string())?;
    if walk.issues.len() > 32 {
        walk.issues.truncate(32);
    }
    let stats = InspectStats {
        raw_nodes: walk.raw,
        semantic_nodes: walk.retained,
        compression_pct: 0.0,
        text_buffers_found: walk.buffers.len(),
        total_extracted_chars: walk.buffers.iter().map(|value| value.total_chars).sum(),
        offscreen_extracted_chars: walk.buffers.iter().map(|value| value.offscreen_chars).sum(),
        scan_time_ms: started.elapsed().as_secs_f64() * 1000.0,
        estimated_tokens: llm_yaml.len().div_ceil(4),
        issues: walk.issues,
        truncated: walk.truncated
            || capture_text && walk.buffers.iter().any(|value| value.truncated),
    };
    Ok(InspectResponse {
        window: InspectWindowMeta {
            hwnd: window.id,
            pid: window.app.pid,
            process: window.app.name.clone(),
            title: window.info.title.clone(),
            class_name: window.info.class_name.clone(),
            offscreen: false,
        },
        stats,
        llm_yaml,
        text_buffers: walk.buffers,
        tree,
    })
}

pub fn capabilities() -> Value {
    let trusted = unsafe { AXIsProcessTrustedWithOptions(ptr::null()) };
    json!({"backend":"AXUIElement", "nativeRust":true, "pythonRequired":false,
        "permissionGranted":trusted, "permissionStatus":if trusted {"granted"} else {"accessibilityPermissionRequired"}, "screenReaderFlagSupported":false,
        "permissionPrompted":false, "readOnly":true, "nativeEvents":EVENT_REGISTRATIONS.load(Ordering::Relaxed)>0, "supportsNativeEvents":true,
        "eventMode":"AXObserver invalidation with bounded on-demand reconciliation", "events":native_event_status(), "applicationActivation":false,
        "windows":"selected graphical user session AXWindows", "offscreenText":"when exposed by the application",
        "offsetUnits":"AX text ranges use UTF-16 code units; returned counts use Unicode scalar values",
        "visibility":"unknown unless AXVisibleCharacterRange is exposed; generic occlusion is unknown",
        "limits":{"messagingTimeoutMs":250,"nativeWorkDeadlineMs":2400,"workerDeadlineMs":3000,"rawNodes":5000,"nodes":250,"depth":64,"textBytes":49152,"retainedNativeElements":4096},
        "protectedFields":"AXSecureTextField values and descendants excluded before text extraction",
        "stuckProviderRecovery":"sender worker process isolation", "uiActions":false})
}
