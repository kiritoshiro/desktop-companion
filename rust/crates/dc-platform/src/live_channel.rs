//! Port of app/live_channel.py: the two-way live channel between the
//! settings window and the running overlay.
//!
//! The Python used `QLocalServer`/`QLocalSocket`; this is a Windows named
//! pipe (`\\.\pipe\<name>`, the same namespace QLocalServer used on
//! Windows) carrying the same newline-delimited JSON messages:
//!
//! | type             | direction | payload                                   |
//! |------------------|-----------|-------------------------------------------|
//! | `hello`          | C -> S    | none, sent right after connecting          |
//! | `session_state`  | S -> C    | `state`: `CreatureManager.session_snapshot()` |
//! | `preset_update`  | C -> S    | `preset_path`, `data` (a full preset)      |
//! | `stop_request`   | C -> S    | none: save and quit                        |
//! | `reset_progress` | C -> S    | none: forget saved stats and bases (DC-85) |
//!
//! Every message carries `"v": 1`. The server drops a client that sends any
//! other version; the client ignores such messages. Both ends keep the file
//! fallbacks ([`crate::session_control`]) because a failed connection is
//! routine.
//!
//! Differences from the Python, all forced:
//! - The name hashes the resolved state directory with **SHA-256** (sha1 is
//!   not a workspace dependency), so a Rust peer never meets a Python peer
//!   on the same pipe; they fall back to file polling with each other.
//! - The first pipe instance is created with `FILE_FLAG_FIRST_PIPE_INSTANCE`,
//!   so a second overlay on the same state directory fails `listen()` (and
//!   uses the file fallback) instead of silently sharing the name.
//! - Qt signals become events: call `poll()` from the frame loop, or set a
//!   waker to be told when one arrives. Writes are queued to a per-client
//!   writer thread, so a stuck peer never blocks the overlay's frame.
//! - Keys are sorted at every level as `json.dumps(sort_keys=True)` does,
//!   but written compactly (no spaces after `,` and `:`).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_IO_PENDING, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE,
    WAIT_OBJECT_0, WIN32_ERROR,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_MODE,
    FlushFileBuffers, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, NAMED_PIPE_MODE, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, WaitNamedPipeW,
};
use windows::Win32::System::Threading::{CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects};

use crate::win::{pcwstr, wide};

pub const PROTOCOL_VERSION: i64 = 1;
const BUFFER_SIZE: u32 = 64 * 1024;

/// A pipe name unique to one state directory: an overlay and a settings
/// window sharing a state directory share one channel; different installs
/// never cross-talk.
pub fn channel_name(state_dir: impl AsRef<Path>) -> String {
    let resolved = resolve_path_string(state_dir.as_ref());
    let digest = Sha256::digest(resolved.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("DesktopBugCompanion-{}", &hex[..16])
}

/// `str(Path(p).resolve())`: absolute, symlinks resolved where the path
/// exists, without the `\\?\` prefix `canonicalize` adds on Windows.
pub fn resolve_path_string(path: &Path) -> String {
    let resolved =
        std::fs::canonicalize(path).or_else(|_| std::path::absolute(path)).unwrap_or_else(|_| path.to_path_buf());
    let s = resolved.to_string_lossy().into_owned();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s
    }
}

/// The Win32 path of a channel name.
pub fn pipe_path(name: &str) -> String {
    format!(r"\\.\pipe\{name}")
}

fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let ordered: BTreeMap<&String, Value> = map.iter().map(|(k, v)| (k, sorted(v))).collect();
            Value::Object(ordered.into_iter().map(|(k, v)| (k.clone(), v)).collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

/// The message with `"v"` added when it has none (`_stamped`).
pub fn stamped(message: &Map<String, Value>) -> Map<String, Value> {
    let mut out = message.clone();
    out.entry("v").or_insert(Value::from(PROTOCOL_VERSION));
    out
}

/// One framed message: sorted-key JSON plus `"\n"` (`_encode`).
pub fn encode(message: &Map<String, Value>) -> Vec<u8> {
    let value = sorted(&Value::Object(stamped(message)));
    let mut bytes = serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec());
    bytes.push(b'\n');
    bytes
}

