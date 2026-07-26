# Plan — Remote Control (Companion browser drives the Mac app)

> **Status: DONE — shipped in `1.19.0` (2026-07-25), commit `20de7cb`.** This document started as design-only and is kept as the architecture record of what was built: the four seams, SYNC-1/SSOT-1/ENV-1, and the frozen wire protocol in §13 are all live in the code and are still the contract source-of-truth referenced from `src/services/*`, `src/store/*` and `src-tauri/src/web_server.rs`. §3.4 (dialogs as mirrored state) was the one part deferred past 1.19.0 and was built in `1.20.0` — see `docs/plan/done/1.20.0-terminal-and-remote-sync.md` §3.
> Current-state feature doc: `docs/feat/remote-control.md`. Original baseline when this was written: exactly `1.18.0` (working tree restored; the earlier patchwork attempt was parked in `git stash@{0}` — "WIP remote-companion (patchwork)").

---

## 0. Goal, and the failure this design exists to avoid

**Goal.** Open any browser on the LAN, control the Mac app, full two-way signal: what the Mac shows,
the browser shows; what the browser does, the Mac does.

**The failure mode of the parked attempt** (learned, not theorised):

| Symptom in the parked WIP | Root cause |
| :-- | :-- |
| `usage blank on remote`, then per-source registration in `useAgentUsage.js` | mirrored state was **hand-registered per key**; state living inside a *factory* composable had no addressable key |
| "data-query commands are Host-only and skipped on the remote" | forwarding `invoke` **without a reply** forced a per-command skip-list |
| `startSync` as "the one deliberate exception" at intent level | forwarding `invoke` instead of the **gesture** means a mutation performed on the browser's copy of state never reaches the Mac's copy |
| 4 items in a "Deferred" list | each new feature needed its own wiring, so features could be "not wired yet" |

Every one of those is the same root disease: **the browser was treated as a second copy of the app
that has to be selectively disabled.** This plan removes that by construction.

**Non-goal, explicitly.** Moving state into Rust. Vue on the Mac stays the one brain and the one SSOT,
byte-for-byte as in 1.18.0. Rust gains exactly one new job: a **dumb relay**.

---

## 0.5 Why remote at all — the pinned scope decision (read this before proposing anything)

This was almost built as the wrong thing twice; the scope below is a hard boundary, not a preference.

* **The reason this exists is the phone.** The real, immediate need is: pick up a phone, control the
  Mac app quickly. Everything is optimised for *"fast and lightweight to reach from a phone"*, not for
  generality.
* **It is signal-remote, not image-remote.** Think "remote desktop, but we ship the *signals*
  (state + intents) instead of streaming pixels." The browser runs the real Vue UI locally and stays
  in lockstep with the Mac over a thin data channel. That is the whole value: same UI, no video.
* **Multi-user LAN co-use is OUT OF SCOPE and stays out.** If the goal were several people developing
  against the app together in a LAN, the right answer is a **separate app**, not this. Conflating the
  two is exactly what produced the earlier over-engineering. Do not add anything whose only
  justification is "multiple simultaneous operators" — one Mac, N mirror screens of the *same*
  session is the model.

> **Invariant SCOPE-1 — one session, mirrored to companion screens. Not a multi-tenant server.**
> A feature request that only makes sense for "different people doing different things at once"
> belongs in a different app, not here.

---

## 1. Architecture — one role flag, four seams

