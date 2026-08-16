// Remote Control relay — see docs/plan/done/remote-control.md §7 (Rust — relay only), §7.0 (ICON-1), §7.1 + §7.1a (pairing + Tailscale), §7.5 (FileView / read_text_file / FILE-1), and §13 (FROZEN wire-protocol contract). This module implements exactly that contract; protocol decisions live in JS seams — this file provides content-blind routing plus native operations (pairing, icon cache, confined file read, address discovery).
//
// INVARIANT (§13.6): the WS relay never holds mirrored app state, reading frames only for ROUTING and DROP-SAFETY. Three documented departures:
//  1. (plan §2.3) Relay originates `{"t":"companion-connected","id":<conn_key>}` to host on token pass so host pushes `init` snapshot and scrollback replay.
//  2. (backpressure) Relay reads top-level `t` on queued companion frames only when budget is blown, dropping re-derivable `pty_output` (see `is_coalescible`).
//  3. (per-connection addressing, 1.21.1) Relay reads `to` on host frames (`dispatch`) and stamps `from` on companion frames (`handle_companion_socket`). Unit is a connection (`c<conn_id>`), not a device, ensuring multiple tabs on one device have isolated request counters and replays (preserving INVARIANT R). Device-level grouping is used strictly for `revoke_device`.

use axum::{
    body::Body,
    extract::{
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, Query, Request,
    },
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, Mutex as StdMutex, OnceLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::AppHandle;
use tokio::{
    net::TcpListener,
    sync::{mpsc, Notify},
};

/// Fixed relay port (baked into `tauri.conf.json` CSP `connect-src` and `get_companion_url()`). Not user-configurable (plan §7.1a).
const PORT: u16 = 1421;

/// How many consecutive bad `/pair` codes are tolerated before the relay shuts itself off.
const MAX_PAIR_FAILURES: u32 = 10;

// ── WS close codes (mirrored in src/constants/protocol.js — keep both in sync) ─────────────
// Distinct codes prevent companions from treating temporary server-off state as credential revocation.

/// Token presented is unknown/absent/revoked: companion must drop token and prompt for re-pairing.
const CLOSE_UNPAIRED: u16 = 4001;
/// Remote control is disabled on the host: companion keeps token and reconnects with backoff.
const CLOSE_SERVER_DISABLED: u16 = 4002;
/// `role=host` refused (non-loopback or invalid process-local secret).
const CLOSE_HOST_ROLE_REJECTED: u16 = 4003;

// ── Backpressure toward companions ────────────────────────────────────────────────────────────
// Policy: Coalesce then re-hydrate. When an outbox blows `COMPANION_QUEUE_LIMIT_BYTES`, drop re-derivable `pty_output` frames and flag for resync. When the queue drains, re-issue `companion-connected` to trigger fresh `init` and `reset:true` scrollback replay.
// Blocking the producer is unacceptable (would stall host loop and all clients), while dropping arbitrary frames causes visual corruption in xterm.

/// Per-companion outbound budget derived from terminal caps (asserted in `invariant_r_holds_for_a_full_scrollback_replay`).
///
/// Sizing: `MAX_TABS` (16) * replay frame size (174,892 B at 128 KiB cap) = 2.67 MiB for one full replay.
///
/// INVARIANT R:
/// - **R1**: `MAX_TABS * replay_frame_bytes(SCROLLBACK_CAP) <= COMPANION_QUEUE_LIMIT_BYTES / 2` (2,798,272 <= 4,194,304). Ensures recovery replay fits with 50% budget reserved for undroppable state frames. Address is per-connection so multiple tabs on one device do not multiply this load.
/// - **R2**: `2 * MAX_TABS * replay_frame_bytes(SCROLLBACK_CAP) <= COMPANION_QUEUE_LIMIT_BYTES` (5,596,544 <= 8,388,608). Prevents coalesce when an addressed replay overlaps with a broadcast congestion replay.
const COMPANION_QUEUE_LIMIT_BYTES: usize = 8 * 1024 * 1024;

/// Frame tag of the ONE coalescible kind (mirrors `FRAME_PTY_OUTPUT` in `src/constants/protocol.js`).
const FRAME_PTY_OUTPUT: &str = "pty_output";

/// RFC 6455 1013 "Try Again Later": sent when undroppable frames exceed budget, triggering clean client reconnect and full re-hydrate.
const CLOSE_TOO_FAR_BEHIND: u16 = 1013;

/// A companion's pending outbound frames with coalescing and lifecycle tracking.
struct Outbox {
    queue: VecDeque<Message>,
    /// Payload bytes currently queued.
    bytes: usize,
    /// Set when a coalesce dropped frames; consumed when queue drains to request re-hydrate.
    resync_pending: bool,
    /// Set when connection teardown begins (Close frame queued); ignores subsequent pushes.
    closed: bool,
}

/// Shared between the relay's producers (the host-socket loop, `revoke_device`,
/// `stop_companion_server`) and the one companion task that owns the socket.
type CompanionOutbox = Arc<(StdMutex<Outbox>, Notify)>;

impl Outbox {
    fn new() -> Self {
        Outbox { queue: VecDeque::new(), bytes: 0, resync_pending: false, closed: false }
    }

    /// Queue one frame and enforce the budget. THE WHOLE POLICY LIVES HERE, not in the caller, so
    /// there is exactly one answer to "what happens when a phone falls behind" no matter which
    /// producer pushed the frame that tipped it over.
    fn push_within_budget(&mut self, msg: Message) {
        if self.closed {
            return;
        }
        self.bytes += frame_bytes(&msg);
        self.queue.push_back(msg);
        if self.bytes > COMPANION_QUEUE_LIMIT_BYTES {
            self.coalesce();
            if self.bytes > COMPANION_QUEUE_LIMIT_BYTES {
                self.force_close();
            }
        }
    }

    /// Drops every coalescible frame and flags the connection for a re-hydrate. Called only when
    /// the budget is already blown, so the JSON parsing it does is off the hot path entirely: after
    /// a collapse the queue is near-empty, so the next one cannot happen until another whole budget
    /// has accumulated.
    fn coalesce(&mut self) {
        let before = self.queue.len();
        self.queue.retain(|m| !is_coalescible(m));
        if self.queue.len() != before {
            self.resync_pending = true;
            self.bytes = self.queue.iter().map(frame_bytes).sum();
        }
    }

    /// Last resort: force-closes connection with RFC 6455 1013 when undroppable frames exceed budget, triggering clean client reconnect.
    fn force_close(&mut self) {
        self.queue.clear();
        self.bytes = 0;
        self.resync_pending = false;
        self.queue.push_back(Message::Close(Some(CloseFrame {
            code: CLOSE_TOO_FAR_BEHIND,
            reason: "this device fell too far behind the host — reconnect for a fresh snapshot".into(),
        })));
        self.closed = true;
    }

    /// Hands backlog to socket writer; returns `(batch, wants_resync)` where resync is true only if coalesce occurred and queue is now empty.
    fn take(&mut self) -> (Vec<Message>, bool) {
        let batch: Vec<Message> = self.queue.drain(..).collect();
        self.bytes = 0;
        let resync = self.resync_pending;
        self.resync_pending = false;
        (batch, resync)
    }
}

fn frame_bytes(msg: &Message) -> usize {
    match msg {
        Message::Text(s) => s.len(),
        Message::Binary(b) => b.len(),
        // Ping/pong/close carry nothing worth budgeting and are never coalescible.
        _ => 0,
    }
}

/// DROP-SAFETY CHECK (routing is handled by `addressed_to` and `stamp_from`).
///
/// COALESCIBLE (re-derivable from host scrollback): `pty_output` chunks and replays.
///
/// NEVER DROPPED (default-deny for unrecognized/binary):
/// - `init` / `delta`: mirrored app state (including confirmation dialogs).
/// - `invoke_result`: RPC reply matching companion request id.
/// - `pty_exit`, `pty_resize`: terminal liveness and dimension edges.
/// - `ping` / `pong`: connection health.
fn is_coalescible(msg: &Message) -> bool {
    let Message::Text(text) = msg else { return false };
    #[derive(Deserialize)]
    struct FrameTag {
        t: Option<String>,
    }
    match serde_json::from_str::<FrameTag>(text) {
        Ok(tag) => tag.t.as_deref() == Some(FRAME_PTY_OUTPUT),
        Err(_) => false,
    }
}

/// Reads optional top-level `to` naming recipient connection key (`c<conn_id>`). Absent or invalid returns `None` (broadcast). Parsed once per host frame in `dispatch`.
fn addressed_to(msg: &Message) -> Option<String> {
    let Message::Text(text) = msg else { return None };
    #[derive(Deserialize)]
    struct FrameAddress {
        to: Option<String>,
    }
    serde_json::from_str::<FrameAddress>(text).ok().and_then(|f| f.to)
}

/// Stamps sending connection key `c<conn_id>` unconditionally onto inbound companion JSON frames for host reply routing.
fn stamp_from(msg: Message, conn_key: &str) -> Message {
    let Message::Text(text) = msg else { return msg };
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(serde_json::Value::Object(mut map)) => {
            map.insert("from".to_string(), serde_json::Value::String(conn_key.to_string()));
            Message::Text(serde_json::Value::Object(map).to_string())
        }
        _ => Message::Text(text),
    }
}

