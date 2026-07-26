// Remote Control relay — see docs/plan/done/remote-control.md §7 (Rust — relay only), §7.0 (ICON-1),
// §7.1 + §7.1a (pairing + Tailscale), §7.5 (FileView / read_text_file / FILE-1), and §13 (the
// FROZEN wire-protocol contract). This module implements exactly that contract; every protocol
// decision (what a `delta`/`intent`/`invoke` frame means) lives in the JS seams — this file is
// content-blind routing plus the small set of native operations the plan calls out (pairing,
// icon scan-and-hold, confined file read, LAN/Tailscale address discovery).
//
// INVARIANT (§13.6): the WS relay never holds mirrored app state and never transforms a payload it
// is asked to forward. Two deliberate, documented departures from strict content-blindness:
//
//  1. (plan §2.3) The relay itself ORIGINATES a `{"t":"companion-connected","id":<deviceId>}` frame
//     to the host right after a companion's token check passes, so the host knows to push an `init`
//     snapshot. That frame is not in §13.2's table (a gap in the frozen doc as written); it is
//     produced by the relay about a *connection lifecycle event*, not derived from parsing any
//     client payload.
//  2. (backpressure, below) The relay READS ONE FIELD — the top-level `t` — of frames already
//     sitting in a companion's outbound queue, and only when that queue has blown its byte budget.
//     It never reads `t` on the hot path, never reads any other field, and never rewrites a frame.
//     This is a real, if narrow, widening of the relay's knowledge: it now knows that `pty_output`
//     is re-derivable from the host and everything else is not (see `is_coalescible`). The
//     alternative — a content-blind cap — can only drop arbitrary frames, which is precisely the
//     "phone renders a corrupted half-stream" outcome the policy exists to avoid.

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

/// Fixed relay port — also baked into `tauri.conf.json`'s CSP `connect-src` and every
/// `get_companion_url()` result. Not user-configurable (see plan §7.1a "Dev vs prod" table).
const PORT: u16 = 1421;

/// How many consecutive bad `/pair` codes are tolerated before the relay shuts itself off.
const MAX_PAIR_FAILURES: u32 = 10;

// ── WS close codes (mirrored in src/constants/protocol.js — keep the two in step) ─────────────
//
// These three used to be one undifferentiated `4001`, and that ambiguity was the whole bug: the
// companion could not tell "your token is dead" from "the Mac's relay is not accepting right now",
// so it took the destructive reading and wiped a perfectly good token on every app restart and
// every Off. Splitting them is what lets the client keep a token it has no reason to distrust.

/// The token presented is unknown / absent / revoked. The companion MUST drop it and fall back to
/// code entry — it will fail identically on every retry otherwise.
const CLOSE_UNPAIRED: u16 = 4001;
/// Remote control is switched off (or was just switched off) on the Mac. Says nothing at all about
/// the token: the companion KEEPS it and reconnects with backoff.
const CLOSE_SERVER_DISABLED: u16 = 4002;
/// A `role=host` connection was refused (not loopback, or the process-local host secret did not
/// match). Only the Mac's own webview ever sees this; it re-reads the secret and retries.
const CLOSE_HOST_ROLE_REJECTED: u16 = 4003;

// ── Backpressure toward companions ────────────────────────────────────────────────────────────
//
// THE SHAPE OF THE PROBLEM. A companion used to be fed by an UNBOUNDED mpsc: the host's frames
// were pushed in at whatever rate the Mac produced them, and drained at whatever rate the phone
// could accept them. Those two rates are unrelated, and one of them has no ceiling — a runaway
// shell (`yes`) in the in-app terminal emits a `pty_output` frame every ~20ms forever. A phone on
// a weak link that cannot keep up therefore grows a queue that nothing bounds, in the Mac's
// address space, until the app dies. That is the whole bug: not "the queue is too big" but "there
// is no relationship at all between production and consumption".
//
// WHY NOT THE TWO OBVIOUS FIXES.
//  * Block the producer (a bounded `send().await`). The producer is the relay's host-socket loop,
//    which serves EVERY companion and also carries the host's own inbound frames. Stalling it to
//    wait for the slowest phone hands one bad link the power to freeze the terminal for everyone,
//    and — through the WS backpressure chain — eventually the shell itself.
//  * Drop arbitrary frames at a cap. A terminal byte stream with a hole in it is not "slightly
//    stale", it is corrupt: xterm renders whatever escape sequence the surviving half implies. A
//    phone showing a coherent screen that is 3 seconds old is strictly better than one showing
//    scrambled output it will never correct.
//
// THE POLICY: COALESCE, THEN RE-HYDRATE. Every companion gets a byte-budgeted outbox. When the
// budget is blown, the backlog is collapsed by dropping exactly the frames the host can regenerate
// in full — `pty_output`, whose entire content is re-derivable from `pty_get_scrollback` — and the
// connection is flagged for a re-hydrate. Once the phone has drained what remains (i.e. it is
// actually able to receive again), the relay re-issues the `companion-connected` frame it already
// originates on join. The host's existing handlers answer that with a full `init` snapshot plus a
// `reset:true` scrollback replay — the exact "put this screen back in a known-good state" path that
// already runs on every join and every reconnect. No new frame type, no new host-side code, and the
// recovery path is the one that gets exercised constantly rather than a special one that only runs
// under a fault.
//
// Re-hydrating only once the queue has DRAINED is what keeps this self-limiting without a timer or
// a counter: a phone that is still behind has not drained, so it cannot trigger another resync.
// Under a sustained firehose the phone therefore settles into "one coherent full snapshot whenever
// it can absorb one", which is the best a slow link can be given.