```
┌──────────────────────── MAC (the one brain, unchanged 1.18.0) ────────────────────────┐
│  Vue components ── call ──► store actions ──► mutate module-level refs (SSOT)         │
│         ▲                        │                        │                            │
│         │ render                 │ invoke()               │ watch                      │
│         └────────────────────────┼────────────────────────┼──────────────┐             │
│                                  ▼                        ▼              │             │
│                          Rust #[tauri::command]      services/mirror     │             │
│                          (fs / ssh / git / rsync)         │              │             │
└───────────────────────────────────────────────────────────┼──────────────┼─────────────┘
                                    ws://127.0.0.1:1421  ◄──┘              │
┌───────────────────────────── RUST RELAY (web_server.rs) ──┼──────────────┼─────────────┐
│  axum: static dist/ + /icon/:host + /ws                   │              │             │
│  holds NO state. host→all companions, companion→host. That is all it does.             │
└───────────────────────────────────────────────────────────┼──────────────┼─────────────┘
                                    ws://<mac-lan-ip>:1421  │              │
┌────────────────────── COMPANION (browser — same bundle, dumb) ───────────┼─────────────┐
│  Vue components ── call ──► store actions ──► FORWARD as intent ─────────┘             │
│         ▲                                                                              │
│         └── render ◄── the same module-level refs ◄── deltas applied by services/mirror │
│  No timers. No polling. No boot sequence. Nothing writes those refs locally.            │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

### The four seams

| # | Seam | File | Host behaviour | Companion behaviour |
| :-- | :-- | :-- | :-- | :-- |
| **T** | Transport + role | `services/bridge.js` | WS client → `127.0.0.1:1421?role=host` | WS client → `<origin>/ws?role=companion` |
| **S** | State mirror (in) | `services/mirror.js` | watch every store ref → broadcast delta | apply delta into the *same* ref |
| **A** | Intent relay (out) | `services/intents.js` | run the real action fn | forward `{key,args}`, run nothing |
| **N** | Native call | `utils/tauri.js` | real `invoke` | RPC over WS → host invokes → reply |
| **P** | Producer gate | `utils/scheduler.js` | timers/boot run | no-op |

**S+A are auto-discovered, not registered.** Adding a feature never touches a seam file. §2 and §3
prove it.

### 1.1 The line between "mirror it" and "keep it local" — the on-input rule

Borrowed verbatim from how Claude's own remote control behaves, and it is the rule that decides every
ambiguous case (this replaces the old "mirror everything vs mirror nothing" fork — neither extreme):

> **Invariant SYNC-1 — mirror *data events*, not *input events*.**
> * **Input events** (a keystroke in a text field, focus, caret position, a purely local
>   modal-open, scroll) are **local to the screen that produced them.** They are never streamed
>   character-by-character. A form is a local draft.
> * **Data events** (a value committed to a store ref, a command result, a **dialog that requires a
>   response**, a sync log line, a toggle that changes real behaviour) are **seen by both sides.**
>   Either side may answer/act on them.

Consequences that fall straight out of SYNC-1:

* Typing the SSH config or a project path on the phone does **not** lag and does **not** fight a
  second typist — it is a local draft until **Save**, and Save is one intent carrying the whole
  payload (a data event).
* A confirm-delete dialog is a **data event** → it appears on **both** the phone and the Mac, and
  **whichever screen answers it wins.** This is the direct fix for the parked WIP's dead end
  ("triggered from the phone, but you had to walk to the Mac to confirm"). Design in §3.4.
* This keeps latency where it belongs (never on keystrokes) while making every *decision* reachable
  from the phone — which was the actual objective.

---

## 2. Seam S — mirroring singleton stores with zero per-key wiring

### 2.1 The rule that makes discovery possible

> **Invariant SSOT-1 — every piece of shared application state is a module-level `ref` exported from
> `src/store/*.js`. Nothing else is shared state.**

1.18.0 already obeys this for 7 stores. It does *not* for state that lives in composables/components
(`useGit`'s modal refs, `useAgentUsage`'s per-source refs, `AgentUsageSection.vue`'s source toggles).
Making them obey is a mechanical move, listed in §8 — **this move is the entire cost of the feature.**

### 2.2 Discovery

```js
// services/mirror.js
const mods = import.meta.glob('../store/*.js', { eager: true })   // build-time, no runtime cost
// → { '../store/projectStore.js': { projects: Ref, projectRuntime: Ref, beginRefresh: fn, ... } }

for (const [path, mod] of Object.entries(mods))
  for (const [name, val] of Object.entries(mod))
    if (isRef(val)) STATE.set(`${basename(path)}.${name}`, val)   // 'projectStore.projects'
```

Both sides run this identical code and get **identical keys pointing at their own local refs.**
That symmetry is the whole trick: a delta from the Mac lands directly in the browser's real ref, and
Vue re-renders. No parallel data model, no adapter, no per-screen wiring.

### 2.3 Broadcast (host) / apply (companion)

```
HOST                                          COMPANION
ref changes (any cause: user, poll,           ws message {t:'delta', v:{key:value}}
subprocess result, timer)                             │
    │ watch(ref, deep:true)                           ▼
    ▼                                         applying = true
queue key in a dirty Set                      STATE.get(key).value = decode(value)
    │ flush on next microtask (coalesce)      applying = false
    ▼                                                 │
send {t:'delta', v:{k1:…, k2:…}}                      ▼ Vue re-renders
```

* **Echo guard:** the companion never registers broadcast watchers at all (role check happens once,
  inside `mirror.js`). `applying` additionally guards against any watcher a component may own.
* **Join:** on `companion-connected` the relay tells the host, which sends `{t:'init', v:<all keys>}`.
  One code path — `init` is just a delta containing every key.
* **Serialization contract (SER-1):** mirrored values must be JSON-safe. The encoder handles
  `Set`/`Map` (tagged objects — `useAgentUsage.activeEmails` is a `Set`). A value that fails to
  encode (DOM node — `logStore.consoleRef`; function — `logStore.globalListener`) is dropped from
  the mirror **automatically, once, with a console warning**. No hand-maintained exclusion list.
* **Delta size, v1:** whole value per changed key, coalesced per microtask. This is fine at this
  app's scale (a handful of projects). `logStore.projectLogs` is the only ref that grows without
  bound during a sync — see risk R-3 in §10.

---

## 3. Seam A — an intent is a *gesture*, not an `invoke`

### 3.1 Why forwarding `invoke` is wrong (this is the 1.18.0-specific proof)

`ProjectTable.vue:249` today:

```html
<input type="checkbox" v-model="p.dry_run" @change="saveProjectsList()" />
```

If the browser forwarded only the `invoke`:
`v-model` mutates the **browser's** copy of `projects` → `save_projects` runs on the Mac with the
browser's payload → the Mac's `projects` ref is never touched → the Mac UI still shows the old
checkbox → the next delta from the Mac **reverts the browser**. Two brains, one wins at random.

So the browser must forward **the gesture**, and the Mac must perform it in its own reactive context.

> **Invariant ACT-1 — any function that mutates mirrored state is an exported action of a store
> module, wrapped in `action()`. Components never write mirrored state directly (no `v-model` onto
> it).**

### 3.2 Declaration and discovery

```js
// store/projectStore.js
export const setProjectFlag = action((id, key, value) => {
  const p = projects.value.find(p => p.id === id); if (!p) return
  p[key] = value
  return saveProjectsList()
})
```

`action()` is a one-word declaration at the definition site (like `export` itself) — **not**
per-feature plumbing: it names nothing, registers nothing, and no seam file lists it.

* **Host:** `action(fn)` returns `fn` unchanged. Zero overhead, zero behaviour change vs 1.18.0.
* **Companion:** returns a stub. On first call it finds its own key by *identity* in the same
  `import.meta.glob` map (`fn === mod[name]` → `'projectStore.setProjectFlag'`), then sends
  `{t:'intent', key, args}`. A wrapped fn not reachable from a globbed module throws loudly in dev —
  never silently no-ops.

### 3.3 Round trip

```
[browser] click DRY
   └─ setProjectFlag('p3','dry_run',true)     ← the same call the Mac's UI makes
        └─ stub → ws {t:'intent', key:'projectStore.setProjectFlag', args:['p3','dry_run',true]}
             └─ RELAY (no logic) → host
                  └─ STATE-glob lookup → real setProjectFlag('p3','dry_run',true)
                       ├─ mutates the Mac's projects ref     → Mac UI updates
                       └─ saveProjectsList() → invoke('save_projects') → disk
                            └─ watch fires → {t:'delta', v:{'projectStore.projects':[…]}}
                                 └─ RELAY → every companion → ref assigned → browser UI updates
```

Latency is one LAN round-trip; the browser shows the change when the **Mac** confirms it — no
optimistic local write, therefore no divergence, ever.

**Args contract (ARG-1):** actions take JSON-safe scalars (`id`, `direction`, `value`) — never a live
object. 1.18.0's `startSync(project, …)` takes the project object and mutates it in place; it becomes
`startSync(projectId, …)` and resolves the object from `projects.value` (§8 R-2). The companion's
encoder throws on a non-serializable arg — loud, at dev time.

---

## 4. Seam N — native calls, no skip-list, no classification

```js
// utils/tauri.js — the ONLY module allowed to import @tauri-apps/api/core
export const invoke = isHost ? tauriInvoke
                             : (cmd, args) => bridge.request({ t:'invoke', cmd, args })
```

The companion's `invoke` is a **full RPC with a reply**: the host runs the real command and returns
the value or the error. Consequences:

* `invoke()` has the **same signature and the same semantics on both sides** → nothing in the app
  needs to know where it runs → no skip-list, no "host-only command" concept, no deferred feature.
* A read-only query fired by a *user gesture* (open GitModal → `get_git_info`) just works on the
  browser and feeds that browser's local view state.
* A command that both invokes **and** mutates mirrored state must be an action (ACT-1) — otherwise
  the mutation would be applied on the wrong side. §8 R-3 audits the 14 component call sites.
* The **host half** answering these frames is `services/hostInvoke.js` (booted host-only in
  `initRemote`): it runs the real IPC and replies `{t:'invoke_result', id, ok}` / `{id, err}`. Without
  it a companion `invoke()` hangs — the original "seam N not wired" bug.
* A 20 s watchdog in `bridge.request()` bounds the RPC: since every genuinely long op (`run_sync`,
  delete-preview) is an **action/intent** rather than a companion invoke, an invoke that goes 20 s
  unanswered means the host is not replying at all, not that it is "slow" — so failing fast with a
  named error beats hanging forever. Liveness is also WS ping/pong; a dropped socket rejects all
  in-flight RPCs at once.

Note `run_sync` is never RPC'd from a browser anyway: it is called *inside* `startSync`, which is an
action and therefore runs entirely on the Mac.

### 3.4 Dialogs as mirrored state — a confirm is answerable from either screen

Per SYNC-1 a confirm dialog is a **data event**, so it cannot be a `Swal.fire()` awaited inside the
action's call stack on the Mac (that stack only exists on the Mac). The dialog becomes **state**:

```js
// store/dialogStore.js
export const pendingDialog = ref(null)   // { id, kind:'confirm-delete', title, files, needsTyped }
export const resolveDialog = action((id, answer) => {          // answer: {confirmed, typed}
  const d = pendingDialog.value
  if (!d || d.id !== id) return          // already answered on the other screen — ignore
  pendingDialog.value = null
  _waiters.get(id)?.(answer)             // unblocks the awaiting action on the Mac
})
```

Flow, using only the two seams already defined — **no third mechanism:**

```
[phone] startSync(id,'push')  ──intent──► [Mac] runs startSync
   │                                          └─ needs delete-confirm:
   │                                               pendingDialog.value = {id, files, …}   (a data event)
   │                                               await promise keyed by id
   ▼                                                     │ watch → delta
a <DialogHost> component renders pendingDialog  ◄────────┴──────────► same on the Mac
   │  (both screens now show the identical modal)
   └─ user types the project name on the PHONE, clicks confirm
        └─ resolveDialog(id,{confirmed:true,typed}) ──intent──► [Mac] resolves the promise
             └─ startSync continues on the Mac, log lines mirror back to the phone
```

* The 6 `Swal.fire` sites in `useSync.js` become one reusable `<DialogHost>` that renders
  `pendingDialog`. That is the "most work" cost flagged in Q2 — but it buys the core requirement
  (act from the phone end-to-end) with **zero new transport**.
* First-answer-wins is handled by the `id` guard above; the losing screen's click is a no-op.
* The typed-confirmation value travels *with the answer* (`typed`), validated on the Mac — the phone
  never gets to skip the safety check.

---

## 5. Seam P — no second brain (the part the parked WIP got structurally wrong)

The browser must not *produce*. Producers in 1.18.0 are enumerable (5 sites — §8 R-4):

| Producer | Location |
| :-- | :-- |
| usage poll timer | `useAgentUsage.js:612` |
| webview-occlusion watchdog | `useAgentUsage.js:252` |
| background git / remote-diff timers | `useBackgroundRefresh.js:47,64` |
| boot sequence (`loadData`, `initGlobalNote`, `refreshClaudeMode`, `cleanup_legacy_baselines`) | `App.vue:42` |
| sync-output event listener (`listen()`) | `useLogs.js` / `logStore.globalListener` |

```js
// utils/scheduler.js
export const hostInterval = isHost ? (fn, ms) => setInterval(fn, ms) : () => null
export const onHostBoot   = isHost ? (fn) => fn()                    : () => {}
```

Gating these is what makes "the browser is dumb" **structural** rather than a habit:
the browser cannot poll Anthropic's API, cannot double-fetch, cannot trip a rate limit, cannot start
a second `listen()` on an event stream that does not exist for it. Sync output reaches the browser as
`logStore.projectLogs` deltas — the mirror, not a listener.

Cosmetic timers (`UsageCircle.vue`, `AgentUsage.vue` animation ticks) are **not** producers and stay
untouched.

---

## 6. Proof: the 5 required cases need zero per-feature remote code

Each row shows the only change (a one-time §8 refactor), then the automatic two-way behaviour.

| # | Case | Refactor needed (one-time) | Browser → Mac | Mac → Browser |
| :-- | :-- | :-- | :-- | :-- |
| 1 | **DRY toggle** | `ProjectTable.vue:249` `v-model` → `:checked` + `setProjectFlag()` action (ACT-1) | intent `projectStore.setProjectFlag` runs on Mac, mutates `projects`, saves | `projects` delta → checkbox flips in the browser |
| 2 | **Reload project** | `loadData` / `refreshProject` move to a store + `action()` | intent runs on Mac → `invoke('load_projects')`, `check_sync_status`… write `projects`/`projectRuntime` | those two refs delta out; spinner state is `projectRuntime.refreshCount`, itself mirrored → the browser sees the spinner spin and stop |
| 3 | **Switch usage account** | `useAgentUsage` factory → keyed singleton in `store/usageStore.js`; `selectAccount(sourceId,email)` becomes an action | intent runs on Mac; the Mac refetches/repins | `usageStore` refs (`data,accounts,viewingEmail,loading,…`) delta out |
| 4 | **Sync check on/off** | *none* — `syncCheckEnabled` is already a store ref, `toggleSyncCheck` already an exported fn; only wrap in `action()` | intent runs on Mac → flips ref, writes the Mac's `localStorage`, `bumpEpoch`s every project | `syncCheckEnabled` + `projectRuntime` delta out |
| 5 | **Usage monitor on/off** | `useToggleableSource` moves from `AgentUsageSection.vue` to `store/usageSourceStore.js` | identical to case 4 | `enabled` delta out; because the *Mac's* `enabled` flipped, the Mac's poll timer starts/stops — the browser changed real behaviour, not a local flag |

The pattern is the same three lines for all five, which is the point:
**ref lives in `src/store/*` → mirrored automatically. Mutation is an `action()` → forwarded
automatically. Nothing in `bridge/mirror/intents/tauri.js` is edited when a feature is added.**

Case 4 is the strongest evidence: with the seams in place it is **literally a zero-diff feature**
beyond one `action()` wrapper.

---

## 7. Rust — relay only

`src-tauri/src/web_server.rs`, new deps `axum` + `tokio` (`ws` feature) + `if-addrs` (LAN IP without
shelling out).

| Route | Purpose |
| :-- | :-- |
| `GET /ws?role=host\|companion&token=…` | the relay. host frame → all companions; companion frame → host. No parsing of payloads beyond the role tag. |
| `GET /*` (prod only) | static `dist/` |

New commands: `start_companion_server()`, `stop_companion_server()`, `get_companion_url()`,
`list_paired_devices()`, `revoke_device(id)`, `get_project_icons_map()`, `read_text_file(path)`.

### 7.0 Icons — backend scans and HOLDS, so nothing can 404 (learned from the parked WIP)

The parked attempt died to a **404 storm on icons**: the browser (and even the Mac) kept *requesting*
each icon and getting 404s in a loop. Root cause: icons were treated as a *fetchable resource*. The
decision, applied everywhere:

> **Invariant ICON-1 — an icon is HELD STATE, never a request.** The backend scans project dirs once,
> holds a **complete** map in memory (every project id → either its icon bytes+mime, or an explicit
> `none`), and hands the frontend a map of `id → data:` URI. The frontend renders straight from that
> mirrored state. **There is no icon endpoint and no icon fetch on either side, so a 404 is
> structurally impossible.**

* `get_project_icons_map()` (async, `spawn_blocking`) returns `{ id: "data:image/png;base64,…" | null }`,
  built from a held in-memory scan. `null` = "scanned, none found" — a *terminal* answer that stops
  any retry, not a miss that re-requests.
* The map is a mirrored key (`projectStore.projectIcons` in §2), so the browser gets it via the state
  mirror like everything else — **no `/icon/` route, no custom-scheme dependency on the browser.**
* Re-scan is an explicit host event (projects changed / reload), never a per-render pull. The old
  `aki-devsync-icon://` scheme may stay for the *native* window if convenient, but it is no longer on
  any hot path and never reached from a browser.

### 7.1 Security — pair once per device, then silent reconnect (resolved)

The three textbook options were each rejected as a *sole* mechanism: per-launch token = re-pair every
time the app restarts (annoying); open-LAN = unsafe on café/public wifi; Tailscale-only = would make a
third party mandatory. The chosen model is **trusted-device pairing**, which has none of those costs
and is **transport-agnostic** — so it secures LAN and Tailscale with the *same* token, no extra path:

```
FIRST TIME (pairing)                          EVERY TIME AFTER
phone opens http://<mac-ip>:1421              phone reconnects with its stored deviceToken
   │ no token → server serves a               → server checks token ∈ paired set
   │   "pair this device" page only           → OK → full mirror. No QR, no retype.
   ▼
Mac shows a 6-digit code (a modal)
phone enters it → Mac mints a random
per-device token, returns it
   │
   └─ phone stores it (localStorage,
      survives app restarts on BOTH ends)
```

* **Persistence is per-device, both ends.** The phone keeps its `deviceToken` in `localStorage`; the
  Mac keeps the paired-token set in a small JSON next to `projects.json` (`dirs`-resolved app config
  dir). Restarting the Mac app does **not** invalidate already-paired phones → no reconnect friction,
  the exact complaint about the per-launch option.
* **Bound `0.0.0.0`, but useless without a token.** A stranger on the same wifi hitting `:1421` gets
  only the pairing page and cannot mirror or send intents without the 6-digit code shown on the Mac
  screen (which they don't have physical access to).
* **Revocable.** `list_paired_devices()` / `revoke_device(id)` back a tiny "paired devices" list in a
  settings modal (lost/old phone → revoke one token, others unaffected — this is a multi-entity store,
  so CLAUDE.md's scoped-clear rule applies: revoke removes ONE token, never wipes the set).
* **WS gate:** `/ws?role=companion&token=…`; a bad/absent token → close code 4001, the companion UI
  shows "pair this device", never a blank mirror.
* This is not multi-tenant (SCOPE-1): paired devices are all mirror screens of the **one** Mac
  session, not separate accounts.
* **Host entry point — the header dropdown "Remote Control" section (shipped 1.18.x, host-only).**
  The on/off toggle lives in the existing hamburger menu (`AppHeader.vue`), next to SSH config / Claude
  profile / window presets, per the Extreme-Narrow rule (a section in the one app-level menu, not a new
  header button). ON → calls `start_companion_server()` and shows the 6-digit **pair code** inline plus
  every reachable **IP:PORT** (LAN + Tailscale, from `get_companion_url()`, click-to-copy); OFF →
  `stop_companion_server()`. Logic in `src/composables/useRemoteControl.js` (module-scope singleton),
  `invoke` via the Seam-N wrapper (`utils/tauri.js`). This minimal surface is enough to *pair and know
  the address*; the **full pairing modal** (QR code + paired-device list backed by
  `list_paired_devices()`/`revoke_device()`) is **Wave 2** — the menu section will gain a "Manage
  devices…" link into it. The section is `v-if="isHost"`, so a companion (phone) never sees a control
  it cannot action.

### 7.1a Reach from outside the LAN — Tailscale, natively, no patchwork

The requirement is to reach the Mac from *anywhere*, not just the local wifi — without a second,
bolted-on remote path. This falls out for free because the server already binds `0.0.0.0` and the
pairing token is transport-agnostic:

> **Invariant NET-1 — one server, one token, every reachable address.** The same `:1421` listener is
> simultaneously reachable on the LAN IP *and* on the Mac's Tailscale IP. Pairing works identically
> over either. Nothing about auth or the protocol is Tailscale-specific.

* **Native address discovery, no CLI shell-out.** `if-addrs` enumerates every interface. We classify
  each address: LAN (`192.168/16`, `10/8`, `172.16/12`), loopback, and **Tailscale = the CGNAT range
  `100.64.0.0/10`** (plus the `fd7a:115c:a1e0::/48` v6 ULA Tailscale uses). Presence of a `100.64/10`
  address == Tailscale is up; we surface it as a reachable URL. **No `tailscale` binary is invoked** —
  reading the interface list is pure and cannot hit the cold-start PATH race (CLAUDE.md GLOBAL TAURI
  STACK). This is the "native, not patchwork" bar.
* **`get_companion_url()` returns *all* reachable addresses**, tagged: `[{kind:'lan', url}, {kind:'tailscale', url}]`.
  The pairing modal shows LAN first (fast local case) and, when present, a Tailscale address + QR for
  the remote case. Same 6-digit code, same stored token for both.
* **Nothing extra to secure.** A device paired over the LAN keeps working over Tailscale and vice
  versa — the token is the identity, not the network path. Over Tailscale the reachability is exactly
  the tailnet's (already authenticated at the WireGuard layer), *and* still gated by our token — two
  independent layers, no weakening of either.
* **Out of scope (SCOPE-1 holds):** we do not run/install/configure Tailscale for the user, do not
  bundle it, do not manage the tailnet. We only *notice* its address and offer it. If it isn't
  installed, the Tailscale row simply doesn't appear; LAN is unaffected.

**Rule compliance (from CLAUDE.md):**

* **NEVER BLOCK UI** — the server runs on Tauri's tokio runtime, spawned in `setup()`. Every new
  `#[tauri::command]` here is `async fn`; LAN-IP discovery uses `if-addrs` (pure, no subprocess). If
  any variant ends up shelling out, it goes through `tauri::async_runtime::spawn_blocking`.
* **Capabilities** — no new frontend permission is needed for a WS the page opens itself, but the new
  commands are auto-covered by `core:default`; re-check `src-tauri/capabilities/default.json` before
  shipping (silent no-op class).
* **CSP — two distinct surfaces, don't conflate them.**
  * *Mac webview* is governed by `tauri.conf.json`'s CSP (`default-src 'self'`), which **blocks the
    Mac webview's own WS to `127.0.0.1`.** Add `connect-src 'self' ws://127.0.0.1:1421 ws://localhost:1421`.
  * *Browser companion* is a normal web page; its CSP (if any) is whatever axum sends. It connects to
    `ws://<lan-ip>:1421` **or `ws://<tailscale-ip>:1421`** — the page derives the WS host from its own
    `location.host`, so no address is hardcoded and Tailscale needs no extra CSP entry. (If the page is
    ever served over TLS, the WS must be `wss://` — out of scope now, noted for NET-1.)
  * Missing either is a **silent** connect failure — no error, no log (the IPC-capability failure
    class from CLAUDE.md, applied to CSP).
* **Version SSOT** — no version field is added anywhere; `Cargo.toml` bumps with `package.json`.

**Dev vs prod — see §7.2. axum on 1421 is the ONE LAN entry in both; `get_companion_url()` returns
`:1421` in both, which is why the menu needs no dev-vs-prod port special-casing.**

---

## 7.2 Single-port dev parity — axum is the ONLY LAN entry, dev AND prod (INVARIANT PORT-1)

**Goal (locked):** the companion address the user types must be **identical in dev and release** —
`http://<ip>:1421` — so DX matches production and there is no "which port in which mode" trap. The
earlier plan drafted a 1420/1421 split (Vite serves the page in dev); that is **rejected** as a DX
regression. axum owns `1421` as the sole LAN-facing server in *both* modes:

> **Invariant PORT-1 — one LAN port (1421), identical contract in dev and prod.** Everything the
> phone loads or calls goes to `:1421`. The frontend never learns which mode it is in; the split is
> confined to how axum *fulfils* a page request, invisible past that boundary.

| Request to `:1421` | Release (`.dmg`) | Dev (`npm run tauri dev`) |
| :-- | :-- | :-- |
| `GET /ws`, `POST /pair` | handled by axum locally | handled by axum locally |
| any other HTTP (`/`, assets) | **served from the EMBEDDED bundle via `app.asset_resolver()`** | **reverse-proxied to the Vite dev server on `127.0.0.1:{vitePort}`** |
| Vite HMR websocket | n/a (no HMR in prod) | **direct to Vite** — the Mac window hot-reloads normally; the phone uses manual refresh (see HMR below) |

* **Release serving — use `app.asset_resolver()`, NOT `ServeDir` on disk.** Tauri v2 compiles
  `frontendDist` **into the binary** and ships **no loose `dist/`** in the `.app` bundle, so the
  first-draft `resolve_dist_dir()` + `ServeDir` finds nothing and 404s (this is the exact bug hit in
  testing). `asset_resolver().get(path)` reads the embedded bytes (`.bytes` + `.mime_type`) and needs
  zero extra `bundle.resources`. SPA fallback: an unknown path serves `index.html` so client routing
  still boots. **Delete `resolve_dist_dir()` and the `ServeDir` branch.**
* **Dev serving — reverse-proxy to Vite.** axum keeps `/ws` + `/pair` local and forwards every other
  HTTP request to the Vite dev server, streaming the response back. Vite stays **localhost-only** — it
  needs no LAN exposure because the phone only ever reaches it *through* axum's proxy.
  **Proxy the whole origin from `app.config().build.dev_url`, host included — never a hardcoded
  `127.0.0.1`.** (`scripts/tauri-runner.js` overrides `devUrl` at runtime with the free port it picked,
  so that URL is authoritative for host *and* port; fallback `http://localhost:{TAURI_DEV_PORT|1420}`.)
  Vite binds the **name** `localhost`, which macOS resolves to **`::1` first** — so a v4-literal dial
  gets connection-refused while the identical server answers on the name. That was a real, shipped
  failure (`vite dev server unreachable on 127.0.0.1:1420`, 502 to the phone) whose signature is
  confusing: the Mac window worked the whole time, because it loads `devUrl` by name.
* **HMR stays DIRECT to Vite (decided — the axum HMR bridge was tried and REMOVED).** The obvious-
  looking move — set Vite `server.hmr.clientPort = 1421` + a custom path and have axum bridge that
  websocket to Vite — **backfired**: `clientPort` is global, so it rerouted the *Mac dev window's* own
  HMR (which loads Vite directly and never needed the proxy) through the bridge too, and the bridge's
  `connect_async` to Vite's HMR socket was refused — a tight connect-refused retry loop that spammed
  the log and flicker-reloaded the window, making dev unusable. **Lesson: do not touch HMR.** Leave
  Vite's HMR at its default (`hmr: undefined` in the localhost case) so the Mac window hot-reloads
  directly against Vite; the phone (whose page is proxied) simply has no live HMR in dev and is
  refreshed manually. Cost: only the phone's dev-time hot-reload — the *page* address stays single-port
  1421 (PORT-1 intact) and the log stays clean. If phone HMR is ever wanted, the safe way is to expose
  Vite on the LAN (`host: true`) so the phone reaches Vite's HMR port **directly**, never via an axum
  bridge.
* **The Mac window is untouched.** In dev it still loads from `devUrl` (Vite:1420) directly and in
  release from the embedded bundle — only the **companion** path is proxied. Host `__AKI_ROLE__`
  stamping (§9) is unaffected.
* **Rules:** the proxy work is async on the tokio runtime (NEVER-BLOCK-UI holds — no `spawn_blocking`
  needed, it is not CPU/subprocess work). No new `#[tauri::command]`. CSP unchanged: the companion page
  axum serves carries no restrictive CSP, and the host's own `127.0.0.1:1421` WS is already
  allow-listed in `connect-src` (§7.1a). One new crate: `reqwest` (with `default-features = false` —
  it only ever dials `http://127.0.0.1`, so no TLS backend) for the dev HTTP proxy. **Confirmed to
  compile + run on the Mac** (`reqwest 0.12`, `asset_resolver()`, `app.config().build.dev_url` all
  build clean).
* **PORT-1 corollary — 1421 is reserved, Vite may never take it.** `scripts/tauri-runner.js` picks the
  dev port by scanning upward from 1420 for a *currently free* port, and derives the HMR port as
  `devPort + 1` — which is **1421 in the normal case**. Both would happily claim the relay's port a few
  seconds before the app starts, after which axum's bind fails with nothing but an `eprintln!` and
  remote control is silently dead for the whole session (the failure mode is indistinguishable from
  "the feature doesn't work"). The scan therefore **skips 1421**, `hmrPort` falls to `devPort + 2` when
  it would land on 1421, `TAURI_FORCE_PORT=1421` is rejected with a message, and `vite.config.js`
  throws if it is ever handed 1421 anyway (defence in depth for a standalone `npm run dev`).

### 7.2a Hardening — findings from the post-implementation audit

Three gaps the first pass left open; all closed in `src-tauri/src/web_server.rs` + the host composable.

* **"Off" must mean off for the HTTP surface too (was: page always served).** Only `/ws` and `/pair`
  checked the `enabled` gate, so with remote control switched **off** anyone on the LAN could still
  `GET :1421/` — the release SPA, or in dev a full reverse-proxied view of the Vite dev server
  *including its `/@fs/` source endpoint*. `reject_if_disabled()` now guards both fallbacks (503).
  Safe for the Mac window in both modes because it never loads assets through axum (dev: Vite on
  localhost; release: the embedded `tauri://` protocol).
* **Pairing-code brute force (was: unlimited attempts).** A 6-digit code is ~10⁶ combinations with no
  expiry and no lockout — walkable from the LAN in minutes. `MAX_PAIR_FAILURES = 10` consecutive bad
  codes now **disables the relay**; the user re-enables from the menu, which mints a fresh code, so the
  attacker's progress is discarded too. The counter resets on success and on `start_companion_server`.
* **Host-UI/relay desync (was: menu could lie).** `enabled` + the pairing code live in the Rust
  process, which outlives the webview: an HMR reload in dev — or any webview reload — reset the
  frontend's `running` ref to `false` while the relay was still serving the LAN and paired phones were
  still connected, i.e. the menu showed **Off** for a server that was **on**. New command
  `get_companion_status()` (enabled / code / port); `useRemoteControl()` re-syncs from it once per page
  load, host only. The toggle also re-asserts the DOM checkbox after `start()` fails, which otherwise
  stayed visibly checked against a `false` ref (no Vue patch, since the ref never changed).
* Dev proxy also answers any websocket **upgrade** attempt with `501` instead of handing back
  index.html — the phone's Vite HMR client then fails fast rather than hanging on a lying `200`
  (it still has no HMR in dev, by the decision above; this only makes the failure honest).

---

## 7.5 In-app file viewing — the phone has no Finder

**Problem.** The Mac has Finder; the phone does not. Anything the app produces as a *file* (starting
with `REPORT.html`) is unreachable from the phone unless the app itself can show it. So this round
adds a **web-native, shared file view** — one viewer both host and companion use, no OS file browser.

* **This round (in scope): view `REPORT.html` from the phone.**
  * New command `read_text_file(path)` (async, `spawn_blocking`) returns file text. It is a **native
    read**, so on the companion it goes through seam N (RPC to the Mac) — the phone never touches the
    Mac filesystem directly, the Mac reads and returns the bytes.
  * A `FileView` component renders the returned content. For `REPORT.html` it renders inside a
    **sandboxed `<iframe srcdoc>`** (the report is self-contained, no external requests — it already
    satisfies that by construction), so it displays identically on Mac and phone with no new asset
    route. Existing `resolve_report_html` locates the file; `read_text_file` returns it.
  * **Path safety (FILE-1):** `read_text_file` must confine reads to an allow-list of roots (project
    dirs + the report path), never an arbitrary path from the wire — a companion is remote-controlled
    input and must not be able to read `~/.ssh/id_rsa` by asking. Enforced on the Mac side.
* **Next round (roadmap, NOT this round):** a general **Explorer View** (file-tree browser) and a
  **Terminal View** (xterm.js, VS Code-style, web-native) built on the same `FileView`/seam
  foundation. Scoped separately in `docs/plan/remote-views-roadmap.md` so it does not bloat this
  round. The relevant design constraint to preserve *now*: `read_text_file` + `FileView` are the first
  members of a **view subsystem**, so keep them generic (not `REPORT.html`-specific) even while only
  the report is wired.

---

## 8. One-time refactor — the whole cost, enumerated

Nothing below is remote-specific plumbing; it is 1.18.0 state being moved to where invariant SSOT-1
and ACT-1 already say it belongs.

* **R-1 · Shared state → `src/store/*`** (the only structural change)
  * `AgentUsageSection.vue` `useToggleableSource` × 3 (`ag`, `ccLocal`, `ccRemote`) → `usageSourceStore.js`
  * `useAgentUsage.js` factory → keyed singleton store (`usageStore.get(sourceId)`), refs into a
    reactive map so one mirrored key covers all three sources
  * modal/singleton refs already at module level in `useGit.js`, `useProjectConfig.js`,
    `useProjectTasks.js`, `useGlobalNote.js`, `useBackgroundRefresh.js` → re-home into `src/store/`
    (pure move; see Q1 in §11 — if view state stays local, only the *data* refs move)
* **R-2 · Writes → actions** — `ProjectTable.vue:249` (`v-model` on `p.dry_run`); `startSync(project…)`
  → `startSync(projectId…)`; any other component that assigns to `projects`/`projectRuntime`.
* **R-3 · Import swap** — 19 files import `invoke` from `@tauri-apps/api/core` → `utils/tauri.js`.
  Mechanical. While doing it, audit the 14 component-level `invoke` sites (10 × `macos_open`,
  `check_for_updates`, `apply_statusline_config`, `set_claude_profile`, `check_statusline_status`, …):
  a site that mutates **mirrored** state must become an action; a pure native effect or a query
  feeding local view state can stay an `invoke` (seam N returns a real value, so it works as-is).
* **R-4 · Producer gate** — 5 sites in §5 through `hostInterval` / `onHostBoot`.
* **R-5 · Window controls** — `useAppWindow.js` reads `bridge.isHost`; on the browser, pin/minimize/
  close/resize are no-ops and the controls are hidden (a browser tab must not close the Mac app).
* **R-6 · Icons** — `utils/projectIcon.js` builds the URL from `bridge.assetBase`
  (`aki-devsync-icon://` on the Mac, `/icon/` on the browser).

Estimated blast radius: ~8 new/moved files, ~19 one-line import swaps, 1 template change, 5 gated
producers. **No component tree change, no state-management rewrite, no Rust logic moved.**

### 8a. What actually shipped (status after the Wave-2 pass)

| Item | State | Notes |
| :-- | :-- | :-- |
| **R-1** state → `src/store/*` | **Already satisfied** — no move needed | The audit found 7 store modules with 26 exported refs (`projects`, `projectRuntime`, `sshStore`, `logStore`, `refreshStore`, `syncCheckStore`, `usageTierStore`, `claudeModeStore`); the composables hold essentially no module-scope state left. The planned `usageStore` keying is **not** done — usage refs still live in the `useAgentUsage` factory, so usage numbers are the one area the mirror does not yet carry. |
| **R-2** writes → actions | **Done (core control surface)** | `src/store/remoteActions.js` (NEW) exposes id-based, `action()`-wrapped entry points: `requestSync(id, dir)` → `startSync`, `setDryRun(id, val)`, `requestRefresh(id)`, `requestRefreshAll()`. `syncCheckStore.toggleSyncCheck` is now wrapped in place. `ProjectTable.vue` PUSH/PULL/DRY/per-project-refresh and `AppHeader` global refresh route through these — so a phone can push, pull, flip DRY, refresh and toggle sync-check, and the result mirrors back to every screen. **Actions take a project `id`, never the object** (a companion serializes args to JSON — an object would arrive detached and the host's in-place mutations would never land in the reactive `projects` array; the host re-resolves the live object by id). On the host `action(fn) === fn`, so behaviour is byte-identical to 1.18.0. **Load-order fix (REGISTRY-1):** giving store modules `import { action }` makes intents ⇄ store a real import cycle; `intents.js` now builds its dispatch REGISTRY **lazily** (first dispatch/resolve, not at module-eval) so a store caught mid-evaluation by intents' eager glob can't throw a TDZ ReferenceError at bootstrap. **Still host-only (Wave 2, §3.4):** the `--delete` "type the project name" confirm and the native file-picker Upload run on the Mac — a phone-triggered `--delete` prompts on the Mac, and the normal no-dialog push/pull/DRY path is fully phone-usable. |
| **R-3** import swap | **Done** — 17 files | `src/utils/tauri.js` is now the only importer of `@tauri-apps/api/core`. |
| **R-4** producer gate | **Done** | `hostInterval`: 2 usage timers, 2 background-refresh timers. `onHostBoot`: App.vue's whole boot sequence, `useLogs`' `listen('sync-log')`, and the `{immediate:true}` host-watch boot fetch in `useAgentUsage`. Display-only clocks (`AgentUsage.vue` ×2, `UsageCircle.vue`) deliberately left raw — gating them would freeze the phone's own countdowns. |
| **R-5** window controls | **Done** | `useAppWindow()` returns a companion stub *before* touching `getCurrentWindow()` (which throws in a browser) and exposes `nativeWindow: false`; AppHeader hides the three titlebar buttons and the `AppWindow:` preset block. |
| **R-6** icons | **Done** | `utils/projectIcon.js` — `aki-devsync-icon://` on the host, the mirrored data URI from `projectStore.projectIcons` on the companion. Filled on host boot by `refreshProjectIcons()` → `get_project_icons_map`. **Only refreshed at boot**: adding a project mid-session does not yet re-fill the map for the phone. |
| **Pairing UI** (was missing entirely) | **Done** | `PairingGate.vue` + `useCompanionPairing.js`. Before this, `connectionState = 'unpaired'` had no consumer at all — a fresh phone could never enter the code, so pairing was impossible regardless of the relay working. |

**READY-1 — the dashboard mounts behind a readiness gate, not just under an overlay.** First attempt
put `PairingGate` as an overlay *on top of* a fully-mounted dashboard. On the phone that meant every
child (`AppHeader`, `AgentUsage`, …) mounted while the socket was still closed, and their `onMounted`
`invoke`s went out over a dead bridge — the observed `bridge.js: send dropped, socket not open` burst
plus `check_for_updates` throwing. Fix: `App.vue` wraps the entire dashboard in `v-if="ready"`
(`useCompanionPairing.ready` = host always, companion only once `connectionState === 'open'`). An
unpaired or reconnecting phone shows only `PairingGate` (enter-code, or a "connecting…" status once it
already holds a token); nothing else exists to fire an early call. `check_for_updates` in `AppHeader`
is *additionally* wrapped in `onHostBoot` — it is a host-only concern even on a paired companion.
This is the invoke-side twin of R-4's timer gate: **a companion must not produce, and "mounted but
not connected" is exactly when it would.**

---

## 9. Where environment branching is allowed to exist

**Defined in exactly one place:**

```js
// services/bridge.js — our own role marker, not Tauri's globals (see §11 S-1)
export const isHost = typeof window !== 'undefined' && window.__AKI_ROLE__ === 'host'
```

**Consumed by exactly four boundary modules, and nowhere else:**

| Module | Why it must know |
| :-- | :-- |
| `services/bridge.js` | which WS URL / role to open |
| `services/mirror.js` | broadcast vs apply |
| `services/intents.js` + `utils/tauri.js` | run vs forward |
| `utils/scheduler.js` | producers on vs off |
| `composables/useAppWindow.js` | native window vs no-op (R-5) |

> **Invariant ENV-1 — `isHost` must never appear in `src/components/**`, `src/store/**`, or any
> composable other than the boundary modules named below.** A grep for it in those paths is a
> CI-grade check and the single best regression guard for this whole feature. If a new feature seems
> to need it there, the seam is the wrong shape — fix the seam, not the feature.

**The complete consumer list as built** (anything else is a violation):
`services/mirror.js`, `services/intents.js`, `utils/tauri.js` (seam N), `utils/scheduler.js` (seam P),
`utils/projectIcon.js` (ICON-1, reads the derived `assetBase` rather than `isHost` itself),
`composables/useAppWindow.js` (R-5), `composables/useRemoteControl.js` (host-only menu section),
`composables/useCompanionPairing.js` (companion-only pairing gate). Components consume these under
neutral names (`available`, `nativeWindow`, `needsPairing`) so the role token never reaches a template.

> **R-2 corollary — a store may consume the seam without knowing the role.** `syncCheckStore.js`
> and `remoteActions.js` (R-2) `import { action }` from `services/intents.js`, but they do **not**
> import or reference `isHost`/`__AKI_ROLE__`/`__TAURI_INTERNALS__` — `action()` encapsulates the
> host-vs-companion branch entirely. So ENV-1 still holds by its letter (no role token in
> `src/store/**`): the grep guard is for the role *symbols*, and importing the role-agnostic
> `action` wrapper is exactly the sanctioned way for a store to become intent-able.

---

## 10. Risks and unknowns

| | Risk | Mitigation / decision needed |
| :-- | :-- | :-- |
| **R-1** | **LAN exposure.** Port 1421 bound to `0.0.0.0` exposes full control of the Mac (rsync, git, terminal spawn) to anyone on the network. The single most serious item in this plan. | **Resolved (§7.1):** trusted-device pairing — 6-digit code once, per-device token persisted both ends, `0.0.0.0` but no mirror without a paired token. Revocable per device. |
| **R-2** | **Confirm dialogs.** `useSync.js` "type the project name to confirm delete" (Swal, 6 sites) runs where the *action* runs — i.e. on the Mac, unreachable from the browser. | **Resolved (§3.4):** dialogs become mirrored `dialogStore` state, answered from either screen via an action — uses the two existing seams, no new transport. Per SYNC-1 a dialog is a data event. |
| **R-3** | **Log flooding.** `projectLogs` grows line-by-line during a sync; a whole-value delta per line is O(n²) traffic on a long rsync. | v1: coalesce per microtask (already spec'd) + cap retained lines in `logStore`. If measured traffic is still bad, add append-delta for array refs — one generic rule for all array refs, still no per-key wiring. |
| **R-4** | **`localStorage` on the browser.** Stores read `localStorage` at module init (`syncCheckStore`, source toggles). On the browser those initial values are meaningless — but harmless: the `init` snapshot overwrites them within milliseconds, and writes only happen inside actions, which run on the Mac. | Accept. Document it; do not add a branch. Cosmetic first-frame flicker only. |
| **R-5** | **Concurrency.** Two operators (Mac + browser) acting at once. Actions are serialized by the host's single JS thread, so no lost update; but a delete-confirm answered on one screen while the other clicks again is undefined. | Depends on Q1/Q2. Cheap guard: existing `projectRuntime.syncing` already blocks a second sync and is mirrored. |
| **R-6** | **Host/companion detection.** ~~Unknown~~ **Resolved (§11 S-1):** our own `window.__AKI_ROLE__` marker — host self-stamps via a Tauri init script (dev+prod), axum stamps companion (prod), default companion. No dependency on Tauri's globals. | none |
| **R-7** | **`action()` reverse lookup.** ~~Unknown~~ **Resolved (§11 S-2):** proven with a Node ESM spike — namespace identity holds. | none |

---

## 11. Decisions (resolved) and the two spikes still gating code

### Resolved in this design round
* **Q1 (view vs data state)** → **SYNC-1** (§1.1). On-input local, data events mirrored. Not the
  "mirror everything" extreme, not "mirror nothing". Form drafts are local; Save is a data event.
* **Q2 (where a confirm appears)** → **§3.4.** Mirrored `dialogStore`, answerable from either screen,
  first-answer-wins, typed value validated on the Mac.
* **Q3 (port security)** → **§7.1.** Trusted-device pairing, per-device persistent token, revocable.
* **Q4 (`isHost` in "one place")** → dropped as a literal constraint (the keyword carried stale
  context). The real, enforceable rule is **ENV-1** (§9): one *definition*, consumed only by the
  named boundary modules, banned everywhere else and grep-checkable. That is what CI guards.

### Scope pin (new, from this round)
* **SCOPE-1** (§0.5). Phone-first signal-remote of ONE session. Multi-user LAN co-development is a
  different app and explicitly out of scope. Anything justified only by "many simultaneous operators"
  is rejected at design time.

### S-2 · `action()` reverse-lookup — PROVEN (no longer a blocker)
Verified with a plain Node ESM spike (`scratchpad/glob-spike/`, no Tauri/Vite needed): a module
namespace value **is** the same function object as the named export (`ns.setProjectFlag === setProjectFlag`
→ `true`), so a companion `action()` stub finds its own key by identity — for both the host-unchanged
fn and the companion stub. Non-function exports (refs) stay enumerable for mirror discovery. The only
Vite-specific residue — that `import.meta.glob({eager:true})` yields namespace objects — is documented
Vite behaviour, not a risk. Fallback if ever needed: explicit key `action('setProjectFlag', fn)`, a
one-word change.

### S-1 · host vs companion detection — DECIDED: our own role marker, not Tauri's globals
We do **not** depend on Tauri's `window.isTauri` (could be renamed across Tauri versions). We define
**our own** marker, `window.__AKI_ROLE__`, stamped positively at each entry point, and default to the
safe side (companion):

```js
// services/bridge.js
export const isHost = (typeof window !== 'undefined') && window.__AKI_ROLE__ === 'host'
// anything else — including no marker at all — is treated as companion (the safe default:
// mis-detecting a phone as host is the dangerous direction; mis-detecting the Mac as companion
// is merely inert).
```

Who stamps the marker:

| Entry | Stamped by | When | Role |
| :-- | :-- | :-- | :-- |
| **Mac webview (host)** | a **first-import side-effect module `src/boot/roleStamp.js`**: `if (window.__TAURI_INTERNALS__) window.__AKI_ROLE__='host'`, imported as line 1 of `main.js` | **dev *and* prod** — the load-bearing mechanism | `host` |
| **Browser (companion)** | nothing needs to stamp it; a plain browser has no `__TAURI_INTERNALS__`, so the marker stays unset and `bridge.js` defaults to companion (axum *may* still positively inject `='companion'` as belt-and-suspenders, not required) | **dev *and* prod** | `companion` |
| **No marker** | — | any browser | defaults to `companion` |

**Why a first-import module, not the Rust init-script originally specified (IMPLEMENTATION CHANGE,
1.18.x):** the main window is declared **declaratively** in `tauri.conf.json`'s `app.windows`, so
there is no `WebviewWindowBuilder` to hang `.initialization_script()` on, and Tauri v2 has no
per-config-window init-script field. Converting to a programmatic window (to regain the builder) is a
high-blast-radius Rust change that can only be built/tested on the Mac — not worth it. Instead
`main.js`'s **first import** is a side-effect module (`src/boot/roleStamp.js`). ES module imports
evaluate depth-first in source order, so it runs before the `App.vue` subtree — which is what pulls in
`services/bridge.js` (via `AppHeader → useRemoteControl → utils/tauri`) — so `window.__AKI_ROLE__` is
set before `bridge.js` reads it at module-eval time, **zero Rust needed**.

> **Not an inline `<head>` script** (a first draft used one): the host webview's CSP is
> `script-src 'self'` with no `'unsafe-inline'` (`tauri.conf.json`), which **blocks inline scripts on
> the host** — the one place the stamp must run. A same-origin module is allowed; an inline script is
> silently dropped and `isHost` would be stuck `false` on the Mac. This is the CSP silent-fail class
> from CLAUDE.md, applied to role detection.

**On the `__TAURI_INTERNALS__` dependency:** S-1 originally rejected Tauri's own globals to avoid
coupling to Tauri's naming. This keeps `window.__AKI_ROLE__` as the *only* marker `bridge.js` reads
(the abstraction is intact) and confines the Tauri-naming coupling to **one line in `index.html`** — a
single documented boundary, a one-line fix if Tauri ever renames the internal. The safe direction still
holds: a phone browser never has `__TAURI_INTERNALS__`, so a companion can never be mis-stamped host.

Rejected alternatives: `location.protocol==='tauri:'` (prod-only, wrong in dev); UA sniffing (fragile);
scattering `__TAURI_INTERNALS__` checks across app code (the coupling S-1 rejected — but confined to
one boundary line, as above, it is acceptable and strictly simpler than a programmatic window).

No blocking spikes remain — both former blockers are resolved above.

---

## 12. Definition of done (phase 2, when it comes)

### 12.0 Companion smoke path — MANDATORY, run before claiming any of this works

Added after a review pass that read every file, reported the 18 modules importing `invoke` directly
from `@tauri-apps/api` as a *list*, and did **not** conclude the obvious: the companion was broken.
Having the evidence and not following it through is the failure this checklist exists to prevent.
Walk the companion's live path in order, and answer each question with a command's output, not a
guess:

1. **Module eval.** `grep -rn "@tauri-apps/api" src --include=*.js --include=*.vue` → the ONLY
   allowed hits are `src/utils/tauri.js` (seam N), `src/composables/useAppWindow.js` (seam-gated
   native window) and `src/composables/useLogs.js` (seam-gated `listen`). Anything else is a
   companion failure, not a note for later.
2. **`setup()`.** Any native call made at the top level of a `<script setup>` (not inside a handler)
   takes the whole component down on a browser — `getCurrentWindow()` reads
   `__TAURI_INTERNALS__.metadata` and throws. Grep `getCurrentWindow|convertFileSrc|listen\(` across
   `src/components` and `src/composables`; every hit must sit behind a seam.
3. **Boot / `onMounted`.** Every boot fetch goes through `onHostBoot` (seam P), AND the whole
   dashboard mounts behind `v-if="ready"` (READY-1) so no child's `onMounted invoke` can fire before
   the companion socket is open. `invoke` is `async` in `@tauri-apps/api/core`, so a companion
   failure there is a *rejected promise*, not a crash — the app renders and looks fine while being
   empty. Silence here proves nothing; watch for `send dropped, socket not open` in the phone console.
4. **Timers.** Every `setInterval` that polls or fetches goes through `hostInterval`. Display-only
   clocks stay raw — gating them freezes the phone's own UI.
5. **Native URLs.** No `aki-devsync-icon://` (or any custom protocol) in a component; those resolve
   to nothing in a browser. Icons go through `utils/projectIcon.js` (ICON-1).
6. **Pairing.** A device with no token must be able to *enter the code* — `connectionState` has to
   have a consumer (`PairingGate.vue`). A gate that exists but nothing renders it is the same bug as
   no gate at all.
7. **Say what was NOT verified.** This dev box cannot build or run the Mac binary. Anything Rust-side
   (release `asset_resolver` path, the `[::]` bind, live pairing) is *unverified* until the user runs
   it — list those separately instead of folding them into "reviewed".

* `grep -rn "isHost\|__TAURI_INTERNALS__" src/components src/store src/composables` → only the
  boundary modules: `useAppWindow.js`, `useRemoteControl.js`, `useCompanionPairing.js`,
  `utils/tauri.js`, `utils/scheduler.js`, `utils/projectIcon.js`. The `__TAURI_INTERNALS__` read
  lives in `src/boot/roleStamp.js` (§9) — not `index.html`, which the host CSP would block.
* The 5 cases in §6 verified **both directions** with the Mac window visible next to a browser.
* SYNC-1 respected: typing a form field on the phone does not stream per-keystroke; a confirm-delete
  is answerable end-to-end from the phone (§3.4).
* A second, unpaired device on the same wifi cannot mirror or send intents (§7.1).
* Multi-entity guard (CLAUDE.md): verified with ≥2 projects and ≥2 usage accounts present — no
  cross-entity state loss when acting on one from the browser.
* No icon 404s on either side (ICON-1); `REPORT.html` renders on the phone (§7.5); a Tailscale address
  appears in the pairing modal when the tailnet is up and pairs with the same token (§7.1a).
* `npm run build` clean; `Cargo.toml` version matches `package.json`; no commit/push without asking.

---

## 13. Wire protocol contract — FROZEN (shared by every implementer, do not diverge)

This is the single contract the Rust relay and the JS seams both implement. Parallel work must not
invent variants. **All frames are JSON text over WS.** Envelope: `{ t: <type>, ... }`.

### 13.1 Connection & pairing
* WS URL: `ws://<host>:1421/ws?role=host|companion&token=<deviceToken>`.
* **Host** connects with `role=host` from `127.0.0.1`; no token required for a loopback host socket.
* **Companion** connects with `role=companion&token=…`. Bad/absent token → server closes with code
  **4001**; the companion UI shows the pairing screen.
* **Pairing (HTTP, not WS):**
  * `POST /pair {code}` → `{ token }` on match, `401` otherwise. `code` is the 6-digit shown on the
    Mac. `token` is a random 128-bit hex string the companion persists in `localStorage`.
  * The Mac holds paired tokens in `<appConfigDir>/companion-devices.json`:
    `[{ id, token, label, pairedAt }]`. `revoke_device(id)` removes exactly one entry (multi-entity
    scoped-clear, CLAUDE.md).

### 13.2 Frame types
| `t` | Direction | Payload | Meaning |
| :-- | :-- | :-- | :-- |
| `init` | host → companion | `{ v: { "<store>.<ref>": value, … } }` | full snapshot of every mirrored key; sent to a companion right after it joins |
| `delta` | host → companion | `{ v: { "<store>.<ref>": value, … } }` | one or more changed keys, coalesced per microtask |
| `intent` | companion → host | `{ key: "<store>.<action>", args: [...] }` | run this action on the host |
| `invoke` | companion → host | `{ id, cmd, args }` | RPC: run a native command, expect a reply |
| `invoke_result` | host → companion | `{ id, ok: value }` or `{ id, err: message }` | reply to a specific `invoke` id |
| `ping` / `pong` | both | `{}` | liveness; a socket with no `pong` within N s is dead → its in-flight `invoke`s reject |
| `companion-connected` | **relay → host** | `{ id }` | the relay sends this to the host's socket **whenever a companion authenticates and joins**. The host reacts by broadcasting a full `init` to everyone. This is the ONLY frame the relay originates; it carries a connection id, not app state — so the relay stays content-blind (§13.6). |

**Why a distinct tag, not an overloaded `init`:** this closes the gap where §2.3's prose ("relay tells
the host") had no frame — a content-blind relay cannot synthesize a state snapshot, but it *can* emit
a connection-event frame. A dedicated `companion-connected` (carrying the joining device `id`) beats
overloading `init`-without-`v`: no ambiguity, and Terminal View (next round) can route per-companion
PTYs by that `id`. Without this frame a companion joining a long-running host session sees nothing
until the next unrelated delta. (Full resync also happens whenever the host's own relay socket
re-opens.) *(Both Wave-1 lanes independently reached for a relay→host join signal; reconciled onto
this shape — the Rust lane already emits it, the JS mirror consumes `t==='companion-connected'`.)*

`PTY_INPUT` / `PTY_OUTPUT` are **reserved** for the next round (Terminal View) — do not implement, but
do not reuse the names.

### 13.3 Value encoding (SER-1)
* Values are JSON. `Set` → `{ "__t":"Set", v:[…] }`; `Map` → `{ "__t":"Map", v:[[k,v],…] }`. The
  decoder reverses these. (Only `usageStore.activeEmails` needs `Set` today.)
* A ref whose value cannot encode (DOM node, function — e.g. `logStore.consoleRef`,
  `logStore.globalListener`) is **auto-dropped from the mirror once, with a `console.warn`**. No
  hand-maintained exclusion list; the encoder decides.

### 13.4 Key namespace
* Every mirrored key is `"<storeFileBasename>.<exportName>"`, e.g. `projectStore.projects`,
  `logStore.projectLogs`, `usageSourceStore.agEnabled`. Both sides derive the identical set from
  `import.meta.glob('../store/*.js', { eager:true })`. The relay never interprets keys.

### 13.5 Icons & files (see §7.0, §7.5)
* Icons ride `delta`/`init` as `projectStore.projectIcons = { id: "data:…"|null }`. **No icon frame,
  no icon route.**
* File read is an `invoke` (`{cmd:'read_text_file', args:[path]}`) → `invoke_result`. Host enforces
  the `FILE-1` allow-list; a path outside it returns `{ err }`, never file bytes.

### 13.6 Relay rule (Rust)
The relay is **content-blind** except for routing: it reads `role` at connect time, then forwards
every `host→*` frame to all companions and every `companion→host` frame to the host. It never parses
`t`, never holds state, never transforms a payload. This keeps all protocol semantics in the JS seams
where the SSOT lives.

**One exception — a connection-event frame, not an app-state frame:** on each companion WS connect the
relay emits `{"t":"init"}` (no `v`) to the host socket (see §13.2). This is awareness of *its own*
connections, not app state, so content-blindness holds. It is the relay's only originated frame.

### 13.7 Note on the `action()` reverse-lookup target (implementation subtlety)
The companion's `action()` stub must find **itself** in the glob registry by identity — not the raw
`fn` passed in. On the companion, `action(fn)` replaces the store's exported binding with the *stub*,
so the glob namespace holds the stub; searching for `fn` would never match. The stub therefore closes
over itself: `const stub = (...a) => …findKeyByIdentity(stub)…; return stub`. (Confirmed during Wave-1
implementation — the plan's §3.2 pseudocode reads correct but would silently fail if taken literally.)