/// Queues a frame into companion outbox without blocking the host loop.
fn enqueue(outbox: &CompanionOutbox, msg: Message) {
    let (lock, notify) = &**outbox;
    lock.lock().unwrap_or_else(|e| e.into_inner()).push_within_budget(msg);
    // `notify_one` stores a permit when nobody is waiting so notifications are not lost while awaiting send.
    notify.notify_one();
}

// ── Shared state (process-global OnceLock<Mutex<..>>) ─────────────────────────────────────────

struct CompanionHandle {
    /// Persistent device id (from `companion-devices.json`), used by `revoke_device` to close all sockets for a device.
    device_id: String,
    /// Wire address for this connection (`c<conn_id>`), minted from monotonic counter.
    conn_key: String,
    outbox: CompanionOutbox,
}

struct RelayState {
    host_tx: StdMutex<Option<mpsc::UnboundedSender<Message>>>,
    companions: StdMutex<HashMap<u64, CompanionHandle>>,
    next_id: AtomicU64,
    pairing_code: StdMutex<String>,
    devices: StdMutex<Vec<PairedDevice>>,
    devices_path: StdMutex<Option<PathBuf>>,
    server_state_path: StdMutex<Option<PathBuf>>,
    /// Process-local secret minted fresh at startup for `role=host` WebSocket authentication (protects against loopback proxy bypass via Tailscale HTTPS).
    host_token: String,
    /// Gate controlling whether remote control accepts connections/pairing.
    enabled: AtomicBool,
    /// Consecutive invalid pairing attempts before automatic server disable (rate-limit guard against brute-force).
    pair_failures: AtomicU32,
}

impl RelayState {
    fn new() -> Self {
        RelayState {
            host_tx: StdMutex::new(None),
            companions: StdMutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            pairing_code: StdMutex::new(String::new()),
            devices: StdMutex::new(Vec::new()),
            devices_path: StdMutex::new(None),
            server_state_path: StdMutex::new(None),
            host_token: generate_token(),
            enabled: AtomicBool::new(false),
            pair_failures: AtomicU32::new(0),
        }
    }

    /// Routes one host frame to recipient connection named in `to`, or broadcasts to all companions if `to` is None.
    ///
    /// Broadcast is default for `pty_output` live stream, `delta`, `init`, and lifecycle events.
    /// `to` is set for companion scrollback replays and `invoke_result` RPC responses.
    fn dispatch(&self, msg: Message) {
        let to = addressed_to(&msg);
        let companions = self.companions.lock().unwrap_or_else(|e| e.into_inner());
        for handle in companions.values() {
            if let Some(conn_key) = to.as_deref() {
                if handle.conn_key != conn_key {
                    continue;
                }
            }
            enqueue(&handle.outbox, msg.clone());
        }
    }

    /// Forwards inbound frame to host (unbounded queue because traffic is human-paced and recipient is local webview).
    fn forward_to_host(&self, msg: Message) {
        let host_tx = self.host_tx.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(tx) = host_tx.as_ref() {
            let _ = tx.send(msg);
        }
    }

    /// Emits `{"t":"companion-connected","id":<conn_key>}` to host so it generates an `init` snapshot and targeted scrollback replay.
    fn notify_host_companion_connected(&self, conn_key: &str) {
        let payload = serde_json::json!({ "t": "companion-connected", "id": conn_key }).to_string();
        self.forward_to_host(Message::Text(payload));
    }

    /// Synchronous disk write for device list (must be wrapped in `spawn_blocking` when called from async handlers).
    fn persist_devices(&self) -> Result<(), String> {
        let path = self
            .devices_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| "companion-devices.json path not initialized".to_string())?;
        let devices = self.devices.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let content = serde_json::to_string_pretty(&devices).map_err(|e| e.to_string())?;
        std::fs::write(&path, content).map_err(|e| e.to_string())
    }

    /// Records the user's LAST EXPLICIT on/off choice so a restart resumes it (see `init` for the
    /// reasoning). Same blocking-write discipline as `persist_devices`: callers on the async
    /// runtime must route it through `spawn_blocking`. Best-effort by design — failing to write
    /// this preference must never fail the toggle the user just asked for.
    fn persist_enabled(&self, enabled: bool) {
        let Some(path) = self.server_state_path.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
            return;
        };
        let content = serde_json::json!({ "enabled": enabled }).to_string();
        if let Err(e) = std::fs::write(&path, content) {
            eprintln!("[web_server] could not persist the remote-control on/off state: {}", e);
        }
    }

    /// Flip the gate and remember the choice in one call, so no path can change one without the
    /// other and leave disk disagreeing with memory.
    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
        self.persist_enabled(enabled);
    }
}