/// Per-companion outbound budget. Sized so four paired phones cannot cost more than ~8 MB of the
/// Mac's RAM between them even while all four are wedged, and so a single budget still holds
/// several seconds of a firehosing terminal (~21 KB of base64 per flush, ~50 flushes/s) — enough
/// that an ordinary hiccup is absorbed silently and only a genuinely stuck link ever coalesces.
const COMPANION_QUEUE_LIMIT_BYTES: usize = 2 * 1024 * 1024;

/// Frame tag of the ONE coalescible kind. Mirrors `FRAME_PTY_OUTPUT` in `src/constants/protocol.js`
/// — same two-file mirroring convention as the close codes above, for the same reason (the JS
/// constant file is the protocol SSoT; Rust cannot import it).
const FRAME_PTY_OUTPUT: &str = "pty_output";

/// RFC 6455 1013 "Try Again Later" — a standard code, deliberately NOT one of the 4001/4002/4003
/// app codes. `services/bridge.js` has no special case for it, so it lands in the generic close
/// branch: state `closed`, then reconnect with backoff. That reconnect re-runs pairing-free auth
/// and a full `init` + scrollback replay, which is exactly the recovery this case needs.
const CLOSE_TOO_FAR_BEHIND: u16 = 1013;

/// A companion's pending outbound frames, plus the two flags that make the coalescing policy a
/// property of the queue rather than of whoever happens to push into it.
struct Outbox {
    queue: VecDeque<Message>,
    /// Payload bytes currently queued — the budget is measured in bytes because bytes are what
    /// actually exhausts the machine; a frame count would let 500 firehose chunks look identical to
    /// 500 keystroke echoes.
    bytes: usize,
    /// Set when a coalesce dropped something. Consumed by the writer once the queue is empty.
    resync_pending: bool,
    /// The connection is being torn down (a Close frame is queued). Further pushes are ignored so a
    /// late broadcast cannot append after the Close.
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

    /// Last resort: the budget is blown by frames that may NOT be dropped (a phone so wedged that
    /// even state deltas have piled up past the cap). Rather than grow — the original bug — or drop
    /// something undroppable, cut the connection and let the companion's own reconnect+`init` do
    /// the recovery. Bounded memory is not negotiable; this connection is.
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