/// A message object from a `serde_json::json!` value (non-objects become an
/// empty message).
pub fn message(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// Buffers bytes and yields complete JSON objects (`_LineReader`).
#[derive(Default)]
pub struct LineReader {
    buf: Vec<u8>,
}

impl LineReader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Vec<Map<String, Value>> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=pos).collect();
            let trimmed = line[..line.len() - 1].trim_ascii();
            if trimmed.is_empty() {
                continue;
            }
            match std::str::from_utf8(trimmed).ok().and_then(|s| serde_json::from_str::<Value>(s).ok()) {
                Some(Value::Object(map)) => out.push(map),
                Some(_) => {}
                None => log::warn!(target: "live_channel", "Dropped an unparsable live-channel message"),
            }
        }
        out
    }

    /// Bytes waiting for their newline.
    pub fn pending(&self) -> usize {
        self.buf.len()
    }
}

/// Whether a message carries the protocol version this build speaks. The
/// Python compared with `!=`, where `1.0 == 1`, so a float 1 counts too.
pub fn version_ok(message: &Map<String, Value>) -> bool {
    message.get("v").and_then(Value::as_f64) == Some(PROTOCOL_VERSION as f64)
}

/// A typed view of the five message types.
#[derive(Clone, Debug, PartialEq)]
pub enum ChannelMessage {
    Hello,
    SessionState { state: Value },
    PresetUpdate { preset_path: Option<String>, data: Value },
    StopRequest,
    ResetProgress,
    Unknown(String),
}

impl ChannelMessage {
    pub fn from_map(map: &Map<String, Value>) -> Self {
        match map.get("type").and_then(Value::as_str).unwrap_or("") {
            "hello" => ChannelMessage::Hello,
            "session_state" => ChannelMessage::SessionState { state: map.get("state").cloned().unwrap_or(Value::Null) },
            "preset_update" => ChannelMessage::PresetUpdate {
                preset_path: map.get("preset_path").and_then(Value::as_str).map(str::to_string),
                data: map.get("data").cloned().unwrap_or(Value::Null),
            },
            "stop_request" => ChannelMessage::StopRequest,
            "reset_progress" => ChannelMessage::ResetProgress,
            other => ChannelMessage::Unknown(other.to_string()),
        }
    }

    pub fn to_map(&self) -> Map<String, Value> {
        let mut m = Map::new();
        let t = match self {
            ChannelMessage::Hello => "hello",
            ChannelMessage::SessionState { state } => {
                m.insert("state".into(), state.clone());
                "session_state"
            }
            ChannelMessage::PresetUpdate { preset_path, data } => {
                m.insert("preset_path".into(), preset_path.clone().map(Value::from).unwrap_or(Value::Null));
                m.insert("data".into(), data.clone());
                "preset_update"
            }
            ChannelMessage::StopRequest => "stop_request",
            ChannelMessage::ResetProgress => "reset_progress",
            ChannelMessage::Unknown(t) => t.as_str(),
        };
        m.insert("type".into(), Value::from(t));
        stamped(&m)
    }
}

// -- Win32 plumbing ---------------------------------------------------------

struct OwnedHandle(HANDLE);

// Kernel handles may be used from any thread.
unsafe impl Send for OwnedHandle {}
unsafe impl Sync for OwnedHandle {}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

fn new_event() -> std::io::Result<OwnedHandle> {
    unsafe { CreateEventW(None, true, false, None) }.map(OwnedHandle).map_err(std::io::Error::other)
}

fn set_event(e: &OwnedHandle) {
    unsafe {
        let _ = SetEvent(e.0);
    }
}

fn is_code(e: &windows::core::Error, code: WIN32_ERROR) -> bool {
    e.code() == code.to_hresult()
}

/// Run one overlapped operation to completion, or cancel it when `stop` is
/// signalled. The `OVERLAPPED` lives on this stack frame and the function
/// never returns while the operation is still pending.
fn overlapped(
    handle: HANDLE,
    stop: &OwnedHandle,
    op: impl FnOnce(*mut OVERLAPPED) -> windows::core::Result<()>,
) -> std::io::Result<u32> {
    let event = new_event()?;
    let mut ov = OVERLAPPED { hEvent: event.0, ..Default::default() };
    let mut transferred = 0u32;
    match op(&mut ov) {
        Ok(()) => {
            unsafe { GetOverlappedResult(handle, &ov, &mut transferred, false) }.map_err(std::io::Error::other)?;
            Ok(transferred)
        }
        Err(e) if is_code(&e, ERROR_IO_PENDING) => {
            let which = unsafe { WaitForMultipleObjects(&[event.0, stop.0], false, INFINITE) };
            if which == WAIT_OBJECT_0 {
                unsafe { GetOverlappedResult(handle, &ov, &mut transferred, false) }.map_err(std::io::Error::other)?;
                Ok(transferred)
            } else {
                unsafe {
                    let _ = CancelIoEx(handle, Some(&ov));
                    // Wait for the cancellation so `ov` is free to drop.
                    let _ = GetOverlappedResult(handle, &ov, &mut transferred, true);
                }
                Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "stopped"))
            }
        }
        Err(e) => Err(std::io::Error::other(e)),
    }
}

