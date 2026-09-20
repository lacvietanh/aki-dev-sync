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
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
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

/// Bad `/pair` codes tolerated from ONE source address before pairing — not the server — is locked.
const MAX_PAIR_FAILURES: u32 = 10;

/// Ceiling behind the per-address counter so a distributed flood still costs only a cooling window.
const MAX_PAIR_FAILURES_GLOBAL: u32 = 100;

/// How long `/pair` stays refused after the strike count is reached. Nothing about the lock is persisted, and `enabled` is never touched (docs/plan/remote-ingress-rework.md §4).
const PAIR_LOCK_SECS: u64 = 300;

/// Idle strike records older than this are dropped, so a scan cannot grow the map without bound.
const PAIR_RECORD_TTL_SECS: u64 = 900;

/// Hard cap on tracked source addresses; the least recently seen record is dropped first.
const MAX_PAIR_IP_RECORDS: usize = 512;

// ── Ingress modes (docs/plan/remote-ingress-rework.md §5) ─────────────────────────────────────
/// App-managed `tailscale serve` — the default and the only mode that touches Tailscale at all.
const INGRESS_TAILSCALE: &str = "tailscale";
/// The owner runs the edge; the app only stores and displays the origin it terminates at.
const INGRESS_PUBLIC: &str = "public";

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

/// Per-companion outbound budget (asserted in tests below).
///
/// Sizing: one tab's replay is ~175 KB (128 KiB scrollback × base64 4/3 + ~128 B envelope). The budget holds at least 2× one tab's replay so a single-tab resync never triggers another coalesce. With unbounded tabs the total replay may exceed the budget; `coalesce` handles that gracefully: first overflow schedules a resync, a second overflow with no scrollback delivered since the last resync stops scheduling resyncs and keeps the connection alive for live output (avoiding an endless coalesce/resync loop).
///
/// INVARIANT R:
/// - **R1**: `replay_frame_bytes(SCROLLBACK_CAP) <= COMPANION_QUEUE_LIMIT_BYTES / 2` (~175 KB <= 4 MiB). Ensures a single-tab recovery replay always fits with headroom for undroppable state frames. Budget is per-connection so multiple tabs on one device have isolated queues.
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
    /// Set after the first resync is requested; prevents a second overflow from looping forever.
    /// Cleared when a non-resync batch drains successfully (connection is making progress again).
    resync_suppressed: bool,
    /// Set when connection teardown begins (Close frame queued); ignores subsequent pushes.
    closed: bool,
}

/// Shared between the relay's producers (the host-socket loop, `revoke_device`,
/// `stop_companion_server`) and the one companion task that owns the socket.
type CompanionOutbox = Arc<(StdMutex<Outbox>, Notify)>;

impl Outbox {
    fn new() -> Self {
        Outbox {
            queue: VecDeque::new(),
            bytes: 0,
            resync_pending: false,
            resync_suppressed: false,
            closed: false,
        }
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
    ///
    /// A second overflow after a resync was already requested (resync_suppressed) does not schedule
    /// another resync — that would loop forever when the total replay exceeds the budget. Instead,
    /// live output continues to flow and the connection stays open until it makes enough progress
    /// (a non-resync drain) to clear the suppression.
    fn coalesce(&mut self) {
        let before = self.queue.len();
        self.queue.retain(|m| !is_coalescible(m));
        if self.queue.len() != before {
            if !self.resync_suppressed {
                self.resync_pending = true;
                self.resync_suppressed = true;
            }
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
            reason: "this device fell too far behind the host — reconnect for a fresh snapshot"
                .into(),
        })));
        self.closed = true;
    }

    /// Hands backlog to socket writer; returns `(batch, wants_resync)` where resync is true only if coalesce occurred and queue is now empty.
    /// A non-resync delivery clears resync suppression so a future overflow may retry.
    fn take(&mut self) -> (Vec<Message>, bool) {
        let batch: Vec<Message> = self.queue.drain(..).collect();
        self.bytes = 0;
        let resync = self.resync_pending;
        self.resync_pending = false;
        if !resync && !batch.is_empty() {
            self.resync_suppressed = false;
        }
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
/// COALESCIBLE (re-derivable from host scrollback): non-reset `pty_output` chunks.
/// Reset frames are authoritative state transitions and must never be dropped.
///
/// NEVER DROPPED (default-deny for unrecognized/binary):
/// - `init` / `delta`: mirrored app state (including confirmation dialogs).
/// - `invoke_result`: RPC reply matching companion request id.
/// - `pty_exit`, `pty_resize`: terminal liveness and dimension edges.
/// - `ping` / `pong`: connection health.
fn is_coalescible(msg: &Message) -> bool {
    let Message::Text(text) = msg else {
        return false;
    };
    #[derive(Deserialize)]
    struct FrameTag {
        t: Option<String>,
        #[serde(default)]
        reset: bool,
    }
    match serde_json::from_str::<FrameTag>(text) {
        Ok(tag) => tag.t.as_deref() == Some(FRAME_PTY_OUTPUT) && !tag.reset,
        Err(_) => false,
    }
}

/// Reads optional top-level `to` naming recipient connection key (`c<conn_id>`). Absent or invalid returns `None` (broadcast). Parsed once per host frame in `dispatch`.
fn addressed_to(msg: &Message) -> Option<String> {
    let Message::Text(text) = msg else {
        return None;
    };
    #[derive(Deserialize)]
    struct FrameAddress {
        to: Option<String>,
    }
    serde_json::from_str::<FrameAddress>(text)
        .ok()
        .and_then(|f| f.to)
}

/// Stamps sending connection key `c<conn_id>` unconditionally onto inbound companion JSON frames for host reply routing.
fn stamp_from(msg: Message, conn_key: &str) -> Message {
    let Message::Text(text) = msg else { return msg };
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(serde_json::Value::Object(mut map)) => {
            map.insert(
                "from".to_string(),
                serde_json::Value::String(conn_key.to_string()),
            );
            Message::Text(serde_json::Value::Object(map).to_string())
        }
        _ => Message::Text(text),
    }
}

/// Queues a frame into companion outbox without blocking the host loop.
fn enqueue(outbox: &CompanionOutbox, msg: Message) {
    let (lock, notify) = &**outbox;
    lock.lock()
        .unwrap_or_else(|e| e.into_inner())
        .push_within_budget(msg);
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
    /// Incremented on every disable so in-flight companion handshakes that passed auth before the
    /// disable can detect the race and refuse registration (P1-2).
    connection_epoch: AtomicU64,
    /// Long-form pairing secret accepted in place of the 6-digit code on an origin the whole internet can also type into. Minted with the code, never persisted.
    pair_link_token: StdMutex<String>,
    /// Strike records and the cooling lock they trigger; in memory only.
    pair_gate: StdMutex<PairGate>,
    /// Which edge produces the public URL, mirrored from `companion-server.json`.
    ingress: StdMutex<IngressSetting>,
}

/// One source address's recent bad-code history.
#[derive(Default)]
struct PairAttempts {
    failures: u32,
    locked_until: u64,
    last_seen: u64,
}