/// `companion-server.json` — the one bit of relay state that outlives the process. `#[serde(default)]`
/// per CLAUDE.md's serde rule, so an older/partial file degrades to "off" instead of failing the read.
#[derive(Serialize, Deserialize, Default)]
struct PersistedServerState {
    #[serde(default)]
    enabled: bool,
}

static RELAY: OnceLock<RelayState> = OnceLock::new();

fn relay() -> &'static RelayState {
    RELAY.get_or_init(RelayState::new)
}

// ── Persisted device model (§13.1: `<appConfigDir>/companion-devices.json`) ──────────────────

#[derive(Clone, Serialize, Deserialize)]
struct PairedDevice {
    id: String,
    token: String,
    label: String,
    #[serde(rename = "pairedAt")]
    paired_at: u64,
}

/// What `list_paired_devices()` returns to the frontend — deliberately omits `token`. The host
/// UI never needs the raw secret back once pairing has completed; not returning it is a small,
/// free reduction of what a shoulder-surf or screen-share of the paired-devices modal can leak.
#[derive(Serialize)]
pub struct PairedDeviceView {
    id: String,
    label: String,
    #[serde(rename = "pairedAt")]
    paired_at: u64,
}

impl From<&PairedDevice> for PairedDeviceView {
    fn from(d: &PairedDevice) -> Self {
        PairedDeviceView { id: d.id.clone(), label: d.label.clone(), paired_at: d.paired_at }
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// Fills `buf` with OS-sourced random bytes. `getrandom` failing at all is exceedingly rare
/// (no OS entropy source); falls back to a time-seeded buffer rather than panicking — a
/// Generates OS-sourced random bytes with fallback to time-seeded PRNG if OS entropy fails.
fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    if getrandom::getrandom(&mut buf).is_err() {
        let seed = now_secs() as u128 ^ (std::process::id() as u128) << 32;
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (seed.wrapping_add(i as u128 * 2654435761)) as u8;
        }
    }
    buf
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// §13.1: "a random 128-bit hex string".
fn generate_token() -> String {
    hex_encode(&random_bytes::<16>())
}

/// Device id — shorter, only needs to be unique among paired devices, never sent as a secret.
fn generate_id() -> String {
    hex_encode(&random_bytes::<8>())
}

/// §7.1: "Mac shows a 6-digit code".
fn generate_pairing_code() -> String {
    let n = u32::from_be_bytes(random_bytes::<4>()) % 1_000_000;
    format!("{:06}", n)
}

// ── Public init, called once from `lib.rs`'s `setup()` ───────────────────────────────────────

/// Loads paired devices, restores enabled preference from `companion-server.json`, and spawns server task on tokio runtime.
pub fn init(app_handle: &AppHandle) {
    let state = relay();
    match crate::projects::get_app_data_dir(app_handle) {
        Ok(dir) => {
            let path = dir.join("companion-devices.json");
            if let Ok(content) = std::fs::read_to_string(&path) {
                match serde_json::from_str::<Vec<PairedDevice>>(&content) {
                    Ok(devices) => *state.devices.lock().unwrap_or_else(|e| e.into_inner()) = devices,
                    Err(e) => eprintln!("[web_server] companion-devices.json is corrupt, starting empty: {}", e),
                }
            }
            *state.devices_path.lock().unwrap_or_else(|e| e.into_inner()) = Some(path);

            let server_state_path = dir.join("companion-server.json");
            let restored = std::fs::read_to_string(&server_state_path)
                .ok()
                .and_then(|c| serde_json::from_str::<PersistedServerState>(&c).ok())
                .unwrap_or_default();
            *state.server_state_path.lock().unwrap_or_else(|e| e.into_inner()) = Some(server_state_path);
            if restored.enabled {
                *state.pairing_code.lock().unwrap_or_else(|e| e.into_inner()) = generate_pairing_code();
                state.enabled.store(true, Ordering::SeqCst);
            }
        }
        Err(e) => {
            eprintln!("[web_server] could not resolve app data dir, pairing will not persist: {}", e);
        }
    }

    let app_for_server = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        serve_forever(app_for_server).await;
    });
}

/// Binds IPv4 listener on port 1421 and runs axum server for the process lifetime. IPv4-only bind avoids IPv6-mapped dual-stack loopback issues.
async fn serve_forever(app: AppHandle) {
    let addr = SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::UNSPECIFIED), PORT);
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[web_server] failed to bind {}: {} — companion server will not start", addr, e);
            return;
        }
    };
    let router = build_router(&app);
    let make_service = router.into_make_service_with_connect_info::<SocketAddr>();
    if let Err(e) = axum::serve(listener, make_service).await {
        eprintln!("[web_server] serve() exited with error: {}", e);
    }
}

// ── §7.2 PORT-1: axum on :1421 is the ONE LAN entry, dev and release alike ───────────────────
// Release: Serves SPA from binary via Tauri embedded asset resolver.
// Dev: Reverse-proxies non-relay requests to Vite dev server on localhost (HMR is not proxied).
fn build_router(app: &AppHandle) -> Router {
    let router = Router::new()
        .route("/ws", get(ws_handler))
        .route("/pair", post(pair_handler))
        .layer(tower_http::cors::CorsLayer::permissive());

    if cfg!(debug_assertions) {
        let vite_origin = resolve_vite_origin(app);
        eprintln!("[web_server] dev mode: proxying non-relay requests to vite on {}", vite_origin);

        return router.fallback(move |req: Request| {
            let origin = vite_origin.clone();
            async move { dev_proxy_handler(origin, req).await }
        });
    }

    let app_for_fallback = app.clone();
    router.fallback(move |req: Request| {
        let app = app_for_fallback.clone();
        async move { release_asset_handler(app, req).await }
    })
}

/// Resolves Vite dev server origin from Tauri config (defaults to `http://localhost:1420`).
fn resolve_vite_origin(app: &AppHandle) -> String {
    if let Some(url) = app.config().build.dev_url.as_ref() {
        let origin = url.as_str().trim_end_matches('/').to_string();
        if !origin.is_empty() {
            return origin;
        }
    }
    let port: u16 = std::env::var("TAURI_DEV_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1420);
    format!("http://localhost:{}", port)
}

/// Rejects LAN HTTP requests with 503 Service Unavailable when remote control is disabled.
fn reject_if_disabled() -> Option<Response> {
    if relay().enabled.load(Ordering::SeqCst) {
        return None;
    }
    Some(
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "remote control is off on the host — turn it on from the app's menu",
        )
            .into_response(),
    )
}

/// Filters hop-by-hop headers (RFC 7230 §6.1) plus host/content-length before proxy forwarding.
fn is_hop_by_hop_header(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        "connection" | "keep-alive" | "transfer-encoding" | "upgrade" | "host" | "content-length"
    ) || name.starts_with("proxy-")
}