/// Wait for a client on a fresh pipe instance (overlapped
/// `ConnectNamedPipe`), or give up when `stop` is signalled.
fn connect_pipe(pipe: HANDLE, stop: &OwnedHandle) -> std::io::Result<()> {
    let event = new_event()?;
    let mut ov = OVERLAPPED { hEvent: event.0, ..Default::default() };
    match unsafe { ConnectNamedPipe(pipe, Some(&mut ov)) } {
        Ok(()) => Ok(()),
        // The client connected between CreateNamedPipe and here.
        Err(e) if is_code(&e, ERROR_PIPE_CONNECTED) => Ok(()),
        Err(e) if is_code(&e, ERROR_IO_PENDING) => {
            let mut ignored = 0u32;
            if unsafe { WaitForMultipleObjects(&[event.0, stop.0], false, INFINITE) } == WAIT_OBJECT_0 {
                unsafe { GetOverlappedResult(pipe, &ov, &mut ignored, false) }.map_err(std::io::Error::other)
            } else {
                unsafe {
                    let _ = CancelIoEx(pipe, Some(&ov));
                    let _ = GetOverlappedResult(pipe, &ov, &mut ignored, true);
                }
                Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "stopped"))
            }
        }
        Err(e) => Err(std::io::Error::other(e)),
    }
}

fn read_some(pipe: HANDLE, stop: &OwnedHandle, buf: &mut [u8]) -> std::io::Result<usize> {
    overlapped(pipe, stop, |ov| unsafe { ReadFile(pipe, Some(buf), None, Some(ov)) }).map(|n| n as usize)
}

fn write_all(pipe: HANDLE, stop: &OwnedHandle, mut data: &[u8]) -> std::io::Result<()> {
    while !data.is_empty() {
        let n = overlapped(pipe, stop, |ov| unsafe { WriteFile(pipe, Some(data), None, Some(ov)) })? as usize;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::WriteZero, "pipe write returned 0"));
        }
        data = &data[n..];
    }
    Ok(())
}

type Waker = Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>;

fn wake(waker: &Waker) {
    let w = waker.lock().ok().and_then(|g| g.clone());
    if let Some(w) = w {
        w();
    }
}

/// One connected peer: a pipe handle shared by a reader and a writer thread.
struct Connection {
    pipe: Arc<OwnedHandle>,
    stop: Arc<OwnedHandle>,
    out: Option<Sender<Vec<u8>>>,
    alive: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl Connection {
    /// Start the two threads. `on_message` returns false to drop the peer;
    /// `on_close` runs once when the reader ends.
    fn start(
        pipe: OwnedHandle,
        server_end: bool,
        mut on_message: impl FnMut(Map<String, Value>) -> bool + Send + 'static,
        on_close: impl FnOnce() + Send + 'static,
    ) -> std::io::Result<Self> {
        let pipe = Arc::new(pipe);
        let stop = Arc::new(new_event()?);
        let alive = Arc::new(AtomicBool::new(true));
        let (out, out_rx) = mpsc::channel::<Vec<u8>>();

        let reader = {
            let (pipe, stop, alive) = (pipe.clone(), stop.clone(), alive.clone());
            std::thread::Builder::new().name("live-channel-read".into()).spawn(move || {
                let mut reader = LineReader::new();
                let mut buf = vec![0u8; BUFFER_SIZE as usize];
                'read: loop {
                    match read_some(pipe.0, &stop, &mut buf) {
                        Ok(0) => continue,
                        Ok(n) => {
                            for msg in reader.feed(&buf[..n]) {
                                if !on_message(msg) {
                                    break 'read;
                                }
                            }
                        }
                        Err(e) => {
                            if e.kind() != std::io::ErrorKind::Interrupted {
                                log::debug!(target: "live_channel", "live channel read ended: {e}");
                            }
                            break;
                        }
                    }
                }
                alive.store(false, Ordering::SeqCst);
                set_event(&stop);
                if server_end {
                    // Drop the peer now (Qt closed the socket), not at the next prune.
                    unsafe {
                        let _ = DisconnectNamedPipe(pipe.0);
                    }
                }
                on_close();
            })?
        };

        let writer = {
            let (pipe, stop, alive) = (pipe.clone(), stop.clone(), alive.clone());
            std::thread::Builder::new().name("live-channel-write".into()).spawn(move || {
                while let Ok(bytes) = out_rx.recv() {
                    if !alive.load(Ordering::SeqCst) || write_all(pipe.0, &stop, &bytes).is_err() {
                        break;
                    }
                    unsafe {
                        let _ = FlushFileBuffers(pipe.0);
                    }
                }
            })?
        };

        Ok(Self { pipe, stop, out: Some(out), alive, threads: vec![reader, writer] })
    }

    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn send(&self, bytes: Vec<u8>) -> bool {
        self.is_alive() && self.out.as_ref().map(|o| o.send(bytes).is_ok()).unwrap_or(false)
    }