/// The `/pair` throttle: strikes counted per source address, with a global ceiling behind them.
#[derive(Default)]
struct PairGate {
    per_ip: HashMap<IpAddr, PairAttempts>,
    global_failures: u32,
    global_locked_until: u64,
}

impl PairGate {
    /// Seconds this address must wait, or `None` when it may try now.
    fn retry_after(&self, ip: IpAddr, now: u64) -> Option<u64> {
        let until = self
            .global_locked_until
            .max(self.per_ip.get(&ip).map(|a| a.locked_until).unwrap_or(0));
        (until > now).then(|| until - now)
    }

    fn record_failure(&mut self, ip: IpAddr, now: u64) {
        self.global_failures += 1;
        if self.global_failures >= MAX_PAIR_FAILURES_GLOBAL {
            self.global_locked_until = now + PAIR_LOCK_SECS;
            self.global_failures = 0;
        }
        let entry = self.per_ip.entry(ip).or_default();
        entry.failures += 1;
        entry.last_seen = now;
        if entry.failures >= MAX_PAIR_FAILURES {
            entry.locked_until = now + PAIR_LOCK_SECS;
            entry.failures = 0;
        }
    }

    fn record_success(&mut self, ip: IpAddr) {
        self.per_ip.remove(&ip);
        self.global_failures = 0;
    }

    /// Drops records that are neither locked nor recently active, then trims the least recently seen until the map is back inside its cap.
    fn prune(&mut self, now: u64) {
        self.per_ip.retain(|_, a| {
            a.locked_until > now || now.saturating_sub(a.last_seen) < PAIR_RECORD_TTL_SECS
        });
        while self.per_ip.len() > MAX_PAIR_IP_RECORDS {
            let Some(oldest) = self
                .per_ip
                .iter()
                .min_by_key(|(_, a)| a.last_seen)
                .map(|(ip, _)| *ip)
            else {
                break;
            };
            self.per_ip.remove(&oldest);
        }
    }
}

/// Outcome of one `/pair` attempt, decided entirely from relay state so it is testable without a socket.
enum PairVerdict {
    Disabled,
    /// Refused for this many more seconds.
    Locked(u64),
    Rejected,
    Accepted,
}

/// The one stored ingress decision: which edge produces the public URL, and the origin it resolves to.
#[derive(Clone)]
struct IngressSetting {
    mode: String,
    origin: String,
}

impl Default for IngressSetting {
    fn default() -> Self {
        IngressSetting {
            mode: INGRESS_TAILSCALE.to_string(),
            origin: String::new(),
        }
    }
}

/// Anything that is not a mode this build knows falls back to `tailscale`, so a newer or hand-edited file cannot leave the app in a mode it cannot serve.
fn normalize_ingress(mode: &str, origin: &str) -> IngressSetting {
    let mode = if mode == INGRESS_PUBLIC {
        INGRESS_PUBLIC
    } else {
        INGRESS_TAILSCALE
    };
    IngressSetting {
        mode: mode.to_string(),
        origin: trim_origin(origin),
    }
}

/// Stored origins carry no trailing slash, so every consumer can append a path unconditionally.
fn trim_origin(origin: &str) -> String {
    origin.trim().trim_end_matches('/').to_string()
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
            host_token: generate_token().expect("OS entropy unavailable: cannot mint host token"),
            enabled: AtomicBool::new(false),
            connection_epoch: AtomicU64::new(0),
            pair_link_token: StdMutex::new(String::new()),
            pair_gate: StdMutex::new(PairGate::default()),
            ingress: StdMutex::new(IngressSetting::default()),
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
        let devices = self
            .devices
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let content = serde_json::to_string_pretty(&devices).map_err(|e| e.to_string())?;
        std::fs::write(&path, content).map_err(|e| e.to_string())
    }

    /// Writes the whole `companion-server.json` from live state — the LAST EXPLICIT on/off choice
    /// so a restart resumes it (see `init`), plus the ingress decision — so saving one can never
    /// drop the other. Same blocking-write discipline as `persist_devices`: callers on the async
    /// runtime must route it through `spawn_blocking`. Best-effort by design — failing to write
    /// this preference must never fail the toggle the user just asked for.
    fn persist_server_state(&self) {
        let Some(path) = self
            .server_state_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        else {
            return;
        };
        let ingress = self
            .ingress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let persisted = PersistedServerState {
            enabled: self.enabled.load(Ordering::SeqCst),
            ingress_mode: ingress.mode,
            ingress_origin: ingress.origin,
        };
        match serde_json::to_string(&persisted) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&path, content) {
                    eprintln!(
                        "[web_server] could not persist the remote-control state: {}",
                        e
                    );
                }
            }
            Err(e) => eprintln!(
                "[web_server] could not serialize the remote-control state: {}",
                e
            ),
        }
    }

    /// Flip the gate and remember the choice in one call, so no path can change one without the
    /// other and leave disk disagreeing with memory.
    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
        self.persist_server_state();
    }

    /// The whole pairing decision in one place: gate first, then secret. A bad code costs a cooling
    /// window and never the server itself (docs/plan/remote-ingress-rework.md §4).
    fn judge_pair_attempt(&self, code: &str, ip: IpAddr, now: u64) -> PairVerdict {
        if !self.enabled.load(Ordering::SeqCst) {
            return PairVerdict::Disabled;
        }
        let mut gate = self.pair_gate.lock().unwrap_or_else(|e| e.into_inner());
        gate.prune(now);
        if let Some(retry_after) = gate.retry_after(ip, now) {
            return PairVerdict::Locked(retry_after);
        }
        let offered = code.trim();
        let expected = self
            .pairing_code
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let link = self
            .pair_link_token
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if (expected.is_empty() || offered != expected) && (link.is_empty() || offered != link) {
            gate.record_failure(ip, now);
            return match gate.retry_after(ip, now) {
                Some(retry_after) => PairVerdict::Locked(retry_after),
                None => PairVerdict::Rejected,
            };
        }
        gate.record_success(ip);
        PairVerdict::Accepted
    }

    /// Mints the 6-digit code and its long-form twin together, so no path can hand out one without
    /// the other. Both values are generated before any state is written, so a failure leaves the
    /// existing state unchanged — no partially-written pair of secrets.
    fn mint_pairing_secrets(&self) -> Result<String, String> {
        let code = generate_pairing_code()?;
        let link = generate_token()?;
        *self.pairing_code.lock().unwrap_or_else(|e| e.into_inner()) = code.clone();
        *self
            .pair_link_token
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = link;
        Ok(code)
    }

    /// Turns the gate off and closes every registered companion. Registration checks the gate and inserts under the same `companions` lock this drains under, so once this returns no handshake that authenticated earlier can still insert.
    fn disable_and_drain(&self) {
        self.set_enabled(false);
        self.connection_epoch.fetch_add(1, Ordering::SeqCst);
        let mut companions = self.companions.lock().unwrap_or_else(|e| e.into_inner());
        for (_, handle) in companions.drain() {
            enqueue(
                &handle.outbox,
                Message::Close(Some(CloseFrame {
                    code: CLOSE_SERVER_DISABLED,
                    reason: "remote control was turned off on the host".into(),
                })),
            );
        }
    }

    /// Attempts to register a companion connection under the companions lock. Returns false if the
    /// server was disabled or the epoch advanced (a disable raced past auth — P1-2 invariant).
    fn try_register_companion(
        &self,
        conn_id: u64,
        device_id: String,
        conn_key: String,
        outbox: CompanionOutbox,
        epoch_at_auth: u64,
    ) -> bool {
        let mut companions = self.companions.lock().unwrap_or_else(|e| e.into_inner());
        if !self.enabled.load(Ordering::SeqCst)
            || self.connection_epoch.load(Ordering::SeqCst) != epoch_at_auth
        {
            return false;
        }
        companions.insert(
            conn_id,
            CompanionHandle {
                device_id,
                conn_key,
                outbox,
            },
        );
        true
    }
}