    /// Hands the whole backlog to the socket writer in one go, and answers "should a re-hydrate be
    /// requested now?" — true only when a coalesce happened AND the queue is now empty, i.e. the
    /// phone has actually caught up and can absorb a snapshot.
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

/// THE ONE PLACE THE RELAY LOOKS INSIDE A FRAME (see the module header's invariant note).
///
/// COALESCIBLE — may be collapsed, because the host can regenerate the full truth on demand:
///   * `pty_output`. Terminal bytes, live chunks and `reset` replays alike. Their entire content is
///     the host's scrollback ring buffer, and `pty_get_scrollback` returns that buffer verbatim
///     along with the authoritative size and liveness. Dropping a run of them and replaying the
///     buffer yields a screen that is *behind*, never a screen that is *wrong*.
///
/// NEVER DROPPED — everything else, including every frame kind not listed here:
///   * `init` / `delta` — mirrored app state. A delta is the only record that a value changed;
///     collapsing state onto a re-hydrate would work only while the re-hydrate is guaranteed, and
///     making correctness depend on a second thing succeeding is how a "safe" drop becomes a phone
///     quietly showing last minute's project list.
///   * dialogs — these ride inside `delta` (`dialogStore.pendingDialog`), and a confirmation the
///     user must answer before an irreversible sync is the single worst frame in the protocol to
///     lose. Called out separately because "it's just a delta" is exactly how it would get dropped.
///   * `invoke_result` — the reply to one specific companion RPC id. Not re-derivable by anything:
///     no snapshot contains it, and dropping it turns a working call into a 20s timeout error.
///   * `pty_exit`, `pty_resize` — liveness and size edges. Tiny, rare, never a cause of backlog, and
///     a lost liveness edge is the 1.20.0 bug (§2.4) where one screen killed the other's live shell.
///   * `ping`/`pong` — dropping a pong makes a healthy phone declare its own socket dead.
///   * anything unrecognized, malformed, or binary — DEFAULT-DENY. A relay that has to guess what a
///     frame means must guess "important"; a future frame type is then safe by construction rather
///     than by someone remembering to come back here.
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

/// Queue one frame for a companion. NEVER blocks and never awaits: the caller is the relay's shared
/// host loop, and anything that can make it wait on a phone is the bug this whole section exists to
/// remove.
fn enqueue(outbox: &CompanionOutbox, msg: Message) {
    let (lock, notify) = &**outbox;
    lock.lock().unwrap().push_within_budget(msg);
    // `notify_one` stores a permit when nobody is waiting, so a notification sent while the
    // companion task is busy awaiting `socket.send` is not lost — its next `notified()` returns
    // immediately. That is what makes creating a fresh `notified()` future each loop iteration safe.
    notify.notify_one();
}

// ── Shared state (process-global, mirrors the OnceLock<Mutex<..>> pattern already used by
// `system::PROJECT_ICONS` — copied deliberately for consistency, not reinvented) ─────────────

struct CompanionHandle {
    /// The paired device's *persistent* id (from `companion-devices.json`), not the ephemeral
    /// per-connection counter used as this map's key — lets `revoke_device` find and close every
    /// live connection belonging to one device without touching any other entity (CLAUDE.md
    /// multi-entity scoped-clear rule).
    device_id: String,
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
    /// Process-local secret minted fresh at every start, handed ONLY to the Tauri webview (via
    /// `get_companion_status`) and required on the `role=host` websocket.
    ///
    /// Why a secret and not just the peer address: `set_tailscale_https` runs
    /// `tailscale serve --bg http://127.0.0.1:PORT`, so with HTTPS-over-Tailscale on, EVERY tailnet
    /// peer's connection arrives at this process from `127.0.0.1`. A loopback test alone therefore
    /// let any peer on the tailnet claim `role=host` with no token at all — overwriting `host_tx`,
    /// cutting the real Mac's mirror off and feeding forged `init`/`delta` frames to every paired
    /// phone. A proxy can forge the source address; it cannot forge a value it was never given.
    /// Never persisted: a new process means a new secret, so a leaked one dies with the app.
    host_token: String,
    /// Gate on top of per-device tokens (defence in depth, not the only gate — the plan's R-1
    /// resolution is "bound 0.0.0.0, but useless without a token"). A fresh install starts `false`
    /// and does not expose anything until the user opens the pairing UI, which calls
    /// `start_companion_server()`; after that the user's own last choice is restored at startup
    /// from `companion-server.json` (see `init`).
    enabled: AtomicBool,
    /// Consecutive wrong codes submitted to `/pair` since the last successful pairing or
    /// `start_companion_server()`. A 6-digit code is only ~1M combinations — without this an
    /// unattended LAN attacker walks the whole space in minutes. At `MAX_PAIR_FAILURES` the relay
    /// disables itself, so brute-forcing costs the attacker the server, not the user the Mac.
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

    fn broadcast_to_companions(&self, msg: Message) {
        let companions = self.companions.lock().unwrap();
        for handle in companions.values() {
            enqueue(&handle.outbox, msg.clone());
        }
    }

    /// The host's own queue stays UNBOUNDED, deliberately. Everything travelling this way is either
    /// human-paced (a companion's keystrokes and intents) or relay-originated (`companion-connected`),
    /// and the consumer is the Mac's own webview over loopback — there is no unbounded producer and
    /// no slow link on this side. More to the point, the coalescing policy has nothing to offer here:
    /// not one frame going to the host is re-derivable, so a bound could only drop something that
    /// matters. Bounding it would be a guard for its own sake.
    fn forward_to_host(&self, msg: Message) {
        let host_tx = self.host_tx.lock().unwrap();
        if let Some(tx) = host_tx.as_ref() {
            let _ = tx.send(msg);
        }
    }

    fn notify_host_companion_connected(&self, device_id: &str) {
        let payload = serde_json::json!({ "t": "companion-connected", "id": device_id }).to_string();
        self.forward_to_host(Message::Text(payload));
    }

    /// Synchronous disk write — callers on the async runtime (the `/pair` handler) MUST run this
    /// through `spawn_blocking`; callers already inside a Tauri-command `spawn_blocking` closure
    /// (`revoke_device`) may call it directly.
    fn persist_devices(&self) -> Result<(), String> {
        let path = self
            .devices_path
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "companion-devices.json path not initialized".to_string())?;
        let devices = self.devices.lock().unwrap().clone();
        let content = serde_json::to_string_pretty(&devices).map_err(|e| e.to_string())?;
        std::fs::write(&path, content).map_err(|e| e.to_string())
    }