/// Reverse-proxies HTTP requests to Vite dev server in debug builds.
async fn dev_proxy_handler(vite_origin: String, req: Request) -> Response {
    if let Some(resp) = reject_if_disabled() {
        return resp;
    }
    if req.headers().contains_key(header::UPGRADE) {
        return (StatusCode::NOT_IMPLEMENTED, "websocket upgrades are not proxied in dev (see docs/plan/done/remote-control.md §7.2)")
            .into_response();
    }

    let (parts, body) = req.into_parts();

    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("failed to read request body: {e}")).into_response()
        }
    };

    let path_and_query = parts.uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
    let target = format!("{}{}", vite_origin, path_and_query);

    let client = dev_proxy_client();
    let mut builder = client.request(parts.method.clone(), &target);
    for (name, value) in parts.headers.iter() {
        if is_hop_by_hop_header(name.as_str()) {
            continue;
        }
        builder = builder.header(name, value);
    }

    let upstream = match builder.body(body_bytes.to_vec()).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[web_server] dev proxy: vite at {} unreachable: {}", vite_origin, e);
            return (
                StatusCode::BAD_GATEWAY,
                format!("vite dev server unreachable at {vite_origin}: {e}"),
            )
                .into_response();
        }
    };

    let status = StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut out = Response::builder().status(status);
    for (name, value) in upstream.headers().iter() {
        if is_hop_by_hop_header(name.as_str()) {
            continue;
        }
        out = out.header(name, value);
    }

    match upstream.bytes().await {
        Ok(bytes) => out
            .body(Body::from(bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(e) => (StatusCode::BAD_GATEWAY, format!("failed reading vite response body: {e}")).into_response(),
    }
}

/// One shared client (connection pooling), built lazily on first use.
fn dev_proxy_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

/// Serves embedded SPA frontend assets in release builds using Tauri asset resolver.
async fn release_asset_handler(app: AppHandle, req: Request) -> Response {
    if let Some(resp) = reject_if_disabled() {
        return resp;
    }
    let raw_path = req.uri().path().trim_start_matches('/');
    let path = if raw_path.is_empty() { "index.html" } else { raw_path };

    let resolver = app.asset_resolver();
    let asset = resolver
        .get(path.to_string())
        .or_else(|| resolver.get("index.html".to_string()));

    match asset {
        Some(asset) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, asset.mime_type)
            .body(Body::from(asset.bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        None => (StatusCode::NOT_FOUND, "asset not found in embedded bundle").into_response(),
    }
}

// ── /ws — the content-blind relay (§13.6) ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct WsQuery {
    role: Option<String>,
    token: Option<String>,
}

/// Max inbound frame size (2MB) to prevent memory exhaustion from unauthenticated peers.
const MAX_INBOUND_FRAME: usize = 2 * 1024 * 1024;

async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(q): Query<WsQuery>,
) -> Response {
    ws.max_message_size(MAX_INBOUND_FRAME)
        .max_frame_size(MAX_INBOUND_FRAME)
        .on_upgrade(move |socket| handle_socket(socket, addr, q))
}

async fn handle_socket(socket: WebSocket, addr: SocketAddr, q: WsQuery) {
    match q.role.as_deref() {
        Some("host") => {
            // Validates host WebSocket on loopback address and process-local host secret.
            let token = q.token.as_deref().unwrap_or_default();
            if !addr.ip().is_loopback() || token != relay().host_token {
                close_with_code(
                    socket,
                    CLOSE_HOST_ROLE_REJECTED,
                    "host role requires a loopback connection and the process host token",
                )
                .await;
                return;
            }
            handle_host_socket(socket).await;
        }
        Some("companion") => {
            let state = relay();
            // Validates companion connection against enabled state and paired device tokens.
            if !state.enabled.load(Ordering::SeqCst) {
                close_with_code(socket, CLOSE_SERVER_DISABLED, "remote control is disabled on the host").await;
                return;
            }
            let token = q.token.clone().unwrap_or_default();
            let device_id = {
                let devices = state.devices.lock().unwrap_or_else(|e| e.into_inner());
                devices.iter().find(|d| d.token == token).map(|d| d.id.clone())
            };
            let Some(device_id) = device_id else {
                close_with_code(socket, CLOSE_UNPAIRED, "invalid or unpaired token").await;
                return;
            };
            let conn_id = state.next_id.fetch_add(1, Ordering::SeqCst);
            handle_companion_socket(socket, conn_id, device_id).await;
        }
        _ => {
            close_with_code(socket, CLOSE_UNPAIRED, "role must be 'host' or 'companion'").await;
        }
    }
}

async fn close_with_code(mut socket: WebSocket, code: u16, reason: &'static str) {
    let _ = socket
        .send(Message::Close(Some(CloseFrame { code, reason: reason.into() })))
        .await;
}

async fn handle_host_socket(mut socket: WebSocket) {
    let state = relay();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    *state.host_tx.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let Some(result) = incoming else { break };
                match result {
                    Ok(msg @ (Message::Text(_) | Message::Binary(_))) => {
                        state.dispatch(msg);
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {} // WS-protocol ping/pong — not app frames, nothing to forward
                    Err(_) => break,
                }
            }
            maybe_msg = rx.recv() => {
                match maybe_msg {
                    Some(msg) => { if socket.send(msg).await.is_err() { break; } }
                    None => break,
                }
            }
        }
    }

    // Clears host sender on disconnect (host reconnects on webview reload).
    *state.host_tx.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Manages companion WebSocket connection, outbox draining, and resync signaling.