/// `companion-server.json` — the relay state that outlives the process. `#[serde(default)]`
/// per CLAUDE.md's serde rule, so an older/partial file degrades to "off" on the Tailscale ingress
/// instead of failing the read.
#[derive(Serialize, Deserialize)]
struct PersistedServerState {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_ingress_mode", rename = "ingressMode")]
    ingress_mode: String,
    #[serde(default, rename = "ingressOrigin")]
    ingress_origin: String,
}

fn default_ingress_mode() -> String {
    INGRESS_TAILSCALE.to_string()
}

impl Default for PersistedServerState {
    fn default() -> Self {
        PersistedServerState {
            enabled: false,
            ingress_mode: default_ingress_mode(),
            ingress_origin: String::new(),
        }
    }
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
        PairedDeviceView {
            id: d.id.clone(),
            label: d.label.clone(),
            paired_at: d.paired_at,
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Fails closed: returns an error if OS entropy is unavailable. No secret may be produced
/// from predictable state. A `cfg(test)` thread-local flag enables deterministic failure tests.
#[cfg(test)]
thread_local! {
    static FAIL_ENTROPY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn random_bytes<const N: usize>() -> Result<[u8; N], String> {
    #[cfg(test)]
    if FAIL_ENTROPY.with(|f| f.get()) {
        return Err("simulated entropy failure".to_string());
    }
    let mut buf = [0u8; N];
    getrandom::getrandom(&mut buf).map_err(|e| format!("OS entropy unavailable: {}", e))?;
    Ok(buf)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// §13.1: "a random 128-bit hex string".
fn generate_token() -> Result<String, String> {
    Ok(hex_encode(&random_bytes::<16>()?))
}

/// Device id — shorter, only needs to be unique among paired devices, never sent as a secret.
fn generate_id() -> Result<String, String> {
    Ok(hex_encode(&random_bytes::<8>()?))
}

/// §7.1: "Mac shows a 6-digit code".
fn generate_pairing_code() -> Result<String, String> {
    let n = u32::from_be_bytes(random_bytes::<4>()?) % 1_000_000;
    Ok(format!("{:06}", n))
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
                    Ok(devices) => {
                        *state.devices.lock().unwrap_or_else(|e| e.into_inner()) = devices
                    }
                    Err(e) => eprintln!(
                        "[web_server] companion-devices.json is corrupt, starting empty: {}",
                        e
                    ),
                }
            }
            *state.devices_path.lock().unwrap_or_else(|e| e.into_inner()) = Some(path);

            let server_state_path = dir.join("companion-server.json");
            let restored = std::fs::read_to_string(&server_state_path)
                .ok()
                .and_then(|c| serde_json::from_str::<PersistedServerState>(&c).ok())
                .unwrap_or_default();
            *state
                .server_state_path
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Some(server_state_path);
            *state.ingress.lock().unwrap_or_else(|e| e.into_inner()) =
                normalize_ingress(&restored.ingress_mode, &restored.ingress_origin);
            if restored.enabled {
                match state.mint_pairing_secrets() {
                    Ok(_) => state.enabled.store(true, Ordering::SeqCst),
                    Err(e) => eprintln!(
                        "[web_server] OS entropy unavailable at startup — remote control not restored: {}",
                        e
                    ),
                }
            }
        }
        Err(e) => {
            eprintln!(
                "[web_server] could not resolve app data dir, pairing will not persist: {}",
                e
            );
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
            eprintln!(
                "[web_server] failed to bind {}: {} — companion server will not start",
                addr, e
            );
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
        .route("/pair", post(pair_handler));

    if cfg!(debug_assertions) {
        let vite_origin = resolve_vite_origin(app);
        eprintln!(
            "[web_server] dev mode: proxying non-relay requests to vite on {}",
            vite_origin
        );

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
        return (
            StatusCode::NOT_IMPLEMENTED,
            "websocket upgrades are not proxied in dev (see docs/plan/done/remote-control.md §7.2)",
        )
            .into_response();
    }

    let (parts, body) = req.into_parts();

    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("failed to read request body: {e}"),
            )
                .into_response()
        }
    };

    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("/");
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
            eprintln!(
                "[web_server] dev proxy: vite at {} unreachable: {}",
                vite_origin, e
            );
            return (
                StatusCode::BAD_GATEWAY,
                format!("vite dev server unreachable at {vite_origin}: {e}"),
            )
                .into_response();
        }
    };

    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
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
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            format!("failed reading vite response body: {e}"),
        )
            .into_response(),
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
    let path = if raw_path.is_empty() {
        "index.html"
    } else {
        raw_path
    };

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
                close_with_code(
                    socket,
                    CLOSE_SERVER_DISABLED,
                    "remote control is disabled on the host",
                )
                .await;
                return;
            }
            let epoch_at_auth = state.connection_epoch.load(Ordering::SeqCst);
            let token = q.token.clone().unwrap_or_default();
            let device_id = {
                let devices = state.devices.lock().unwrap_or_else(|e| e.into_inner());
                devices
                    .iter()
                    .find(|d| d.token == token)
                    .map(|d| d.id.clone())
            };
            let Some(device_id) = device_id else {
                close_with_code(socket, CLOSE_UNPAIRED, "invalid or unpaired token").await;
                return;
            };
            let conn_id = state.next_id.fetch_add(1, Ordering::SeqCst);
            handle_companion_socket(socket, conn_id, device_id, epoch_at_auth).await;
        }
        _ => {
            close_with_code(socket, CLOSE_UNPAIRED, "role must be 'host' or 'companion'").await;
        }
    }
}

async fn close_with_code(mut socket: WebSocket, code: u16, reason: &'static str) {
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code,
            reason: reason.into(),
        })))
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
async fn handle_companion_socket(
    mut socket: WebSocket,
    conn_id: u64,
    device_id: String,
    epoch_at_auth: u64,
) {
    let state = relay();
    let conn_key = format!("c{}", conn_id);
    let outbox: CompanionOutbox = Arc::new((StdMutex::new(Outbox::new()), Notify::new()));
    if !state.try_register_companion(
        conn_id,
        device_id,
        conn_key.clone(),
        Arc::clone(&outbox),
        epoch_at_auth,
    ) {
        close_with_code(
            socket,
            CLOSE_SERVER_DISABLED,
            "remote control was disabled before registration completed",
        )
        .await;
        return;
    }
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

    state
        .companions
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&conn_id);
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