    /// Records the user's LAST EXPLICIT on/off choice so a restart resumes it (see `init` for the
    /// reasoning). Same blocking-write discipline as `persist_devices`: callers on the async
    /// runtime must route it through `spawn_blocking`. Best-effort by design — failing to write
    /// this preference must never fail the toggle the user just asked for.
    fn persist_enabled(&self, enabled: bool) {
        let Some(path) = self.server_state_path.lock().unwrap().clone() else {
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
/// visibly-weaker-than-normal token in that one-in-a-million case beats crashing pairing
/// entirely. Not wrapped in `spawn_blocking`: reads a few bytes from the OS's CSPRNG, not a
/// filesystem or network call.
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

/// Loads any already-paired devices from disk, resolves where to persist future ones, then
/// spawns the axum server on Tauri's own tokio runtime. Never blocks `setup()`: the disk read
/// here is a single small JSON file (same class as `logger::init`'s own synchronous read —
/// CLAUDE.md's "plain, fast, synchronous local file I/O ... does not need spawn_blocking"); the
/// actual `TcpListener::bind` + accept loop happens inside the spawned task, not on this thread.
pub fn init(app_handle: &AppHandle) {
    let state = relay();
    match crate::projects::get_app_data_dir(app_handle) {
        Ok(dir) => {
            let path = dir.join("companion-devices.json");
            if let Ok(content) = std::fs::read_to_string(&path) {
                match serde_json::from_str::<Vec<PairedDevice>>(&content) {
                    Ok(devices) => *state.devices.lock().unwrap() = devices,
                    Err(e) => eprintln!("[web_server] companion-devices.json is corrupt, starting empty: {}", e),
                }
            }
            *state.devices_path.lock().unwrap() = Some(path);

            // Restore the user's last explicit on/off choice.
            //
            // WHY restore "on" rather than always booting off: the person this feature exists for
            // is in another room holding the phone. An app restart they did not ask for (an update,
            // a crash, a reboot) must not turn into "walk back to the Mac and click a toggle" — that
            // is the single most annoying failure this feature can produce, and it is the same
            // failure the token fix below is about. "Off means off" is not weakened, because Off is
            // persisted too: what is restored is always the user's own last decision, never a
            // default the app chose for them. A fresh install still starts off (the file is absent →
            // `Default` → false).
            //
            // The pairing code is deliberately NOT persisted — a fresh one is minted on restore, so
            // a code read off the Mac's screen last week is dead, and the 10-strike brute-force
            // shutdown (MAX_PAIR_FAILURES) still guards the new one while nobody is watching.
            let server_state_path = dir.join("companion-server.json");
            let restored = std::fs::read_to_string(&server_state_path)
                .ok()
                .and_then(|c| serde_json::from_str::<PersistedServerState>(&c).ok())
                .unwrap_or_default();
            *state.server_state_path.lock().unwrap() = Some(server_state_path);
            if restored.enabled {
                *state.pairing_code.lock().unwrap() = generate_pairing_code();
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

/// Binds once and serves for the lifetime of the process. The listener is intentionally never
/// rebound by `start_companion_server`/`stop_companion_server` (see those commands' doc
/// comments) — this avoids a bind-race/"address already in use" class of bug that cannot be
/// exercised on this dev box before it ships.
///
/// Binds on `0.0.0.0` (IPv4 unspecified) — deliberately IPv4-only. A dual-stack `[::]` bind made
/// every inbound IPv4 connection arrive as an IPv4-mapped address (`::ffff:a.b.c.d`), which broke
/// the host's loopback guard (`Ipv6Addr::is_loopback()` matches only `::1`) and left companions
/// with no host to mirror from. IPv4-only removes that whole class of mapped-address bug. The LAN
/// case is IPv4; Tailscale is still reachable over its `100.x` IPv4 (the `tailscale` menu row) —
/// only a pure-IPv6 Tailscale peer is not served, which this app does not target.
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
//
// Release: `/` and every other GET is answered from the EMBEDDED bundle via Tauri's asset
// resolver (`resolve_dist_dir()` + `ServeDir` on disk is deleted — Tauri v2 compiles
// `frontendDist` into the binary and ships no loose `dist/` in the `.app`, so the old approach
// 404'd every time; that exact 404 is what this rewrite fixes).
// Dev: everything but /ws and /pair is reverse-proxied to the Vite dev server, which stays
// localhost-only — the phone only ever reaches Vite *through* this proxy. HMR is NOT proxied
// (§7.2): the Mac window hot-reloads directly against Vite; the phone uses manual refresh.
fn build_router(app: &AppHandle) -> Router {
    let router = Router::new()
        .route("/ws", get(ws_handler))
        .route("/pair", post(pair_handler))
        // Permissive CORS: in `tauri dev` the companion loads the SPA from Vite (port
        // 142x) and calls this server (1421) cross-origin for `POST /pair` (plan §7.1a "Dev vs
        // prod" table). The endpoint's own auth (the 6-digit code / device token) is the real
        // security boundary, not CORS — this is an internal LAN tool, not a public API.
        .layer(tower_http::cors::CorsLayer::permissive());

    if cfg!(debug_assertions) {
        let vite_origin = resolve_vite_origin(app);
        eprintln!("[web_server] dev mode: proxying non-relay requests to vite on {}", vite_origin);

        // HMR is NOT proxied here (§7.2): the Mac dev window hot-reloads directly against Vite;
        // the phone loads the SPA through this proxy but has no live HMR in dev (manual refresh).
        // An axum HMR websocket bridge was tried and removed — it rerouted even the Mac window's
        // HMR through the proxy and broke it with connection-refused spam. Reliable-first.
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

/// The exact origin the Tauri dev window itself loads — `scripts/tauri-runner.js` overrides
/// `build.devUrl` at runtime with the free port it picked, so this is the one authoritative answer
/// for both host AND port.
///
/// Deliberately keeps the URL's **host** instead of hardcoding `127.0.0.1`: Vite binds the name
/// `localhost`, which on macOS resolves to **`::1` first**, so a v4-literal dial gets connection-
/// refused while the very same server answers fine on `http://localhost`. That mismatch is exactly
/// the "vite dev server unreachable on 127.0.0.1:1420" failure this replaced — the window worked
/// (it uses the name) while the proxy did not (it used the literal).
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
    // Fallback uses the NAME too, for the same dual-stack reason.
    format!("http://localhost:{}", port)
}

/// "Off means off": while the `enabled` gate is down, the HTTP surface serves NOTHING to the LAN —
/// not the release SPA, not (in dev) a proxied view of the whole Vite dev server including its
/// `/@fs/` source endpoint. Only `/ws` and `/pair` were gated before this, which left the page
/// itself readable by anyone on the network even with remote control switched off.
///
/// Safe for the Mac window in BOTH modes because the window never loads its assets through axum:
/// in dev it loads Vite directly on localhost, in release the embedded `tauri://` protocol.
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

/// Headers that must never be blindly forwarded across the proxy boundary: the hop-by-hop set
/// (RFC 7230 §6.1), plus two connection-scoped ones —
/// * `host`: reqwest (outbound) and the browser (inbound) set it from the real target;
/// * `content-length`: both hyper and reqwest derive it from the body we actually hand them, so
///   copying the original value risks a mismatch that trips a protocol error instead of a page.
fn is_hop_by_hop_header(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        "connection" | "keep-alive" | "transfer-encoding" | "upgrade" | "host" | "content-length"
    ) || name.starts_with("proxy-")
}

/// Dev fallback: reverse-proxies method + path + query + headers (minus hop-by-hop) + body to the
/// Vite dev server at `vite_origin` (see `resolve_vite_origin`) and streams back its
/// status/headers/body. `502` (never a panic) if Vite is unreachable.
async fn dev_proxy_handler(vite_origin: String, req: Request) -> Response {
    if let Some(resp) = reject_if_disabled() {
        return resp;
    }
    // HMR (and any other) websocket upgrade is NOT proxied (§7.2). Answering it with Vite's
    // index.html would leave the browser's handshake hanging on a 200; a plain 501 makes the
    // Vite client fail fast and fall back instead of retrying against a lying endpoint.
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

/// Release fallback: serves the SPA straight out of the binary via Tauri's asset resolver — no
/// disk `dist/` involved at all. SPA-fallback: an unknown/missing path falls back to
/// `index.html` so client-side routing still boots.
///
/// VERIFY ON MAC (uncertain, the single most load-bearing unknown in this lane — could not
/// compile here): `app.asset_resolver()` — assumed `tauri::Manager::asset_resolver(&self) ->
/// tauri::AssetResolver`, with `.get(path: String) -> Option<tauri::async_runtime::Asset>`
/// (some tauri 2.x point releases have shuffled this between `tauri::` and `tauri::utils::`) and
/// the returned type exposing `.bytes: Vec<u8>` + `.mime_type: String`. If the method or field
/// names differ in the pinned `tauri` crate version, this function is the only place to fix.
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

async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(q): Query<WsQuery>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, addr, q))
}

async fn handle_socket(socket: WebSocket, addr: SocketAddr, q: WsQuery) {
    match q.role.as_deref() {
        Some("host") => {
            // Authority for `role=host` is PROVEN, not inferred from the peer address.
            //
            // The loopback test stays as the outer, cheap check, but it is no longer sufficient on
            // its own: `tailscale serve` proxies the tailnet into `http://127.0.0.1:PORT`, so every
            // tailnet peer's connection also arrives from loopback (see `RelayState::host_token`).
            // The process-local secret is the real gate — the Mac's webview is the only party ever
            // handed it, and it is minted per process so it cannot be replayed after a restart.
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
            // Checked BEFORE the token, and answered with a code that says nothing about the token:
            // a disabled relay must not double as an oracle for whether a token is still valid, and
            // the companion must not read "the Mac is off right now" as "you have been revoked".
            if !state.enabled.load(Ordering::SeqCst) {
                close_with_code(socket, CLOSE_SERVER_DISABLED, "remote control is disabled on the host").await;
                return;
            }
            let token = q.token.clone().unwrap_or_default();
            let device_id = {
                let devices = state.devices.lock().unwrap();
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
    *state.host_tx.lock().unwrap() = Some(tx);

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let Some(result) = incoming else { break };
                match result {
                    Ok(msg @ (Message::Text(_) | Message::Binary(_))) => {
                        state.broadcast_to_companions(msg);
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

    // Only one host connection exists in practice (single Mac app instance, single_instance
    // plugin already enforces that) so an unconditional clear is correct for the real case;
    // a rapid reconnect racing this cleanup is a known, accepted edge case, not silently
    // "fixed" with false confidence.
    *state.host_tx.lock().unwrap() = None;
}

/// Owns one companion's socket, and is the ONLY consumer of that companion's outbox — which is what
/// makes the queue's order the wire's order without any locking discipline asked of the producers.
async fn handle_companion_socket(mut socket: WebSocket, conn_id: u64, device_id: String) {
    let state = relay();
    let outbox: CompanionOutbox = Arc::new((StdMutex::new(Outbox::new()), Notify::new()));
    state
        .companions
        .lock()
        .unwrap()
        .insert(conn_id, CompanionHandle { device_id: device_id.clone(), outbox: Arc::clone(&outbox) });
    state.notify_host_companion_connected(&device_id);

    let (lock, notify) = &*outbox;
    'conn: loop {
        tokio::select! {
            incoming = socket.recv() => {
                let Some(result) = incoming else { break };
                match result {
                    Ok(msg @ (Message::Text(_) | Message::Binary(_))) => {
                        state.forward_to_host(msg);
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            _ = notify.notified() => {}
        }

        // Drained after EITHER branch, so nothing queued during an inbound frame waits for the next
        // notification. The lock is released before the first `await` — a producer must never find
        // this mutex held across socket I/O.
        let (batch, wants_resync) = lock.lock().unwrap().take();
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
            // The queue is empty and the socket accepted everything in it, so this phone can absorb
            // a snapshot now. The host answers this exactly as it answers a fresh join.
            state.notify_host_companion_connected(&device_id);
        }
    }

    state.companions.lock().unwrap().remove(&conn_id);
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

    let expected = state.pairing_code.lock().unwrap().clone();
    if expected.is_empty() || body.code.trim() != expected {
        let failures = state.pair_failures.fetch_add(1, Ordering::SeqCst) + 1;
        if failures >= MAX_PAIR_FAILURES {
            // Shut the whole relay down rather than let a 6-digit space be walked. The user turns
            // it back on from the header menu, which mints a fresh code — so the attacker's
            // progress through the old code space is worthless too.
            state.enabled.store(false, Ordering::SeqCst);
            state.pairing_code.lock().unwrap().clear();
            // Persisted, so a brute-force attempt is not undone by the attacker simply waiting for
            // the app to restart. Blocking write on the async runtime → spawn_blocking, per the
            // never-block-the-UI rule that governs every other disk write in this file.
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

    // Best-effort friendly label from User-Agent — not itemized in §13.1's request body, so no
    // rename/label endpoint is implemented in this Wave-1 pass; a future round can let the user
    // rename a device from the paired-devices modal without changing this file's shape.
    let label = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().chars().take(80).collect::<String>())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Companion device".to_string());

    let device = PairedDevice { id: generate_id(), token: generate_token(), label, paired_at: now_secs() };
    let token = device.token.clone();
    state.devices.lock().unwrap().push(device);

    // The disk write is blocking IO; this handler runs on the same tokio runtime as the rest of
    // the app (spawned via `tauri::async_runtime::spawn` in `init`), so it goes through
    // `spawn_blocking` exactly like every Tauri command's blocking work does.
    let write_result = tauri::async_runtime::spawn_blocking(|| relay().persist_devices())
        .await
        .map_err(|e| format!("spawn_blocking panicked: {}", e))
        .and_then(|r| r);

    if let Err(e) = write_result {
        // Roll back: a token that only works this session and silently stops working (and can
        // never be revoked, since it was never written) after a restart is worse than failing
        // the pairing attempt outright.
        state.devices.lock().unwrap().retain(|d| d.token != token);
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

/// Tailscale's CGNAT range, 100.64.0.0/10.
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

/// (Re)generates the pairing code and marks the relay accepting-companions. Idempotent to call
/// again while already running (e.g. reopening the pairing modal after closing it) — always
/// hands back a *fresh* code so a stale code shown earlier in the session can't be reused.
///
/// Does NOT rebind the TCP listener — that is bound once for the process's lifetime in
/// `init()`/`serve_forever` (see that function's doc comment for why). This command only flips
/// the `enabled` gate that the WS handler and `/pair` both check, and mints a new code.
#[tauri::command]
pub async fn start_companion_server() -> Result<CompanionServerInfo, String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<CompanionServerInfo, String> {
        let state = relay();
        let code = generate_pairing_code();
        *state.pairing_code.lock().unwrap() = code.clone();
        state.pair_failures.store(0, Ordering::SeqCst);
        state.set_enabled(true);
        Ok(CompanionServerInfo { pairing_code: code, port: PORT })
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Flips the `enabled` gate off (new pairing attempts get 401/4001) and immediately closes every
/// currently-connected companion socket — "stop" cuts live mirrors, not just future joins. The
/// listener itself keeps running so a later `start_companion_server()` doesn't need to re-bind.
#[tauri::command]
pub async fn stop_companion_server() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<(), String> {
        let state = relay();
        state.set_enabled(false);
        let mut companions = state.companions.lock().unwrap();
        for (_, handle) in companions.drain() {
            // CLOSE_SERVER_DISABLED, never CLOSE_UNPAIRED: turning the toggle off revokes nothing.
            // Sending 4001 here is exactly what made every paired phone forget its token on an Off.
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

/// The relay's live truth, for the host UI to re-sync against.
///
/// Needed because `enabled`/`pairing_code` live in the Rust process, which OUTLIVES the webview:
/// an HMR full-reload in dev, or any webview reload, resets the frontend's `running` ref to false
/// while the relay is still on and paired phones are still connected — the menu would then claim
/// "Off" while the LAN is actually being served. The frontend calls this once on mount.
#[derive(Serialize)]
pub struct CompanionStatus {
    enabled: bool,
    pairing_code: String,
    port: u16,
    /// The process-local secret the `role=host` websocket requires (`RelayState::host_token`).
    ///
    /// Carried on this EXISTING command rather than a new one on purpose: registering a command
    /// means editing `lib.rs`, which belongs to another workstream in this release, and the
    /// audience is identical either way — a Tauri command is reachable only from the Tauri webview
    /// or from an already-paired companion over the `invoke` RPC, and a paired companion is
    /// already granted arbitrary `invoke` by the declared security posture. What this closes is the
    /// *unpaired* tailnet peer, which was never covered by that posture.
    #[serde(rename = "hostToken")]
    host_token: String,
}

#[tauri::command]
pub async fn get_companion_status() -> Result<CompanionStatus, String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<CompanionStatus, String> {
        let state = relay();
        Ok(CompanionStatus {
            enabled: state.enabled.load(Ordering::SeqCst),
            pairing_code: state.pairing_code.lock().unwrap().clone(),
            port: PORT,
            host_token: state.host_token.clone(),
        })
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// §7.1a: every reachable address, classified. Pure interface enumeration via `if-addrs` — no
/// subprocess, no network call — still routed through `spawn_blocking` for consistency with
/// every other command in this file (and because interface enumeration is, technically, a
/// blocking syscall, however fast in practice).
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
                // The listener is IPv4-only (see serve_forever), so an IPv6 address is never
                // reachable — do not offer it as a companion URL. Tailscale still appears via its
                // 100.x IPv4 above.
                if_addrs::IfAddr::V6(_) => {}
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

// ── Tailscale HTTPS (`tailscale serve`) ──────────────────────────────────────────────────────
// Exposes the companion over HTTPS on the tailnet, which is what unlocks installing it as a
// STANDALONE PWA: Android Chrome only offers a standalone install from a secure context, and plain
// http on a LAN / 100.x IP is not one. Integrated as an in-app toggle so the user never touches the
// CLI. The ONE thing the app cannot do is enable HTTPS certs for the tailnet — that is an
// admin-console account setting; when it's off, tailscale's own error (which carries the admin URL)
// is passed straight through to the UI. All async + spawn_blocking per the never-block-UI rule.

/// Resolve the tailscale CLI. Prefer an explicit install path (no dependency on the GUI app's PATH
/// or rc-sourcing timing — see CLAUDE.md's cold-start PATH race), fall back to the bare name (found
/// via create_command's Homebrew/local PATH prepend on macOS, or the system PATH on the Linux dev box).
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

/// This node's MagicDNS name → its https URL (trailing dot stripped), or None if unreadable.
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

/// True when `tailscale serve` is currently proxying our loopback port.
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

/// Read whether HTTPS-over-Tailscale is available/enabled plus this node's https URL. The host UI
/// calls this on mount/start to render the toggle and the URL row. A tailscale that is missing or
/// erroring is reported as a clean `available:false` (an off state), never a hard error.
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

/// Turn the HTTPS proxy on (`tailscale serve --bg http://127.0.0.1:PORT`) or off
/// (`tailscale serve --https=443 off`). On an enable that fails because the tailnet has no HTTPS
/// certs yet, tailscale's stderr (which carries the admin URL to enable them) is returned verbatim
/// so the UI can show the user exactly what to click.
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
        let devices = relay().devices.lock().unwrap();
        Ok(devices.iter().map(PairedDeviceView::from).collect())
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Scoped-clear (CLAUDE.md multi-entity rule): removes exactly the one device matching `id`
/// from `companion-devices.json`, rewriting the rest unchanged, and closes only that device's
/// live socket(s) — every other paired device and every other companion's connection survives
/// untouched.
#[tauri::command]
pub async fn revoke_device(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = relay();
        let removed = {
            let mut devices = state.devices.lock().unwrap();
            let idx = devices.iter().position(|d| d.id == id);
            idx.map(|i| devices.remove(i))
        };
        if removed.is_none() {
            // Unknown/already-revoked id — idempotent no-op, not an error.
            return Ok(());
        }
        state.persist_devices()?;

        let mut companions = state.companions.lock().unwrap();
        let dead: Vec<u64> = companions
            .iter()
            .filter(|(_, h)| h.device_id == id)
            .map(|(k, _)| *k)
            .collect();
        for k in dead {
            if let Some(handle) = companions.remove(&k) {
                // A genuine revocation IS the CLOSE_UNPAIRED case — this is the one place the
                // companion should drop its stored token, and now the only one that says so.
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

/// ICON-1: scans once, holds a COMPLETE map (every project id → data URI or explicit `null`),
/// so the frontend can never 404 or retry-loop on an icon. Reuses `projects::load_projects`
/// (which already repopulates `system::PROJECT_ICONS` as a side effect) and
/// `system::get_project_icons()` — the existing scan/cache primitives — rather than duplicating
/// project-type detection here.
#[tauri::command]
pub async fn get_project_icons_map(app: AppHandle) -> Result<HashMap<String, Option<String>>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let projects = crate::projects::load_projects(app)?;
        let cache = crate::system::get_project_icons().lock().unwrap();
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

/// FILE-1: reads are confined to an allow-list of roots — every known project's `local_path`
/// (which already contains any project's `REPORT.html`, so no separate report-path root is
/// needed). Confinement uses `canonicalize()` + `Path::starts_with` (component-wise, not a raw
/// string prefix — `/home/user/projectX` does not spuriously match a `/home/user/project` root)
/// so `..` traversal and symlink escapes are both closed. A path outside every root, or one that
/// doesn't exist, returns an error — never bytes.
#[tauri::command]
pub async fn read_text_file(app: AppHandle, path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let projects = crate::projects::load_projects(app)?;
        let requested = std::fs::canonicalize(&path).map_err(|e| format!("path not found: {}", e))?;

        let allowed = projects.iter().any(|p| {
            std::fs::canonicalize(&p.local_path)
                .map(|root| requested.starts_with(&root))
                .unwrap_or(false)
        });
        if !allowed {
            return Err("path is outside every project root — refusing to read".to_string());
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
        // 4001 keeps its shipped meaning (token rejected) so already-installed companions that
        // only understand 4001 still behave correctly on a real revocation.
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
    // These drive `Outbox` directly rather than through a socket: the policy is a property of the
    // queue, and a test that needed a live phone to prove it would prove it on nobody's machine.

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

    /// CLAUDE.md's serde rule: an older or truncated `companion-server.json` must degrade to "off",
    /// never fail the read and never fall through to "on".
    #[test]
    fn persisted_server_state_defaults_to_off() {
        assert!(!serde_json::from_str::<PersistedServerState>("{}").unwrap().enabled);
        assert!(serde_json::from_str::<PersistedServerState>(r#"{"enabled":true}"#).unwrap().enabled);
        assert!(!PersistedServerState::default().enabled);
    }
}