async fn handle_companion_socket(mut socket: WebSocket, conn_id: u64, device_id: String) {
    let state = relay();
    let conn_key = format!("c{}", conn_id);
    let outbox: CompanionOutbox = Arc::new((StdMutex::new(Outbox::new()), Notify::new()));
    state.companions.lock().unwrap_or_else(|e| e.into_inner()).insert(
        conn_id,
        CompanionHandle { device_id, conn_key: conn_key.clone(), outbox: Arc::clone(&outbox) },
    );
    state.notify_host_companion_connected(&conn_key);

    let (lock, notify) = &*outbox;
    'conn: loop {
        tokio::select! {
            incoming = socket.recv() => {
                let Some(result) = incoming else { break };
                match result {
                    Ok(msg @ (Message::Text(_) | Message::Binary(_))) => {
                        // Stamped with THIS connection's relay-minted key so host addresses reply back to this socket (services/hostInvoke.js echoes `to`).
                        state.forward_to_host(stamp_from(msg, &conn_key));
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            _ = notify.notified() => {}
        }

        let (batch, wants_resync) = lock.lock().unwrap_or_else(|e| e.into_inner()).take();
        for msg in batch {
            let is_close = matches!(msg, Message::Close(_));
            if socket.send(msg).await.is_err() {
                break 'conn;
            }
            if is_close {
                break 'conn;
            }
        }
        if wants_resync {
            state.notify_host_companion_connected(&conn_key);
        }
    }

    state.companions.lock().unwrap_or_else(|e| e.into_inner()).remove(&conn_id);
}

// ── /pair (§13.1) ──────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct PairRequest {
    code: String,
}

#[derive(Serialize)]
struct PairResponse {
    token: String,
}

async fn pair_handler(headers: HeaderMap, Json(body): Json<PairRequest>) -> Response {
    let state = relay();
    if !state.enabled.load(Ordering::SeqCst) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "remote control is disabled on the host" })),
        )
            .into_response();
    }

    let expected = state.pairing_code.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if expected.is_empty() || body.code.trim() != expected {
        let failures = state.pair_failures.fetch_add(1, Ordering::SeqCst) + 1;
        if failures >= MAX_PAIR_FAILURES {
            // Disables server after consecutive bad pairing codes to mitigate brute-force attacks.
            state.enabled.store(false, Ordering::SeqCst);
            state.pairing_code.lock().unwrap_or_else(|e| e.into_inner()).clear();
            let _ = tauri::async_runtime::spawn_blocking(|| relay().persist_enabled(false)).await;
            eprintln!(
                "[web_server] {} consecutive bad pairing codes — remote control disabled, turn it back on to get a new code",
                failures
            );
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({ "error": "too many bad codes — remote control was disabled on the host" })),
            )
                .into_response();
        }
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "invalid code" }))).into_response();
    }
    state.pair_failures.store(0, Ordering::SeqCst);

    // Extracts device label from User-Agent header (truncated to 80 chars).
    let label = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().chars().take(80).collect::<String>())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Companion device".to_string());

    let device = PairedDevice { id: generate_id(), token: generate_token(), label, paired_at: now_secs() };
    let token = device.token.clone();
    state.devices.lock().unwrap_or_else(|e| e.into_inner()).push(device);

    let write_result = tauri::async_runtime::spawn_blocking(|| relay().persist_devices())
        .await
        .map_err(|e| format!("spawn_blocking panicked: {}", e))
        .and_then(|r| r);

    if let Err(e) = write_result {
        // Rolls back in-memory device if disk write fails to prevent orphaned tokens.
        state.devices.lock().unwrap_or_else(|e| e.into_inner()).retain(|d| d.token != token);
        eprintln!("[web_server] failed to persist paired device: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "failed to persist device" })),
        )
            .into_response();
    }

    (StatusCode::OK, Json(PairResponse { token })).into_response()
}

// ── Address classification (§7.1a, native `if-addrs` — no CLI shell-out) ─────────────────────

fn is_lan_v4(ip: &Ipv4Addr) -> bool {
    let o = ip.octets();
    (o[0] == 192 && o[1] == 168) || o[0] == 10 || (o[0] == 172 && (16..=31).contains(&o[1]))
}

/// Tailscale CGNAT range check (100.64.0.0/10).
fn is_tailscale_v4(ip: &Ipv4Addr) -> bool {
    let o = ip.octets();
    o[0] == 100 && (64..=127).contains(&o[1])
}

#[derive(Serialize)]
pub struct CompanionUrl {
    kind: &'static str,
    url: String,
}

// ── Tauri commands (all async + spawn_blocking per CLAUDE.md's never-block-UI rule) ──────────