async fn pair_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<PairRequest>,
) -> Response {
    let state = relay();
    match state.judge_pair_attempt(&body.code, addr.ip(), now_secs()) {
        PairVerdict::Disabled => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "remote control is disabled on the host" })),
            )
                .into_response()
        }
        PairVerdict::Locked(retry_after) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({ "error": "too many bad codes — pairing is locked for a few minutes", "retryAfterSecs": retry_after })),
            )
                .into_response()
        }
        PairVerdict::Rejected => {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "invalid code" }))).into_response()
        }
        PairVerdict::Accepted => {}
    }

    // Extracts device label from User-Agent header (truncated to 80 chars).
    let label = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().chars().take(80).collect::<String>())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Companion device".to_string());

    let id = match generate_id() {
        Ok(id) => id,
        Err(e) => {
            eprintln!("[web_server] entropy failure during pairing: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "server error" })),
            )
                .into_response();
        }
    };
    let token = match generate_token() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[web_server] entropy failure during pairing: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "server error" })),
            )
                .into_response();
        }
    };
    let device = PairedDevice {
        id,
        token: token.clone(),
        label,
        paired_at: now_secs(),
    };
    state
        .devices
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(device);

    let write_result = tauri::async_runtime::spawn_blocking(|| relay().persist_devices())
        .await
        .map_err(|e| format!("spawn_blocking panicked: {}", e))
        .and_then(|r| r);

    if let Err(e) = write_result {
        // Rolls back in-memory device if disk write fails to prevent orphaned tokens.
        state
            .devices
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|d| d.token != token);
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