    /// Stop both threads and wait for them.
    fn shutdown(mut self, disconnect_server_end: bool) {
        self.alive.store(false, Ordering::SeqCst);
        set_event(&self.stop);
        if disconnect_server_end {
            unsafe {
                let _ = DisconnectNamedPipe(self.pipe.0);
            }
        }
        // Dropping the sender ends the writer's recv loop.
        self.out = None;
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

// -- server -------------------------------------------------------------------

/// An event from [`OverlayChannelServer::poll`] (the Qt signals).
#[derive(Clone, Debug, PartialEq)]
pub enum ServerEvent {
    ClientConnected(u64),
    Message(u64, Map<String, Value>),
    ClientDisconnected(u64),
}

struct ServerShared {
    clients: Mutex<HashMap<u64, Connection>>,
    events: Mutex<Sender<ServerEvent>>,
    waker: Waker,
    next_id: AtomicU64,
}

impl ServerShared {
    fn emit(&self, event: ServerEvent) {
        if let Ok(tx) = self.events.lock() {
            let _ = tx.send(event);
        }
        wake(&self.waker);
    }
}

/// The overlay's end of the channel: one server, any number of clients.
pub struct OverlayChannelServer {
    name: String,
    shared: Option<Arc<ServerShared>>,
    stop: Option<Arc<OwnedHandle>>,
    accept_thread: Option<JoinHandle<()>>,
    events: Option<Receiver<ServerEvent>>,
    waker: Waker,
}

fn create_instance(path: &[u16], first: bool) -> Option<OwnedHandle> {
    let mut open = PIPE_ACCESS_DUPLEX.0 | FILE_FLAG_OVERLAPPED.0;
    if first {
        open |= FILE_FLAG_FIRST_PIPE_INSTANCE.0;
    }
    let mode = NAMED_PIPE_MODE(PIPE_TYPE_BYTE.0 | PIPE_READMODE_BYTE.0 | PIPE_WAIT.0 | PIPE_REJECT_REMOTE_CLIENTS.0);
    let handle = unsafe {
        CreateNamedPipeW(
            pcwstr(path),
            FILE_FLAGS_AND_ATTRIBUTES(open),
            mode,
            PIPE_UNLIMITED_INSTANCES,
            BUFFER_SIZE,
            BUFFER_SIZE,
            0,
            None,
        )
    };
    if handle.is_invalid() { None } else { Some(OwnedHandle(handle)) }
}

impl OverlayChannelServer {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            shared: None,
            stop: None,
            accept_thread: None,
            events: None,
            waker: Arc::new(Mutex::new(None)),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Called (from a channel thread) whenever an event is queued, so a
    /// winit loop can be woken with an `EventLoopProxy`.
    pub fn set_waker(&self, waker: impl Fn() + Send + Sync + 'static) {
        if let Ok(mut w) = self.waker.lock() {
            *w = Some(Arc::new(waker));
        }
    }

    /// Open the pipe. False (and a warning) when the name is held by another
    /// process; the caller then relies on file polling.
    pub fn listen(&mut self) -> bool {
        if self.is_listening() {
            return true;
        }
        let path = wide(&pipe_path(&self.name));
        let Some(first) = create_instance(&path, true) else {
            log::warn!(
                target: "live_channel",
                "Could not open the live channel ({}): {}",
                self.name,
                std::io::Error::last_os_error()
            );
            return false;
        };
        let Ok(stop) = new_event() else { return false };
        let stop = Arc::new(stop);
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(ServerShared {
            clients: Mutex::new(HashMap::new()),
            events: Mutex::new(tx),
            waker: self.waker.clone(),
            next_id: AtomicU64::new(1),
        });
        let thread = {
            let (shared, stop) = (shared.clone(), stop.clone());
            std::thread::Builder::new()
                .name("live-channel-accept".into())
                .spawn(move || accept_loop(path, first, shared, stop))
        };
        match thread {
            Ok(t) => {
                self.accept_thread = Some(t);
                self.shared = Some(shared);
                self.stop = Some(stop);
                self.events = Some(rx);
                true
            }
            Err(e) => {
                log::warn!(target: "live_channel", "Could not start the live channel thread: {e}");
                false
            }
        }
    }

    pub fn is_listening(&self) -> bool {
        self.accept_thread.as_ref().map(|t| !t.is_finished()).unwrap_or(false)
    }

    pub fn client_count(&self) -> usize {
        self.prune();
        self.shared.as_ref().and_then(|s| s.clients.lock().ok().map(|c| c.len())).unwrap_or(0)
    }

    fn prune(&self) {
        let Some(shared) = self.shared.as_ref() else { return };
        let dead: Vec<Connection> = {
            let Ok(mut clients) = shared.clients.lock() else { return };
            let ids: Vec<u64> = clients.iter().filter(|(_, c)| !c.is_alive()).map(|(id, _)| *id).collect();
            ids.into_iter().filter_map(|id| clients.remove(&id)).collect()
        };
        for c in dead {
            c.shutdown(true);
        }
    }

    /// Send to every connected settings window. Returns how many got it
    /// (queued to their writer).
    pub fn broadcast(&self, message: &Map<String, Value>) -> usize {
        self.prune();
        let payload = encode(message);
        let Some(shared) = self.shared.as_ref() else { return 0 };
        let Ok(clients) = shared.clients.lock() else { return 0 };
        clients.values().filter(|c| c.send(payload.clone())).count()
    }

    /// Every event queued since the last call (non-blocking).
    pub fn poll(&self) -> Vec<ServerEvent> {
        let events: Vec<ServerEvent> = self.events.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        if events.iter().any(|e| matches!(e, ServerEvent::ClientDisconnected(_))) {
            self.prune();
        }
        events
    }

    /// Wait up to `timeout` for the next event.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<ServerEvent> {
        self.events.as_ref()?.recv_timeout(timeout).ok()
    }

    pub fn close(&mut self) {
        if let Some(stop) = self.stop.take() {
            set_event(&stop);
        }
        if let Some(t) = self.accept_thread.take() {
            let _ = t.join();
        }
        if let Some(shared) = self.shared.take() {
            let all: Vec<Connection> =
                shared.clients.lock().map(|mut c| c.drain().map(|(_, v)| v).collect()).unwrap_or_default();
            for c in all {
                c.shutdown(true);
            }
        }
        self.events = None;
    }
}

impl Drop for OverlayChannelServer {
    fn drop(&mut self) {
        self.close();
    }
}

fn accept_loop(path: Vec<u16>, first: OwnedHandle, shared: Arc<ServerShared>, stop: Arc<OwnedHandle>) {
    let mut pending = Some(first);
    loop {
        let pipe = match pending.take() {
            Some(p) => p,
            None => match create_instance(&path, false) {
                Some(p) => p,
                None => {
                    log::warn!(target: "live_channel", "Could not open another live-channel instance");
                    if unsafe { WaitForMultipleObjects(&[stop.0], false, 250) } == WAIT_OBJECT_0 {
                        return;
                    }
                    continue;
                }
            },
        };
        let connected = connect_pipe(pipe.0, &stop);
        match connected {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => return,
            Err(e) => {
                log::debug!(target: "live_channel", "live channel connect failed: {e}");
                continue;
            }
        }
        let id = shared.next_id.fetch_add(1, Ordering::SeqCst);
        let on_message = {
            let shared = shared.clone();
            move |msg: Map<String, Value>| {
                if !version_ok(&msg) {
                    // A settings window from an incompatible build: closing
                    // forces it back onto file polling.
                    log::warn!(
                        target: "live_channel",
                        "Live channel protocol mismatch (got {:?}, want {PROTOCOL_VERSION}); closing",
                        msg.get("v")
                    );
                    return false;
                }
                shared.emit(ServerEvent::Message(id, msg));
                true
            }
        };
        let on_close = {
            let shared = shared.clone();
            move || shared.emit(ServerEvent::ClientDisconnected(id))
        };
        // Report the connection before the reader can report its first
        // message, as Qt's newConnection preceded readyRead. The client
        // table stays locked from the event until the client is in it, so a
        // broadcast made on the event (Python's `client_connected` handler)
        // waits for it instead of reaching nobody.
        let mut clients = shared.clients.lock().ok();
        shared.emit(ServerEvent::ClientConnected(id));
        match Connection::start(pipe, true, on_message, on_close) {
            Ok(conn) => {
                if let Some(clients) = clients.as_mut() {
                    clients.insert(id, conn);
                }
            }
            Err(e) => {
                drop(clients);
                log::warn!(target: "live_channel", "Could not serve a live-channel client: {e}");
                shared.emit(ServerEvent::ClientDisconnected(id));
            }
        }
    }
}

// -- client -------------------------------------------------------------------

/// An event from [`SettingsChannelClient::poll`].
#[derive(Clone, Debug, PartialEq)]
pub enum ClientEvent {
    Connected,
    Message(Map<String, Value>),
    Disconnected,
}

/// The settings window's end: connects to a running overlay.
pub struct SettingsChannelClient {
    name: String,
    conn: Option<Connection>,
    tx: Sender<ClientEvent>,
    rx: Receiver<ClientEvent>,
    waker: Waker,
}

impl SettingsChannelClient {
    pub fn new(name: impl Into<String>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self { name: name.into(), conn: None, tx, rx, waker: Arc::new(Mutex::new(None)) }
    }