/// Starts companion server: generates fresh pairing code, enables relay, and resets failure count.
#[tauri::command]
pub async fn start_companion_server() -> Result<CompanionServerInfo, String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<CompanionServerInfo, String> {
        let state = relay();
        let code = generate_pairing_code();
        *state.pairing_code.lock().unwrap_or_else(|e| e.into_inner()) = code.clone();
        state.pair_failures.store(0, Ordering::SeqCst);
        state.set_enabled(true);
        Ok(CompanionServerInfo { pairing_code: code, port: PORT })
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Stops companion server: disables relay and closes all active companion sockets with `CLOSE_SERVER_DISABLED`.
#[tauri::command]
pub async fn stop_companion_server() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<(), String> {
        let state = relay();
        state.set_enabled(false);
        let mut companions = state.companions.lock().unwrap_or_else(|e| e.into_inner());
        for (_, handle) in companions.drain() {
            enqueue(
                &handle.outbox,
                Message::Close(Some(CloseFrame {
                    code: CLOSE_SERVER_DISABLED,
                    reason: "remote control was turned off on the host".into(),
                })),
            );
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

#[derive(Serialize)]
pub struct CompanionServerInfo {
    pairing_code: String,
    port: u16,
}

/// Returns current relay status, pairing code, port, and process host token to host webview.
#[derive(Serialize)]
pub struct CompanionStatus {
    enabled: bool,
    pairing_code: String,
    port: u16,
    /// The process-local secret the `role=host` websocket requires (`RelayState::host_token`).
    #[serde(rename = "hostToken")]
    host_token: String,
}

#[tauri::command]
pub async fn get_companion_status() -> Result<CompanionStatus, String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<CompanionStatus, String> {
        let state = relay();
        Ok(CompanionStatus {
            enabled: state.enabled.load(Ordering::SeqCst),
            pairing_code: state.pairing_code.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            port: PORT,
            host_token: state.host_token.clone(),
        })
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Enumerates and classifies network interfaces into LAN and Tailscale URLs (plan §7.1a).
#[tauri::command]
pub async fn get_companion_url() -> Result<Vec<CompanionUrl>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut out = Vec::new();
        let interfaces = if_addrs::get_if_addrs().map_err(|e| format!("failed to enumerate interfaces: {}", e))?;
        for iface in interfaces {
            if iface.is_loopback() {
                continue;
            }
            match iface.addr {
                if_addrs::IfAddr::V4(v4) => {
                    if is_lan_v4(&v4.ip) {
                        out.push(CompanionUrl { kind: "lan", url: format!("http://{}:{}", v4.ip, PORT) });
                    } else if is_tailscale_v4(&v4.ip) {
                        out.push(CompanionUrl { kind: "tailscale", url: format!("http://{}:{}", v4.ip, PORT) });
                    }
                }
                if_addrs::IfAddr::V6(_) => {}
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

// ── Tailscale HTTPS (`tailscale serve`) ──────────────────────────────────────────────────────
// Exposes companion over HTTPS via MagicDNS to enable standalone PWA installation on mobile.

/// Resolves path to `tailscale` binary (probes well-known paths before system PATH).
fn tailscale_bin() -> String {
    for p in [
        "/opt/homebrew/bin/tailscale",
        "/usr/local/bin/tailscale",
        "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
    ] {
        if std::path::Path::new(p).exists() {
            return p.to_string();
        }
    }
    "tailscale".to_string()
}

fn run_tailscale(args: &[&str]) -> Result<std::process::Output, String> {
    crate::system::create_command(&tailscale_bin())
        .args(args)
        .output()
        .map_err(|e| format!("could not run tailscale ({}). Is Tailscale installed?", e))
}

/// Gets node MagicDNS HTTPS URL from `tailscale status --json`.
fn tailscale_https_url() -> Option<String> {
    let out = run_tailscale(&["status", "--json"]).ok()?;
    if !out.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let dns = json.get("Self")?.get("DNSName")?.as_str()?.trim_end_matches('.');
    if dns.is_empty() {
        None
    } else {
        Some(format!("https://{}/", dns))
    }
}

/// Returns true if `tailscale serve` is active for local port 1421.
fn tailscale_serve_on() -> bool {
    let target = format!("127.0.0.1:{}", PORT);
    run_tailscale(&["serve", "status"])
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&target))
        .unwrap_or(false)
}

#[derive(Serialize)]
pub struct TailscaleHttps {
    /// tailscale CLI present and usable.
    available: bool,
    /// serve is proxying our port right now.
    enabled: bool,
    /// `https://<magicdns>/` — present whenever the DNS name is readable, even while disabled.
    url: Option<String>,
}

/// Checks Tailscale HTTPS status and MagicDNS URL.
#[tauri::command]
pub async fn get_tailscale_https() -> Result<TailscaleHttps, String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<TailscaleHttps, String> {
        let url = tailscale_https_url();
        let available = url.is_some()
            || run_tailscale(&["version"]).map(|o| o.status.success()).unwrap_or(false);
        let enabled = available && tailscale_serve_on();
        Ok(TailscaleHttps { available, enabled, url })
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Configures `tailscale serve` for HTTPS relay (`--bg http://127.0.0.1:1421` or `--https=443 off`).
#[tauri::command]
pub async fn set_tailscale_https(enable: bool) -> Result<TailscaleHttps, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<TailscaleHttps, String> {
        let port_arg = format!("http://127.0.0.1:{}", PORT);
        let args: Vec<&str> = if enable {
            vec!["serve", "--bg", port_arg.as_str()]
        } else {
            vec!["serve", "--https=443", "off"]
        };
        let out = run_tailscale(&args)?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            let m = if stderr.trim().is_empty() { stdout.trim() } else { stderr.trim() };
            return Err(if m.is_empty() { "tailscale serve failed".to_string() } else { m.to_string() });
        }
        Ok(TailscaleHttps {
            available: true,
            enabled: tailscale_serve_on(),
            url: tailscale_https_url(),
        })
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

#[tauri::command]
pub async fn list_paired_devices() -> Result<Vec<PairedDeviceView>, String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<Vec<PairedDeviceView>, String> {
        let devices = relay().devices.lock().unwrap_or_else(|e| e.into_inner());
        Ok(devices.iter().map(PairedDeviceView::from).collect())
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Revokes paired device by id: removes from `companion-devices.json` and closes all associated sockets with `CLOSE_UNPAIRED`.
#[tauri::command]
pub async fn revoke_device(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = relay();
        let removed = {
            let mut devices = state.devices.lock().unwrap_or_else(|e| e.into_inner());
            let idx = devices.iter().position(|d| d.id == id);
            idx.map(|i| devices.remove(i))
        };
        if removed.is_none() {
            return Ok(());
        }
        state.persist_devices()?;

        let mut companions = state.companions.lock().unwrap_or_else(|e| e.into_inner());
        let dead: Vec<u64> = companions
            .iter()
            .filter(|(_, h)| h.device_id == id)
            .map(|(k, _)| *k)
            .collect();
        for k in dead {
            if let Some(handle) = companions.remove(&k) {
                enqueue(
                    &handle.outbox,
                    Message::Close(Some(CloseFrame {
                        code: CLOSE_UNPAIRED,
                        reason: "this device was revoked on the host".into(),
                    })),
                );
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Returns map of project id to base64 icon data URI or null (ICON-1).
#[tauri::command]
pub async fn get_project_icons_map(app: AppHandle) -> Result<HashMap<String, Option<String>>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let projects = crate::projects::load_projects_blocking(app)?;
        let cache = crate::system::get_project_icons().lock().unwrap_or_else(|e| e.into_inner());
        let mut map = HashMap::with_capacity(projects.len());
        for p in &projects {
            let uri = cache
                .get(&p.id)
                .map(|icon| format!("data:{};base64,{}", icon.mime_type, STANDARD.encode(&icon.bytes)));
            map.insert(p.id.clone(), uri);
        }
        Ok(map)
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Confined file reader (FILE-1): validates requested path resides within known project roots and enforces 2MB size limit.
#[tauri::command]
pub async fn read_text_file(app: AppHandle, path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let projects = crate::projects::load_projects_blocking(app)?;
        let requested = std::fs::canonicalize(&path).map_err(|e| format!("path not found: {}", e))?;

        let allowed = projects.iter().any(|p| {
            std::fs::canonicalize(&p.local_path)
                .map(|root| requested.starts_with(&root))
                .unwrap_or(false)
        });
        if !allowed {
            return Err("path is outside every project root — refusing to read".to_string());
        }

        const MAX_READ_BYTES: u64 = 2 * 1024 * 1024;
        let size = std::fs::metadata(&requested)
            .map_err(|e| format!("failed to stat file: {}", e))?
            .len();
        if size > MAX_READ_BYTES {
            return Err(format!(
                "file is too large to read ({} bytes, limit {} bytes)",
                size, MAX_READ_BYTES
            ));
        }

        std::fs::read_to_string(&requested).map_err(|e| format!("failed to read file: {}", e))
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of the fix: "your token is dead" and "the Mac is off right now" must never
    /// arrive as the same number again. A future edit that collapses them fails here.
    #[test]
    fn close_codes_are_distinct() {
        let codes = [CLOSE_UNPAIRED, CLOSE_SERVER_DISABLED, CLOSE_HOST_ROLE_REJECTED];
        for (i, a) in codes.iter().enumerate() {
            for b in codes.iter().skip(i + 1) {
                assert_ne!(a, b, "close codes must stay distinguishable on the wire");
            }
        }
        // 4001 keeps its shipped meaning (token rejected) so already-installed companions that only understand 4001 still behave correctly on a real revocation.
        assert_eq!(CLOSE_UNPAIRED, 4001);
    }

    /// A guessable or empty host token would leave the loopback-only guard as the real gate, which
    /// is exactly what `tailscale serve` defeats.
    #[test]
    fn host_token_is_a_full_length_random_hex_secret() {
        let a = RelayState::new();
        let b = RelayState::new();
        assert_eq!(a.host_token.len(), 32, "128-bit hex");
        assert!(a.host_token.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a.host_token, b.host_token, "must not be a constant");
    }

    // ── Backpressure / coalescing policy ─────────────────────────────────────────────────────
    //
    // These drive `Outbox` directly rather than through a socket: the policy is a property of the queue, and a test that needed a live phone to prove it would prove it on nobody's machine.

    fn text(t: &str, payload_bytes: usize) -> Message {
        Message::Text(serde_json::json!({ "t": t, "data": "x".repeat(payload_bytes) }).to_string())
    }

    /// The policy in one assertion per frame kind. If someone later widens what may be dropped,
    /// this is where it fails — a `delta` carrying an unanswered delete-confirmation is the frame
    /// this test exists to protect.
    #[test]
    fn only_terminal_output_may_be_coalesced() {
        assert!(is_coalescible(&text("pty_output", 8)), "terminal bytes are re-derivable from the host's scrollback");

        for kind in ["init", "delta", "invoke_result", "pty_exit", "pty_resize", "ping", "pong", "companion-connected", "intent"] {
            assert!(!is_coalescible(&text(kind, 8)), "`{}` is not re-derivable and must never be dropped", kind);
        }

        // Default-deny for anything this function was not taught about.
        assert!(!is_coalescible(&text("some-future-frame", 8)), "an unknown frame kind must be kept, not guessed at");
        assert!(!is_coalescible(&Message::Text("not json at all".into())), "an unparseable frame must be kept");
        assert!(!is_coalescible(&Message::Text("{}".into())), "a frame with no `t` must be kept");
        assert!(!is_coalescible(&Message::Binary(vec![1, 2, 3])), "binary frames are not classified and must be kept");
    }

    /// The core of the fix: over budget, the terminal backlog collapses and everything else survives
    /// **in order**, with the connection flagged for a re-hydrate.
    #[test]
    fn overflow_collapses_terminal_output_and_keeps_state_frames_in_order() {
        let mut ob = Outbox::new();
        let chunk = COMPANION_QUEUE_LIMIT_BYTES / 16;

        ob.push_within_budget(text("delta", 32));
        ob.push_within_budget(text("invoke_result", 32));

        // A firehosing shell, one flush at a time, until the budget blows.
        for _ in 0..64 {
            ob.push_within_budget(text("pty_output", chunk));
            if ob.resync_pending {
                break;
            }
        }

        assert!(ob.bytes <= COMPANION_QUEUE_LIMIT_BYTES, "coalescing must bring the queue back inside its budget");
        assert!(ob.resync_pending, "dropping output must flag the connection for a re-hydrate");
        let kinds: Vec<bool> = ob.queue.iter().map(is_coalescible).collect();
        assert_eq!(kinds, vec![false, false], "exactly the two undroppable frames survive");
        assert!(
            matches!(&ob.queue[0], Message::Text(s) if s.contains("delta")),
            "and they survive in their original order — the delta was queued first"
        );

        // The re-hydrate is requested only once the phone has actually drained the queue.
        let (batch, wants_resync) = ob.take();
        assert_eq!(batch.len(), 2);
        assert!(wants_resync);
        assert_eq!(ob.bytes, 0);
        let (_, again) = ob.take();
        assert!(!again, "one coalesce must produce exactly one resync request, not one per drain");
    }

    /// A phone so wedged that even undroppable frames blow the budget is cut loose rather than
    /// allowed to grow — the OOM path must be closed for every input, not just the nice one.
    #[test]
    fn a_backlog_of_undroppable_frames_closes_the_connection_instead_of_growing() {
        let mut ob = Outbox::new();
        let chunk = COMPANION_QUEUE_LIMIT_BYTES / 4;
        for _ in 0..8 {
            ob.push_within_budget(text("delta", chunk));
        }

        assert_eq!(ob.bytes, 0);
        assert!(ob.closed);
        assert_eq!(ob.queue.len(), 1, "the whole backlog is discarded — only the close survives");
        assert!(!ob.resync_pending, "a closing connection must not also ask the host for a snapshot");
        assert!(
            matches!(ob.queue.front(), Some(Message::Close(Some(f))) if f.code == CLOSE_TOO_FAR_BEHIND),
            "the queue must end in a close the companion can reconnect from"
        );

        // 1013 is not an app close code: the companion must reconnect, not treat itself as revoked.
        for app_code in [CLOSE_UNPAIRED, CLOSE_SERVER_DISABLED, CLOSE_HOST_ROLE_REJECTED] {
            assert_ne!(CLOSE_TOO_FAR_BEHIND, app_code);
        }
    }

    // ── Per-connection addressing ────────────────────────────────────────────────────────────

    /// Registers a companion on a fresh relay state and hands back its outbox, so a test can read
    /// exactly what `dispatch` decided to queue for it. Mints `conn_key` the same way
    /// `handle_companion_socket` does, so the tests address connections by the real wire value.
    fn register(state: &RelayState, conn_id: u64, device_id: &str) -> CompanionOutbox {
        let outbox: CompanionOutbox = Arc::new((StdMutex::new(Outbox::new()), Notify::new()));
        state.companions.lock().unwrap().insert(
            conn_id,
            CompanionHandle {
                device_id: device_id.to_string(),
                conn_key: format!("c{}", conn_id),
                outbox: Arc::clone(&outbox),
            },
        );
        outbox
    }

    fn queued(outbox: &CompanionOutbox) -> usize {
        outbox.0.lock().unwrap().queue.len()
    }

    /// Backward-compatibility guarantee: unaddressed frames reach all companions (pre-1.21.1 behavior).
    #[test]
    fn a_frame_with_no_address_still_reaches_every_companion() {
        let state = RelayState::new();
        let a = register(&state, 1, "device-a");
        let b = register(&state, 2, "device-b");

        state.dispatch(text("delta", 8));
        state.dispatch(Message::Text(r#"{"t":"pty_output","tab_id":0,"data":"x"}"#.into()));
        // Not addressable and not parseable — must still go everywhere rather than nowhere.
        state.dispatch(Message::Text("not json at all".into()));
        state.dispatch(Message::Binary(vec![1, 2, 3]));

        assert_eq!(queued(&a), 4, "an unaddressed frame is a broadcast, as it always was");
        assert_eq!(queued(&b), 4);
    }

    /// Bug 2 test: `invoke_result` is delivered strictly to the requesting connection.
    #[test]
    fn an_addressed_frame_reaches_only_that_connection() {
        let state = RelayState::new();
        let a = register(&state, 1, "device-a");
        let b = register(&state, 2, "device-b");

        state.dispatch(Message::Text(r#"{"t":"invoke_result","id":1,"ok":true,"to":"c2"}"#.into()));
        assert_eq!(queued(&a), 0, "no other socket may see this reply");
        assert_eq!(queued(&b), 1);

        // Bug 1: a joining phone's scrollback replay must not reset a phone that is mid-command.
        state.dispatch(Message::Text(r#"{"t":"pty_output","tab_id":0,"data":"","reset":true,"to":"c1"}"#.into()));
        assert_eq!(queued(&a), 1);
        assert_eq!(queued(&b), 1, "a replay addressed elsewhere must not clear this phone's screen");

        // An address nobody answers to is delivered to nobody, not to everybody.
        state.dispatch(Message::Text(r#"{"t":"invoke_result","id":2,"to":"c99"}"#.into()));
        assert_eq!(queued(&a), 1);
        assert_eq!(queued(&b), 1);
    }

    /// Two tabs on one phone have isolated request counters and outboxes (connection-level routing).
    #[test]
    fn two_connections_of_one_device_do_not_receive_each_others_frames() {
        let state = RelayState::new();
        let tab1 = register(&state, 1, "device-a");
        let tab2 = register(&state, 2, "device-a");

        // Both pages issued their first invoke, so both are waiting on id 1.
        state.dispatch(Message::Text(r#"{"t":"invoke_result","id":1,"ok":"answer-for-tab2","to":"c2"}"#.into()));
        assert_eq!(queued(&tab1), 0, "tab 1 must not resolve its own id-1 call with tab 2's answer");
        assert_eq!(queued(&tab2), 1);

        // One full scrollback replay per JOIN, and a join is per connection.
        state.dispatch(Message::Text(r#"{"t":"pty_output","tab_id":0,"data":"","reset":true,"to":"c2"}"#.into()));
        assert_eq!(queued(&tab1), 0, "one outbox must never receive a second connection's replay");
        assert_eq!(queued(&tab2), 2);

        // The device id is still what `revoke_device` groups on — connection addressing did not remove device grouping, it just stopped using it as the wire address.
        let companions = state.companions.lock().unwrap();
        assert_eq!(companions.values().filter(|h| h.device_id == "device-a").count(), 2);
    }

    /// `from` sender stamp is minted by relay and cannot be forged by companion.
    #[test]
    fn the_sender_stamp_cannot_be_forged_by_the_companion() {
        let stamped = stamp_from(Message::Text(r#"{"t":"invoke","id":1,"from":"c99"}"#.into()), "c1");
        let Message::Text(json) = stamped else { panic!("a text frame must stay a text frame") };
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["from"], "c1", "the relay's connection key must overwrite whatever the client claimed");
        assert_eq!(v["t"], "invoke", "no other field may be touched");
        assert_eq!(v["id"], 1);

        // A companion cannot smuggle a `to` past the stamp either — but note the real reason it is harmless is structural: inbound frames go to `forward_to_host`, never to `dispatch`.
        let stamped = stamp_from(Message::Text(r#"{"t":"invoke","id":1,"to":"c2"}"#.into()), "c1");
        let Message::Text(json) = stamped else { panic!("a text frame must stay a text frame") };
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["from"], "c1");

        // Anything the relay cannot parse as a JSON object is forwarded byte-for-byte.
        assert!(matches!(stamp_from(Message::Text("not json".into()), "c1"), Message::Text(s) if s == "not json"));
        assert!(matches!(stamp_from(Message::Text("[1,2]".into()), "c1"), Message::Text(s) if s == "[1,2]"));
        assert!(matches!(stamp_from(Message::Binary(vec![7]), "c1"), Message::Binary(b) if b == vec![7]));
    }

    // ── INVARIANT R ──────────────────────────────────────────────────────────────────────────
    // Asserts sizing relationship between `pty::MAX_TABS`, `pty::SCROLLBACK_CAP`, and `COMPANION_QUEUE_LIMIT_BYTES`.

    /// Bytes one tab's scrollback occupies in a `pty_output` replay frame's JSON text. base64 is `ceil(n/3) * 4` (128 covers the JSON envelope).
    fn replay_frame_bytes(scrollback_cap: usize) -> usize {
        scrollback_cap.div_ceil(3) * 4 + 128
    }

    /// Bytes for one full scrollback replay across all tabs.
    fn one_replay_bytes() -> usize {
        crate::pty::MAX_TABS * replay_frame_bytes(crate::pty::SCROLLBACK_CAP)
    }

    /// **R1**: recovery replay fits with 50% budget reserved for undroppable state frames.
    #[test]
    fn invariant_r1_a_recovery_replay_fits_with_room_for_undroppable_frames() {
        let one = one_replay_bytes();
        assert!(
            one <= COMPANION_QUEUE_LIMIT_BYTES / 2,
            "INVARIANT R1 broken: a {}-tab scrollback replay is {} bytes, over half the {}-byte companion budget. \
             A replay is the RECOVERY path and shares the queue with an `init` and pending `delta`s; once it exceeds \
             what the queue can hold, coalesce() eats it, the resync loop re-issues it, and the phone never receives \
             the early tabs at all. Raise COMPANION_QUEUE_LIMIT_BYTES, lower pty::SCROLLBACK_CAP, or lower \
             pty::MAX_TABS (and src/store/terminalTabsStore.js's MAX_TABS with it) — in this same commit.",
            crate::pty::MAX_TABS,
            one,
            COMPANION_QUEUE_LIMIT_BYTES
        );
    }

    /// **R2**: concurrent addressed replay and broadcast congestion replay do not exceed full budget.
    #[test]
    fn invariant_r2_the_reachable_double_replay_does_not_trip_a_coalesce() {
        let double = 2 * one_replay_bytes();
        assert!(
            double <= COMPANION_QUEUE_LIMIT_BYTES,
            "INVARIANT R2 broken: an addressed replay plus a concurrent broadcast congestion replay is {} bytes \
             against a {}-byte budget, so the ordinary two-replay overlap now coalesces and costs the joining phone \
             its rehydrate. Same three dials as R1.",
            double,
            COMPANION_QUEUE_LIMIT_BYTES
        );
    }

    /// Verifies addressed replay delivers one replay per outbox (not N per device).
    #[test]
    fn an_addressed_replay_is_one_replay_per_outbox_not_one_per_connection_on_the_device() {
        let state = RelayState::new();
        let tab1 = register(&state, 1, "device-a");
        let tab2 = register(&state, 2, "device-a");

        // Three joins on one device: each answered with its own addressed replay.
        for key in ["c1", "c2", "c1"] {
            state.dispatch(Message::Text(
                format!(r#"{{"t":"pty_output","tab_id":0,"data":"","reset":true,"to":"{}"}}"#, key),
            ));
        }
        assert_eq!(queued(&tab1), 2, "each outbox holds exactly the replays addressed to it");
        assert_eq!(queued(&tab2), 1);

        // The arithmetic the assertion above protects, stated so a reader does not have to re-derive what a device-level address would have cost.
        let one = one_replay_bytes();
        assert!(2 * one > COMPANION_QUEUE_LIMIT_BYTES / 2, "two replays in one outbox would break R1");
        assert!(3 * one > COMPANION_QUEUE_LIMIT_BYTES, "three would blow the budget outright");
    }

    /// Verifies budget headroom constants against plan §2.2 arithmetic.
    #[test]
    fn the_budget_keeps_the_headroom_it_was_sized_for() {
        assert_eq!(replay_frame_bytes(128 * 1024), 174_892, "base64 expansion is 4/3, not something else");
        assert_eq!(COMPANION_QUEUE_LIMIT_BYTES, 8 * 1024 * 1024);
        let one = one_replay_bytes();
        assert_eq!(one, 2_798_272, "a full 16-tab replay is ~2.67 MiB on the wire");
        assert_eq!(2 * one, 5_596_544, "R2's modelled pair is ~5.34 MiB, 67% of the budget");
        assert!(
            COMPANION_QUEUE_LIMIT_BYTES / one >= 2,
            "the budget must stay at least 2x a full replay — that ratio IS invariant R1"
        );
    }

    /// CLAUDE.md serde rule: missing/corrupt `companion-server.json` defaults to disabled.
    #[test]
    fn persisted_server_state_defaults_to_off() {
        assert!(!serde_json::from_str::<PersistedServerState>("{}").unwrap().enabled);
        assert!(serde_json::from_str::<PersistedServerState>(r#"{"enabled":true}"#).unwrap().enabled);
        assert!(!PersistedServerState::default().enabled);
    }
}