/// Starts companion server: mints a fresh pairing code and link token, clears the pairing throttle, and enables the relay.
#[tauri::command]
pub async fn start_companion_server() -> Result<CompanionServerInfo, String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<CompanionServerInfo, String> {
        let state = relay();
        let code = state.mint_pairing_secrets()?;
        *state.pair_gate.lock().unwrap_or_else(|e| e.into_inner()) = PairGate::default();
        state.set_enabled(true);
        Ok(CompanionServerInfo {
            pairing_code: code,
            port: PORT,
        })
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Stops companion server: disables relay and closes all active companion sockets with `CLOSE_SERVER_DISABLED`.
#[tauri::command]
pub async fn stop_companion_server() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<(), String> {
        relay().disable_and_drain();
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
    /// Long-form pairing secret `/pair` accepts in place of the 6-digit code, so the host UI can build a one-tap pairing link for a public origin.
    #[serde(rename = "pairLinkToken")]
    pair_link_token: String,
}

#[tauri::command]
pub async fn get_companion_status() -> Result<CompanionStatus, String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<CompanionStatus, String> {
        let state = relay();
        Ok(CompanionStatus {
            enabled: state.enabled.load(Ordering::SeqCst),
            pairing_code: state
                .pairing_code
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            port: PORT,
            host_token: state.host_token.clone(),
            pair_link_token: state
                .pair_link_token
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
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
        let interfaces = if_addrs::get_if_addrs()
            .map_err(|e| format!("failed to enumerate interfaces: {}", e))?;
        for iface in interfaces {
            if iface.is_loopback() {
                continue;
            }
            match iface.addr {
                if_addrs::IfAddr::V4(v4) => {
                    if is_lan_v4(&v4.ip) {
                        out.push(CompanionUrl {
                            kind: "lan",
                            url: format!("http://{}:{}", v4.ip, PORT),
                        });
                    } else if is_tailscale_v4(&v4.ip) {
                        out.push(CompanionUrl {
                            kind: "tailscale",
                            url: format!("http://{}:{}", v4.ip, PORT),
                        });
                    }
                }
                if_addrs::IfAddr::V6(_) => {}
            }
        }
        let ingress = relay()
            .ingress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if ingress.mode == INGRESS_PUBLIC && !ingress.origin.is_empty() {
            out.push(CompanionUrl {
                kind: "public",
                url: ingress.origin,
            });
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
    let dns = json
        .get("Self")?
        .get("DNSName")?
        .as_str()?
        .trim_end_matches('.');
    if dns.is_empty() {
        None
    } else {
        Some(format!("https://{}/", dns))
    }
}

/// Who holds the node's 443 `/` mount. `tailscale serve` and `tailscale funnel` share one per-node
/// config, so a sibling app's Funnel and this app's serve compete for the same handler — see
/// docs/research/remote-ingress-tailscale-conflict.md F1.
#[derive(PartialEq, Debug)]
enum MountOwner {
    Ours,
    /// Held by something else, described by its proxy target.
    Foreign(String),
    Vacant,
}

/// Parses a `ServeConfig` as printed by `tailscale serve status --json`: `Web["<host>:443"].Handlers["/"]`.
fn parse_mount_owner(status_json: &str, target: &str) -> MountOwner {
    let Ok(config) = serde_json::from_str::<serde_json::Value>(status_json) else {
        return MountOwner::Vacant;
    };
    let Some(web) = config.get("Web").and_then(|w| w.as_object()) else {
        return MountOwner::Vacant;
    };
    for (host_port, server) in web {
        if !host_port.ends_with(":443") {
            continue;
        }
        let Some(handler) = server.get("Handlers").and_then(|h| h.get("/")) else {
            continue;
        };
        let proxy = handler
            .get("Proxy")
            .and_then(|p| p.as_str())
            .unwrap_or_default();
        if proxy.contains(target) {
            return MountOwner::Ours;
        }
        return MountOwner::Foreign(if proxy.is_empty() {
            handler.to_string()
        } else {
            proxy.to_string()
        });
    }
    MountOwner::Vacant
}

/// The ONE ownership check. Enable and disable both route through it, so the safety cannot be lost
/// by a caller that forgets it (`pattern.A8`). A CLI that will not run reads as `Vacant`, which is
/// the conservative answer for both: enable proceeds, disable does nothing.
fn mount_owner() -> MountOwner {
    let target = format!("127.0.0.1:{}", PORT);
    match run_tailscale(&["serve", "status", "--json"]) {
        Ok(out) if out.status.success() => {
            parse_mount_owner(&String::from_utf8_lossy(&out.stdout), &target)
        }
        _ => MountOwner::Vacant,
    }
}

#[derive(Serialize)]
pub struct TailscaleHttps {
    /// tailscale CLI present and usable.
    available: bool,
    /// serve is proxying our port right now.
    enabled: bool,
    /// `https://<magicdns>/` — present whenever the DNS name is readable, even while disabled.
    url: Option<String>,
    /// What holds the 443 `/` mount when this app does not, so the UI can name it instead of the app stealing it.
    #[serde(rename = "foreignTarget")]
    foreign_target: Option<String>,
}

/// Reads the live Tailscale state around one already-determined mount owner.
fn https_state(owner: MountOwner) -> TailscaleHttps {
    let url = tailscale_https_url();
    let available = url.is_some()
        || run_tailscale(&["version"])
            .map(|o| o.status.success())
            .unwrap_or(false);
    let (enabled, foreign_target) = match owner {
        MountOwner::Ours => (true, None),
        MountOwner::Foreign(target) => (false, Some(target)),
        MountOwner::Vacant => (false, None),
    };
    TailscaleHttps {
        available,
        enabled,
        url,
        foreign_target,
    }
}

/// In `public` mode the edge is the owner's, so the app reports Tailscale as none of its business rather than probing it (docs/plan/remote-ingress-rework.md §5).
fn tailscale_unmanaged() -> TailscaleHttps {
    TailscaleHttps {
        available: false,
        enabled: false,
        url: None,
        foreign_target: None,
    }
}

fn ingress_mode() -> String {
    relay()
        .ingress
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .mode
        .clone()
}

/// Checks Tailscale HTTPS status and MagicDNS URL.
#[tauri::command]
pub async fn get_tailscale_https() -> Result<TailscaleHttps, String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<TailscaleHttps, String> {
        if ingress_mode() == INGRESS_PUBLIC {
            return Ok(tailscale_unmanaged());
        }
        Ok(https_state(mount_owner()))
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Mounts or unmounts THIS app's own 443 `/` handler and nothing else: it refuses to take a mount
/// another app holds, and unmounting is a no-op unless the handler there proxies our port
/// (docs/plan/remote-ingress-rework.md §3).
#[tauri::command]
pub async fn set_tailscale_https(enable: bool) -> Result<TailscaleHttps, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<TailscaleHttps, String> {
        if ingress_mode() == INGRESS_PUBLIC {
            if enable {
                return Err("remote ingress is set to a public origin — Tailscale is not managed by the app in this mode".to_string());
            }
            return Ok(tailscale_unmanaged());
        }
        let owner = mount_owner();
        if let (true, MountOwner::Foreign(target)) = (enable, &owner) {
            return Err(format!(
                "the tailnet's 443 / mount is currently served to {} — turn that off first, or switch this app to a public ingress",
                target
            ));
        }
        if !enable && owner != MountOwner::Ours {
            return Ok(https_state(owner));
        }
        let proxy_target = format!("http://127.0.0.1:{}", PORT);
        let args: Vec<&str> = if enable {
            vec!["serve", "--bg", proxy_target.as_str()]
        } else {
            vec!["serve", "--https=443", "--set-path=/", "off"]
        };
        let out = run_tailscale(&args)?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            let m = if stderr.trim().is_empty() { stdout.trim() } else { stderr.trim() };
            return Err(if m.is_empty() { "tailscale serve failed".to_string() } else { m.to_string() });
        }
        Ok(https_state(mount_owner()))
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

// ── Remote ingress (docs/plan/remote-ingress-rework.md §5) ───────────────────────────────────

#[derive(Serialize)]
pub struct RemoteIngress {
    mode: String,
    origin: String,
    /// A hint only — see `suggested_public_origin`.
    #[serde(rename = "suggestedOrigin")]
    suggested_origin: Option<String>,
}

/// Best-effort hint from the sibling app `aki-mcp-sv`, which records its own edge in
/// `~/.aki/mcpsv/ingress.json`. Every failure — no file, no permission, an unexpected shape — is
/// `None`: Dev Sync must never need that file to work.
fn suggested_public_origin() -> Option<String> {
    let path = dirs::home_dir()?
        .join(".aki")
        .join("mcpsv")
        .join("ingress.json");
    let content = std::fs::read_to_string(path).ok()?;
    let config: serde_json::Value = serde_json::from_str(&content).ok()?;
    sibling_origin(config.get("origin")?.as_str()?, "devsync")
}

/// `https://mcp.example.com` → `https://devsync.example.com`: same domain, sibling subdomain. An
/// origin with no subdomain to replace yields nothing rather than a guess at the apex.
fn sibling_origin(origin: &str, label: &str) -> Option<String> {
    let rest = origin
        .trim()
        .strip_prefix("https://")
        .or_else(|| origin.trim().strip_prefix("http://"))?;
    let host = rest.split('/').next()?.split(':').next()?;
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 3 || labels.iter().any(|l| l.is_empty()) {
        return None;
    }
    Some(format!("https://{}.{}", label, labels[1..].join(".")))
}

fn remote_ingress_view() -> RemoteIngress {
    let ingress = relay()
        .ingress
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    RemoteIngress {
        mode: ingress.mode,
        origin: ingress.origin,
        suggested_origin: suggested_public_origin(),
    }
}

#[tauri::command]
pub async fn get_remote_ingress() -> Result<RemoteIngress, String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<RemoteIngress, String> {
        Ok(remote_ingress_view())
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Stores the ingress decision. `origin: None` leaves the stored origin as it is, so switching mode
/// back and forth does not cost the owner the hostname they typed.
#[tauri::command]
pub async fn set_remote_ingress(
    mode: String,
    origin: Option<String>,
) -> Result<RemoteIngress, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<RemoteIngress, String> {
        if mode != INGRESS_TAILSCALE && mode != INGRESS_PUBLIC {
            return Err(format!(
                "unknown ingress mode '{}' — expected '{}' or '{}'",
                mode, INGRESS_TAILSCALE, INGRESS_PUBLIC
            ));
        }
        let state = relay();
        {
            let mut ingress = state.ingress.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(origin) = origin {
                let origin = trim_origin(&origin);
                if !origin.is_empty()
                    && !origin.starts_with("https://")
                    && !origin.starts_with("http://")
                {
                    return Err(format!(
                        "'{}' is not a full origin — it must start with https:// or http://",
                        origin
                    ));
                }
                ingress.origin = origin;
            }
            ingress.mode = mode;
        }
        state.persist_server_state();
        Ok(remote_ingress_view())
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
pub async fn get_project_icons_map(
    app: AppHandle,
) -> Result<HashMap<String, Option<String>>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let projects = crate::projects::load_projects_blocking(app)?;
        let cache = crate::system::get_project_icons()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut map = HashMap::with_capacity(projects.len());
        for p in &projects {
            let uri = cache.get(&p.id).map(|icon| {
                format!(
                    "data:{};base64,{}",
                    icon.mime_type,
                    STANDARD.encode(&icon.bytes)
                )
            });
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
        let requested =
            std::fs::canonicalize(&path).map_err(|e| format!("path not found: {}", e))?;

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
        let codes = [
            CLOSE_UNPAIRED,
            CLOSE_SERVER_DISABLED,
            CLOSE_HOST_ROLE_REJECTED,
        ];
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
        assert!(
            is_coalescible(&text("pty_output", 8)),
            "terminal bytes are re-derivable from the host's scrollback"
        );

        assert!(
            !is_coalescible(&Message::Text(
                serde_json::json!({ "t": "pty_output", "data": "", "reset": true }).to_string()
            )),
            "an authoritative reset must survive coalescing"
        );

        for kind in [
            "init",
            "delta",
            "invoke_result",
            "pty_exit",
            "pty_resize",
            "ping",
            "pong",
            "companion-connected",
            "intent",
        ] {
            assert!(
                !is_coalescible(&text(kind, 8)),
                "`{}` is not re-derivable and must never be dropped",
                kind
            );
        }

        // Default-deny for anything this function was not taught about.
        assert!(
            !is_coalescible(&text("some-future-frame", 8)),
            "an unknown frame kind must be kept, not guessed at"
        );
        assert!(
            !is_coalescible(&Message::Text("not json at all".into())),
            "an unparseable frame must be kept"
        );
        assert!(
            !is_coalescible(&Message::Text("{}".into())),
            "a frame with no `t` must be kept"
        );
        assert!(
            !is_coalescible(&Message::Binary(vec![1, 2, 3])),
            "binary frames are not classified and must be kept"
        );
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

        assert!(
            ob.bytes <= COMPANION_QUEUE_LIMIT_BYTES,
            "coalescing must bring the queue back inside its budget"
        );
        assert!(
            ob.resync_pending,
            "dropping output must flag the connection for a re-hydrate"
        );
        let kinds: Vec<bool> = ob.queue.iter().map(is_coalescible).collect();
        assert_eq!(
            kinds,
            vec![false, false],
            "exactly the two undroppable frames survive"
        );
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
        assert!(
            !again,
            "one coalesce must produce exactly one resync request, not one per drain"
        );
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
        assert_eq!(
            ob.queue.len(),
            1,
            "the whole backlog is discarded — only the close survives"
        );
        assert!(
            !ob.resync_pending,
            "a closing connection must not also ask the host for a snapshot"
        );
        assert!(
            matches!(ob.queue.front(), Some(Message::Close(Some(f))) if f.code == CLOSE_TOO_FAR_BEHIND),
            "the queue must end in a close the companion can reconnect from"
        );

        // 1013 is not an app close code: the companion must reconnect, not treat itself as revoked.
        for app_code in [
            CLOSE_UNPAIRED,
            CLOSE_SERVER_DISABLED,
            CLOSE_HOST_ROLE_REJECTED,
        ] {
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

    fn new_outbox() -> CompanionOutbox {
        Arc::new((StdMutex::new(Outbox::new()), Notify::new()))
    }

    /// P1-1: an entropy failure must mint nothing and leave the previous secrets untouched.
    #[test]
    fn entropy_failure_mints_no_secret_and_keeps_existing_state() {
        let state = RelayState::new();
        *state.pairing_code.lock().unwrap() = "123456".to_string();
        *state.pair_link_token.lock().unwrap() = "old-link".to_string();
        FAIL_ENTROPY.with(|f| f.set(true));
        let minted = state.mint_pairing_secrets();
        let token = generate_token();
        FAIL_ENTROPY.with(|f| f.set(false));
        assert!(minted.is_err());
        assert!(token.is_err());
        assert_eq!(*state.pairing_code.lock().unwrap(), "123456");
        assert_eq!(*state.pair_link_token.lock().unwrap(), "old-link");
    }

    /// P1-2: a companion that authenticated before a disable must not insert after it, and re-enabling admits a fresh connection.
    #[test]
    fn disable_after_auth_rejects_the_late_registration_and_leaves_registry_empty() {
        let state = RelayState::new();
        state.enabled.store(true, Ordering::SeqCst);
        let epoch_at_auth = state.connection_epoch.load(Ordering::SeqCst);

        state.disable_and_drain();

        assert!(!state.try_register_companion(
            1,
            "dev".into(),
            "c1".into(),
            new_outbox(),
            epoch_at_auth
        ));
        assert!(state.companions.lock().unwrap().is_empty());

        state.enabled.store(true, Ordering::SeqCst);
        let fresh_epoch = state.connection_epoch.load(Ordering::SeqCst);
        assert!(state.try_register_companion(
            2,
            "dev".into(),
            "c2".into(),
            new_outbox(),
            fresh_epoch
        ));
    }

    /// P1-2: disabling closes sockets that were already registered.
    #[test]
    fn disable_closes_and_clears_registered_companions() {
        let state = RelayState::new();
        state.enabled.store(true, Ordering::SeqCst);
        let outbox = register(&state, 1, "dev");
        state.disable_and_drain();
        assert!(state.companions.lock().unwrap().is_empty());
        assert_eq!(queued(&outbox), 1, "one Close frame queued");
    }

    /// Backward-compatibility guarantee: unaddressed frames reach all companions (pre-1.21.1 behavior).
    #[test]
    fn a_frame_with_no_address_still_reaches_every_companion() {
        let state = RelayState::new();
        let a = register(&state, 1, "device-a");
        let b = register(&state, 2, "device-b");

        state.dispatch(text("delta", 8));
        state.dispatch(Message::Text(
            r#"{"t":"pty_output","tab_id":0,"data":"x"}"#.into(),
        ));
        // Not addressable and not parseable — must still go everywhere rather than nowhere.
        state.dispatch(Message::Text("not json at all".into()));
        state.dispatch(Message::Binary(vec![1, 2, 3]));

        assert_eq!(
            queued(&a),
            4,
            "an unaddressed frame is a broadcast, as it always was"
        );
        assert_eq!(queued(&b), 4);
    }

    /// Bug 2 test: `invoke_result` is delivered strictly to the requesting connection.
    #[test]
    fn an_addressed_frame_reaches_only_that_connection() {
        let state = RelayState::new();
        let a = register(&state, 1, "device-a");
        let b = register(&state, 2, "device-b");

        state.dispatch(Message::Text(
            r#"{"t":"invoke_result","id":1,"ok":true,"to":"c2"}"#.into(),
        ));
        assert_eq!(queued(&a), 0, "no other socket may see this reply");
        assert_eq!(queued(&b), 1);

        // Bug 1: a joining phone's scrollback replay must not reset a phone that is mid-command.
        state.dispatch(Message::Text(
            r#"{"t":"pty_output","tab_id":0,"data":"","reset":true,"to":"c1"}"#.into(),
        ));
        assert_eq!(queued(&a), 1);
        assert_eq!(
            queued(&b),
            1,
            "a replay addressed elsewhere must not clear this phone's screen"
        );

        // An address nobody answers to is delivered to nobody, not to everybody.
        state.dispatch(Message::Text(
            r#"{"t":"invoke_result","id":2,"to":"c99"}"#.into(),
        ));
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
        state.dispatch(Message::Text(
            r#"{"t":"invoke_result","id":1,"ok":"answer-for-tab2","to":"c2"}"#.into(),
        ));
        assert_eq!(
            queued(&tab1),
            0,
            "tab 1 must not resolve its own id-1 call with tab 2's answer"
        );
        assert_eq!(queued(&tab2), 1);

        // One full scrollback replay per JOIN, and a join is per connection.
        state.dispatch(Message::Text(
            r#"{"t":"pty_output","tab_id":0,"data":"","reset":true,"to":"c2"}"#.into(),
        ));
        assert_eq!(
            queued(&tab1),
            0,
            "one outbox must never receive a second connection's replay"
        );
        assert_eq!(queued(&tab2), 2);

        // The device id is still what `revoke_device` groups on — connection addressing did not remove device grouping, it just stopped using it as the wire address.
        let companions = state.companions.lock().unwrap();
        assert_eq!(
            companions
                .values()
                .filter(|h| h.device_id == "device-a")
                .count(),
            2
        );
    }

    /// `from` sender stamp is minted by relay and cannot be forged by companion.
    #[test]
    fn the_sender_stamp_cannot_be_forged_by_the_companion() {
        let stamped = stamp_from(
            Message::Text(r#"{"t":"invoke","id":1,"from":"c99"}"#.into()),
            "c1",
        );
        let Message::Text(json) = stamped else {
            panic!("a text frame must stay a text frame")
        };
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            v["from"], "c1",
            "the relay's connection key must overwrite whatever the client claimed"
        );
        assert_eq!(v["t"], "invoke", "no other field may be touched");
        assert_eq!(v["id"], 1);

        // A companion cannot smuggle a `to` past the stamp either — but note the real reason it is harmless is structural: inbound frames go to `forward_to_host`, never to `dispatch`.
        let stamped = stamp_from(
            Message::Text(r#"{"t":"invoke","id":1,"to":"c2"}"#.into()),
            "c1",
        );
        let Message::Text(json) = stamped else {
            panic!("a text frame must stay a text frame")
        };
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["from"], "c1");

        // Anything the relay cannot parse as a JSON object is forwarded byte-for-byte.
        assert!(
            matches!(stamp_from(Message::Text("not json".into()), "c1"), Message::Text(s) if s == "not json")
        );
        assert!(
            matches!(stamp_from(Message::Text("[1,2]".into()), "c1"), Message::Text(s) if s == "[1,2]")
        );
        assert!(
            matches!(stamp_from(Message::Binary(vec![7]), "c1"), Message::Binary(b) if b == vec![7])
        );
    }

    // ── INVARIANT R ──────────────────────────────────────────────────────────────────────────
    // Asserts per-tab sizing relationship between `pty::SCROLLBACK_CAP` and `COMPANION_QUEUE_LIMIT_BYTES`.

    /// Bytes one tab's scrollback occupies in a `pty_output` replay frame's JSON text. base64 is `ceil(n/3) * 4` (128 covers the JSON envelope).
    fn replay_frame_bytes(scrollback_cap: usize) -> usize {
        scrollback_cap.div_ceil(3) * 4 + 128
    }

    /// Bytes for one tab's worst-case scrollback replay frame on the wire.
    fn one_tab_replay_bytes() -> usize {
        replay_frame_bytes(crate::pty::SCROLLBACK_CAP)
    }

    /// **R1**: one tab's worst-case replay fits within 25 % of the outbox budget.
    #[test]
    fn invariant_r1_a_recovery_replay_fits_with_room_for_undroppable_frames() {
        assert!(
            one_tab_replay_bytes() * 4 <= COMPANION_QUEUE_LIMIT_BYTES,
            "INVARIANT R1 broken: one tab's scrollback replay ({} bytes) must fit within 25 % of the \
             {}-byte companion budget. Raise COMPANION_QUEUE_LIMIT_BYTES or lower pty::SCROLLBACK_CAP.",
            one_tab_replay_bytes(),
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
            state.dispatch(Message::Text(format!(
                r#"{{"t":"pty_output","tab_id":0,"data":"","reset":true,"to":"{}"}}"#,
                key
            )));
        }
        assert_eq!(
            queued(&tab1),
            2,
            "each outbox holds exactly the replays addressed to it"
        );
        assert_eq!(queued(&tab2), 1);
    }

    /// Verifies budget headroom constants against per-tab arithmetic.
    #[test]
    fn the_budget_keeps_the_headroom_it_was_sized_for() {
        assert_eq!(
            replay_frame_bytes(128 * 1024),
            174_892,
            "base64 expansion is 4/3, not something else"
        );
        assert_eq!(COMPANION_QUEUE_LIMIT_BYTES, 8 * 1024 * 1024);
        assert!(
            one_tab_replay_bytes() * 4 <= COMPANION_QUEUE_LIMIT_BYTES,
            "one tab's replay ({} bytes) must fit within 25 % of the {}-byte outbox budget",
            one_tab_replay_bytes(),
            COMPANION_QUEUE_LIMIT_BYTES
        );
    }

    /// CLAUDE.md serde rule: missing/corrupt `companion-server.json` defaults to disabled.
    #[test]
    fn persisted_server_state_defaults_to_off() {
        assert!(
            !serde_json::from_str::<PersistedServerState>("{}")
                .unwrap()
                .enabled
        );
        assert!(
            serde_json::from_str::<PersistedServerState>(r#"{"enabled":true}"#)
                .unwrap()
                .enabled
        );
        assert!(!PersistedServerState::default().enabled);
    }

    // ── W1: who owns the tailnet's 443 `/` mount ─────────────────────────────────────────────

    /// The detector this replaced grepped `serve status` text for our port, so a sibling app's
    /// Funnel on the same mount read as "off" and enabling then silently stole it.
    #[test]
    fn the_443_mount_is_ours_only_when_it_proxies_our_port() {
        let target = format!("127.0.0.1:{}", PORT);
        let ours = r#"{"TCP":{"443":{"HTTPS":true}},"Web":{"mac.tail1234.ts.net:443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:1421"}}}}}"#;
        let funnel = r#"{"TCP":{"443":{"HTTPS":true}},"Web":{"mac.tail1234.ts.net:443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:9999"}}}},"AllowFunnel":{"mac.tail1234.ts.net:443":true}}"#;

        assert_eq!(parse_mount_owner(ours, &target), MountOwner::Ours);
        assert_eq!(parse_mount_owner(funnel, &target), MountOwner::Foreign("http://127.0.0.1:9999".to_string()), "a foreign mount must be reported WITH its target, so the UI can name what is holding it");
        assert_eq!(
            parse_mount_owner("{}", &target),
            MountOwner::Vacant,
            "an empty serve config is what a fresh node prints"
        );
        assert_eq!(
            parse_mount_owner("not json at all", &target),
            MountOwner::Vacant
        );
        assert_eq!(
            parse_mount_owner(
                r#"{"Web":{"mac.tail1234.ts.net:8443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:9999"}}}}}"#,
                &target
            ),
            MountOwner::Vacant,
            "another port is not the mount this app manages"
        );
        assert_eq!(
            parse_mount_owner(
                r#"{"Web":{"mac.tail1234.ts.net:443":{"Handlers":{"/mcp":{"Proxy":"http://127.0.0.1:9999"}}}}}"#,
                &target
            ),
            MountOwner::Vacant,
            "another path is not the mount this app manages"
        );
    }

    // ── W2: the pairing gate throttles, it does not shut the server down ─────────────────────

    fn enabled_relay() -> (RelayState, String) {
        let state = RelayState::new();
        let code = state.mint_pairing_secrets().unwrap();
        state.enabled.store(true, Ordering::SeqCst);
        (state, code)
    }

    fn ip(n: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, n))
    }

    /// The regression this exists to prevent: ten bad codes from an unauthenticated stranger used
    /// to switch remote control off and persist that off, recoverable only at the Mac itself.
    #[test]
    fn a_bad_code_storm_locks_pairing_and_leaves_the_server_enabled() {
        let (state, code) = enabled_relay();
        let now = 1_000_000;
        for _ in 0..MAX_PAIR_FAILURES * 3 {
            state.judge_pair_attempt("not-the-code", ip(7), now);
        }

        assert!(
            state.enabled.load(Ordering::SeqCst),
            "a stranger must never be able to switch remote control off"
        );
        assert!(
            !state.pairing_code.lock().unwrap().is_empty(),
            "the owner's pairing code must survive the storm"
        );
        assert!(
            matches!(
                state.judge_pair_attempt(&code, ip(7), now),
                PairVerdict::Locked(_)
            ),
            "the attacking address is throttled, even with the right code"
        );
        assert!(
            matches!(
                state.judge_pair_attempt(&code, ip(8), now),
                PairVerdict::Accepted
            ),
            "one address's strikes must not lock everybody out"
        );
        assert!(
            matches!(
                state.judge_pair_attempt(&code, ip(7), now + PAIR_LOCK_SECS + 1),
                PairVerdict::Accepted
            ),
            "the lock is a window and expires on its own"
        );

        state.enabled.store(false, Ordering::SeqCst);
        assert!(matches!(
            state.judge_pair_attempt(&code, ip(8), now),
            PairVerdict::Disabled
        ));
    }

    /// The difference between a throttle and a kill switch: devices that already paired are outside
    /// the gate entirely, so a flood costs a delayed re-pair and never a live session.
    #[test]
    fn an_already_paired_device_still_authenticates_after_a_lock() {
        let (state, code) = enabled_relay();
        let now = 1_000_000;
        assert!(matches!(
            state.judge_pair_attempt(&code, ip(1), now),
            PairVerdict::Accepted
        ));
        let device = PairedDevice {
            id: generate_id().unwrap(),
            token: generate_token().unwrap(),
            label: "phone".into(),
            paired_at: now,
        };
        let token = device.token.clone();
        state.devices.lock().unwrap().push(device);

        for _ in 0..MAX_PAIR_FAILURES_GLOBAL * 2 {
            state.judge_pair_attempt("not-the-code", ip(2), now);
        }

        assert!(state.enabled.load(Ordering::SeqCst));
        // Exactly the lookup `handle_socket` performs for a `role=companion` connection.
        let known = state
            .devices
            .lock()
            .unwrap()
            .iter()
            .any(|d| d.token == token);
        assert!(
            known,
            "a lock on pairing must not revoke a device that already paired"
        );
    }

    /// A distributed flood must still land on a cooling window, never on the old outcome.
    #[test]
    fn a_flood_from_many_addresses_hits_the_global_ceiling_not_an_off_switch() {
        let (state, code) = enabled_relay();
        let now = 1_000_000;
        for i in 0..MAX_PAIR_FAILURES_GLOBAL {
            state.judge_pair_attempt("not-the-code", ip((i % 200) as u8 + 1), now);
        }

        assert!(state.enabled.load(Ordering::SeqCst));
        assert!(
            matches!(
                state.judge_pair_attempt(&code, ip(250), now),
                PairVerdict::Locked(_)
            ),
            "the ceiling holds even for an address with no strikes of its own"
        );
        assert!(matches!(
            state.judge_pair_attempt(&code, ip(250), now + PAIR_LOCK_SECS + 1),
            PairVerdict::Accepted
        ));
    }

    /// Six digits are safe on a LAN because of the strike counter; a public origin needs a secret
    /// the whole internet cannot also type. Both arrive in the same `code` field.
    #[test]
    fn either_the_six_digit_code_or_the_long_link_token_pairs() {
        let (state, code) = enabled_relay();
        let link = state.pair_link_token.lock().unwrap().clone();
        let now = 1_000_000;

        assert_eq!(code.len(), 6);
        assert_eq!(link.len(), 32, "128-bit hex");
        assert!(link.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(matches!(
            state.judge_pair_attempt(&link, ip(1), now),
            PairVerdict::Accepted
        ));
        assert!(
            matches!(
                state.judge_pair_attempt(&format!("  {}  ", code), ip(1), now),
                PairVerdict::Accepted
            ),
            "a code pasted with whitespace still pairs"
        );
        assert!(matches!(
            state.judge_pair_attempt("not-the-code", ip(1), now),
            PairVerdict::Rejected
        ));

        // Both secrets share one lifetime: minting replaces the pair, so a restart invalidates an old link.
        state.mint_pairing_secrets().unwrap();
        assert!(matches!(
            state.judge_pair_attempt(&link, ip(1), now),
            PairVerdict::Rejected
        ));
    }

    /// The throttle must not become a memory leak an internet-wide scan can drive.
    #[test]
    fn the_per_address_record_map_stays_bounded() {
        let mut gate = PairGate::default();
        let now = 1_000_000;
        for i in 0..(MAX_PAIR_IP_RECORDS as u32 * 4) {
            gate.record_failure(IpAddr::V4(Ipv4Addr::from(i + 1)), now);
            gate.prune(now);
        }
        assert!(gate.per_ip.len() <= MAX_PAIR_IP_RECORDS);

        gate.prune(now + PAIR_RECORD_TTL_SECS + PAIR_LOCK_SECS + 1);
        assert!(
            gate.per_ip.is_empty(),
            "records with nothing left to remember are dropped once they go quiet"
        );
    }

    // ── W3: the ingress mode ─────────────────────────────────────────────────────────────────

    /// CLAUDE.md serde rule: an install written before this feature must load as Tailscale, unchanged.
    #[test]
    fn persisted_ingress_defaults_to_tailscale() {
        let old = serde_json::from_str::<PersistedServerState>(r#"{"enabled":true}"#).unwrap();
        assert!(old.enabled);
        assert_eq!(old.ingress_mode, INGRESS_TAILSCALE);
        assert_eq!(old.ingress_origin, "");
        assert_eq!(
            PersistedServerState::default().ingress_mode,
            INGRESS_TAILSCALE
        );

        let saved = serde_json::from_str::<PersistedServerState>(r#"{"enabled":true,"ingressMode":"public","ingressOrigin":"https://devsync.example.com"}"#).unwrap();
        assert_eq!(saved.ingress_mode, INGRESS_PUBLIC);
        assert_eq!(saved.ingress_origin, "https://devsync.example.com");

        assert_eq!(
            normalize_ingress("cloudflared", "").mode,
            INGRESS_TAILSCALE,
            "a mode this build cannot serve falls back rather than sticking"
        );
        assert_eq!(
            normalize_ingress(INGRESS_PUBLIC, " https://x.example.com/// ").origin,
            "https://x.example.com",
            "a stored origin never keeps a trailing slash"
        );
    }

    /// The sibling-app hint is a suggestion, never a dependency.
    #[test]
    fn the_sibling_origin_hint_is_best_effort() {
        assert_eq!(
            sibling_origin("https://mcp.example.com", "devsync").as_deref(),
            Some("https://devsync.example.com")
        );
        assert_eq!(
            sibling_origin("https://mcp.example.com/", "devsync").as_deref(),
            Some("https://devsync.example.com")
        );
        assert_eq!(
            sibling_origin("https://mcp.a.b.example.com", "devsync").as_deref(),
            Some("https://devsync.a.b.example.com")
        );

        for junk in [
            "",
            "example.com",
            "https://example.com",
            "ftp://mcp.example.com",
            "https://",
            "https://mcp..com",
        ] {
            assert_eq!(
                sibling_origin(junk, "devsync"),
                None,
                "`{}` must yield no hint at all",
                junk
            );
        }
    }
}