    pub fn set_waker(&self, waker: impl Fn() + Send + Sync + 'static) {
        if let Ok(mut w) = self.waker.lock() {
            *w = Some(Arc::new(waker));
        }
    }

    /// Connect if not already connected, waiting up to `timeout_ms` for a
    /// busy server. Failure is routine: every caller keeps its fallback.
    pub fn try_connect(&mut self, timeout_ms: u64) -> bool {
        if self.is_connected() {
            return true;
        }
        if let Some(old) = self.conn.take() {
            old.shutdown(false);
        }
        let path = wide(&pipe_path(&self.name));
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let handle = loop {
            let r = unsafe {
                CreateFileW(
                    pcwstr(&path),
                    (GENERIC_READ | GENERIC_WRITE).0,
                    FILE_SHARE_MODE(0),
                    None,
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED,
                    None,
                )
            };
            match r {
                Ok(h) => break OwnedHandle(h),
                Err(e) if is_code(&e, ERROR_PIPE_BUSY) => {
                    let left = deadline.saturating_duration_since(Instant::now()).as_millis() as u32;
                    if left == 0 || !unsafe { WaitNamedPipeW(pcwstr(&path), left) }.as_bool() {
                        return false;
                    }
                }
                Err(_) => return false,
            }
        };
        let on_message = {
            let (tx, waker) = (self.tx.clone(), self.waker.clone());
            move |msg: Map<String, Value>| {
                if !version_ok(&msg) {
                    log::warn!(
                        target: "live_channel",
                        "Live channel protocol mismatch (got {:?}, want {PROTOCOL_VERSION}); ignoring",
                        msg.get("v")
                    );
                    return true;
                }
                let _ = tx.send(ClientEvent::Message(msg));
                wake(&waker);
                true
            }
        };
        let on_close = {
            let (tx, waker) = (self.tx.clone(), self.waker.clone());
            move || {
                let _ = tx.send(ClientEvent::Disconnected);
                wake(&waker);
            }
        };
        match Connection::start(handle, false, on_message, on_close) {
            Ok(conn) => {
                self.conn = Some(conn);
                self.send(&ChannelMessage::Hello.to_map());
                let _ = self.tx.send(ClientEvent::Connected);
                true
            }
            Err(_) => false,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.conn.as_ref().map(Connection::is_alive).unwrap_or(false)
    }

    pub fn send(&self, message: &Map<String, Value>) -> bool {
        match self.conn.as_ref() {
            Some(c) => c.send(encode(message)),
            None => false,
        }
    }

    /// Write bytes as they are, bypassing the version stamp (tests use it to
    /// play an incompatible build).
    #[doc(hidden)]
    pub fn send_raw(&self, bytes: &[u8]) -> bool {
        self.conn.as_ref().map(|c| c.send(bytes.to_vec())).unwrap_or(false)
    }

    pub fn poll(&self) -> Vec<ClientEvent> {
        self.rx.try_iter().collect()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Option<ClientEvent> {
        self.rx.recv_timeout(timeout).ok()
    }

    pub fn close(&mut self) {
        if let Some(c) = self.conn.take() {
            c.shutdown(false);
        }
    }
}

impl Drop for SettingsChannelClient {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn unique_name(tag: &str) -> String {
        let dir = std::env::temp_dir().join(format!("dc-live-{tag}-{}-{}", std::process::id(), crate::win::tick_ms()));
        channel_name(dir)
    }

    fn wait_for<T>(mut f: impl FnMut() -> Option<T>) -> Option<T> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Some(v) = f() {
                return Some(v);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        None
    }

    #[test]
    fn channel_name_is_stable_and_distinct() {
        let a = channel_name(r"C:\some\state");
        assert_eq!(a, channel_name(r"C:\some\state"));
        assert_ne!(a, channel_name(r"C:\other\state"));
        assert!(a.starts_with("DesktopBugCompanion-"));
        assert_eq!(a.len(), "DesktopBugCompanion-".len() + 16);
        assert!(!resolve_path_string(&std::env::temp_dir()).starts_with(r"\\?\"));
    }

    #[test]
    fn framing_survives_arbitrary_chunks() {
        let a = encode(&message(json!({"type": "preset_update", "data": {"b": 1, "a": [1, 2]}, "preset_path": "x"})));
        assert_eq!(a.last(), Some(&b'\n'));
        let text = std::str::from_utf8(&a).unwrap();
        // Sorted keys at every level, and the version stamp.
        assert_eq!(text, "{\"data\":{\"a\":[1,2],\"b\":1},\"preset_path\":\"x\",\"type\":\"preset_update\",\"v\":1}\n");
        let b = encode(&message(json!({"type": "hello"})));
        let mut stream = a.clone();
        stream.extend_from_slice(b"\n  \nnot json\n[1]\n");
        stream.extend_from_slice(&b);
        let mut reader = LineReader::new();
        let mut got = Vec::new();
        for chunk in stream.chunks(3) {
            got.extend(reader.feed(chunk));
        }
        assert_eq!(got.len(), 2);
        assert_eq!(got[0]["type"], "preset_update");
        assert_eq!(got[1]["type"], "hello");
        assert_eq!(reader.pending(), 0);
        // An explicit version is kept, not overwritten.
        let odd = stamped(&message(json!({"type": "x", "v": 999})));
        assert_eq!(odd["v"], 999);
        assert!(!version_ok(&odd));
        assert!(version_ok(&stamped(&message(json!({"type": "x"})))));
        assert!(version_ok(&message(json!({"v": 1.0}))));
        assert!(!version_ok(&message(json!({"v": "1"}))));
    }

    #[test]
    fn typed_messages_round_trip() {
        for m in [
            ChannelMessage::Hello,
            ChannelMessage::SessionState { state: json!({"mood_mode": "playful"}) },
            ChannelMessage::PresetUpdate { preset_path: Some("p.json".into()), data: json!({"a": 1}) },
            ChannelMessage::StopRequest,
            ChannelMessage::ResetProgress,
        ] {
            let map = m.to_map();
            assert_eq!(map["v"], PROTOCOL_VERSION);
            assert_eq!(ChannelMessage::from_map(&map), m);
        }
        assert_eq!(ChannelMessage::from_map(&message(json!({"type": "zzz"}))), ChannelMessage::Unknown("zzz".into()));
    }

    #[test]
    fn protocol_round_trip_over_a_real_pipe() {
        let name = unique_name("roundtrip");
        let mut server = OverlayChannelServer::new(&name);
        let mut client = SettingsChannelClient::new(&name);
        assert!(server.listen(), "server could not bind the channel");
        assert!(server.listen(), "listen twice is a no-op");

        // A second server on the same name must not bind.
        let mut rival = OverlayChannelServer::new(&name);
        assert!(!rival.listen(), "two overlays shared one channel name");

        assert!(client.try_connect(500), "client could not reach the server");
        assert!(wait_for(|| (server.client_count() == 1).then_some(())).is_some());

        let mut received = Vec::new();
        let hello = wait_for(|| {
            received.extend(server.poll());
            received.iter().find_map(|e| match e {
                ServerEvent::Message(_, m) => Some(m.clone()),
                _ => None,
            })
        })
        .expect("no hello");
        assert_eq!(hello["type"], "hello");
        assert_eq!(hello["v"], PROTOCOL_VERSION);
        assert!(matches!(received[0], ServerEvent::ClientConnected(_)), "{received:?}");

        // A big preset crosses many reads and still arrives whole.
        let big: Vec<i64> = (0..40_000).collect();
        assert!(
            client.send(&message(json!({"type": "preset_update", "preset_path": "x", "data": {"a": 1, "big": big}})))
        );
        let update = wait_for(|| {
            server.poll().into_iter().find_map(|e| match e {
                ServerEvent::Message(_, m) => Some(m),
                _ => None,
            })
        })
        .expect("no preset_update");
        assert_eq!(update["type"], "preset_update");
        assert_eq!(update["preset_path"], "x");
        assert_eq!(update["data"]["a"], 1);
        assert_eq!(update["data"]["big"].as_array().unwrap().len(), 40_000);

        assert_eq!(server.broadcast(&message(json!({"type": "session_state", "state": {"mood_mode": "playful"}}))), 1);
        let state = wait_for(|| {
            client.poll().into_iter().find_map(|e| match e {
                ClientEvent::Message(m) => Some(m),
                _ => None,
            })
        })
        .expect("no session_state");
        assert_eq!(state["state"]["mood_mode"], "playful");
        assert_eq!(state["v"], PROTOCOL_VERSION);

        // A second client is served alongside the first.
        let mut second = SettingsChannelClient::new(&name);
        assert!(second.try_connect(500));
        assert!(wait_for(|| (server.client_count() == 2).then_some(())).is_some());
        assert_eq!(server.broadcast(&ChannelMessage::StopRequest.to_map()), 2);
        second.close();
        assert!(wait_for(|| (server.client_count() == 1).then_some(())).is_some(), "closed client still counted");

        client.close();
        server.close();
        assert!(!server.is_listening());
        // After close the name is free again.
        assert!(rival.listen());
        rival.close();
    }

    #[test]
    fn a_broadcast_on_client_connected_reaches_that_client() {
        let name = unique_name("welcome");
        let mut server = OverlayChannelServer::new(&name);
        assert!(server.listen());
        for round in 0..25 {
            let mut client = SettingsChannelClient::new(&name);
            assert!(client.try_connect(500), "round {round}: no connection");
            let connected = server.recv_timeout(Duration::from_secs(2));
            assert!(matches!(connected, Some(ServerEvent::ClientConnected(_))), "round {round}: {connected:?}");
            // What the overlay does on the event: welcome the new window.
            assert_eq!(server.broadcast(&ChannelMessage::Hello.to_map()), 1, "round {round}: nobody to welcome");
            client.close();
            assert!(wait_for(|| (server.client_count() == 0).then_some(())).is_some(), "round {round}");
            let _ = server.poll();
        }
        server.close();
    }

    #[test]
    fn protocol_version_mismatch_disconnects() {
        let name = unique_name("mismatch");
        let mut server = OverlayChannelServer::new(&name);
        let mut client = SettingsChannelClient::new(&name);
        assert!(server.listen());
        assert!(client.try_connect(500));
        assert!(wait_for(|| (server.client_count() == 1).then_some(())).is_some());
        assert!(client.send_raw(b"{\"v\": 999, \"type\": \"preset_update\"}\n"));
        let disconnected = wait_for(|| client.poll().into_iter().any(|e| e == ClientEvent::Disconnected).then_some(()));
        assert!(disconnected.is_some(), "an unversioned message should have been rejected");
        assert!(!client.is_connected());
        assert!(wait_for(|| (server.client_count() == 0).then_some(())).is_some());
        server.close();
    }

    #[test]
    fn connecting_to_nothing_fails_quietly() {
        let mut client = SettingsChannelClient::new(unique_name("nobody"));
        assert!(!client.try_connect(50));
        assert!(!client.is_connected());
        assert!(!client.send(&ChannelMessage::Hello.to_map()));
    }
}
