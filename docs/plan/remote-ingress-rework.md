# Plan: Remote Control ingress — stop the cross-app teardown, make the edge swappable

Research this plan executes: `docs/research/remote-ingress-tailscale-conflict.md` — why Aki Dev Sync's `tailscale serve` and `aki-mcp-sv`'s `tailscale funnel` collide on one mount point, and what a non-Tailscale origin would cost.
Feature doc this changes: `docs/feat/remote-control.md` (its "HTTPS over Tailscale" section stops being the whole story).

## 1. What is being fixed, and why in this order

| # | Symptom the owner can see | Root cause | This plan's answer |
| :-- | :-- | :-- | :-- |
| W1 | Turning Remote Control off can kill `aki-mcp-sv`'s public endpoint | `set_tailscale_https(false)` runs `tailscale serve --https=443 off`, clearing the node's **whole** 443 handler set, not the one handler this app installed (`web_server.rs:1017`) | Own only our own mount: read `serve status --json`, act only on the handler whose proxy target is `127.0.0.1:1421`, and report when something else owns `/` |
| W2 | Ten wrong pairing codes from anyone permanently disable Remote Control | `pair_handler`'s strike counter is global and its penalty is `enabled = false` + persist (`web_server.rs:770-777`) — sound on a LAN, a remote kill switch on a public origin | Penalty becomes a throttle on pairing, not a shutdown of the server; plus a long secret for the public path (§4) |
| W3 | Remote Control only works where Tailscale works | Companion URLs are **derived from network interfaces** (`get_companion_url`, `web_server.rs:913-930`); a public origin is configured, not discoverable, so there is nowhere to put one | An ingress mode resolving to one stored `origin`, merged into the URL list as a third kind |
| W4 | Every remote setting lives in a cramped AppHeader dropdown | The feature outgrew a toggle; Extreme Narrow (`CLAUDE.md`) forbids growing that dropdown further | A Remote Control settings modal; the dropdown keeps only on/off + code + URLs |

**Order is by damage, not by visibility.** W1 is the only item that fixes harm the app can cause *today*, and it is independent of the other three — it ships alone if nothing else does. W2 must land **with or before** W3: shipping a public origin while the kill switch stands would put that switch on the open internet.

## 2. Design — one origin, three ways to get it

Ported, not invented: `aki-mcp-sv` shipped this in v1.8.0 (`~/aki/Nodejs/aki-mcp-sv/docs/plan/done/cloudflare-tunnel-ingress.md`). Its property — *"ingress is a swappable edge"* — is what kept OAuth, gatekeeper and bridge unchanged when Cloudflare was added. The same property holds here: the relay, pairing, mirror and PTY code never learn which edge produced the URL.

```
ingress mode (stored, one value)          resolves to           what the phone opens
──────────────────────────────────────────────────────────────────────────────────────
tailscale   (default, today's behaviour) → https://<magicdns>/  ← tailscale serve, app-managed
public      (owner runs the edge)        → https://<host>/      ← stored string, app does nothing
cloudflared (optional, later)            → https://<host>/      ← app spawns `cloudflared tunnel run`
lan         (always on, not a mode)      → http://<ip>:1421     ← interface discovery, unchanged
```

`lan` is not a mode: it is what `get_companion_url` already returns and it stays available in every mode. The mode chooses only what the *public* row is.

**`cloudflared` is deliberately last and separately optional.** `public` mode gets the owner unblocked with no new process to supervise — the tunnel is started however they already start it — and it is what makes the "one shared tunnel, two hostnames" arrangement possible later without redesign. Spawning `cloudflared` from a Tauri app adds a child process, its lifecycle, and its failure modes to an app that currently supervises none; build it only if running the tunnel by hand turns out to be the friction.

**Non-goals**, stated so they are not re-litigated mid-build: no shared hostname split by path (the companion SPA, `/ws` and `/pair` are all origin-relative — see the research doc F1); no Tailscale removal; no change to the wire protocol (`docs/plan/done/remote-control.md` §13 is FROZEN); no multi-tenant remote (SCOPE-1).

## 3. W1 — stop the cross-app teardown

Rust only, `src-tauri/src/web_server.rs`, no frontend change, no protocol change.

1. Replace the text-grep detector. `tailscale_serve_on()` (`:978-983`) greps `serve status` stdout for `127.0.0.1:1421`; parse `serve status --json` instead and return **who owns the 443 `/` mount**: ours, someone else's (with the target string), or nothing.
2. Enable stays `serve --bg http://127.0.0.1:1421`, but refuses when the mount is owned by another target, returning that target in the error so the UI can say *"`/` is currently served to 127.0.0.1:9999 — turn that off first, or use a different ingress"* instead of silently stealing it.
3. Disable acts only on our own handler and becomes a no-op when the mount is not ours. `--https=443 off` as a blanket command does not appear in this file again.
4. `useRemoteControl.js:79-83` (auto-disable on Remote Control off) keeps calling the same command — the safety now lives in the command, not in the caller, so no other call site can reintroduce it (`pattern.A8`: reshape the flow rather than guard each caller).

`TailscaleHttps` gains one field for the foreign-owner case; it is a `#[derive(Serialize)]` struct read by one composable, so the change is additive and needs no `#[serde(default)]` (nothing persists it).

**Not in W1:** answering whether `serve --bg` also clears `AllowFunnel` for 443. W1 makes it moot — the app stops issuing blanket commands either way — and the read-only diagnostic below reports the live state whenever someone wants to look.

## 4. W2 — the pairing gate, sized before it is changed

Verdict record (`METHOD-proportionality.md` C1), because this is a defensive mechanism being resized rather than a feature:

| Measure | Value | Basis |
| :-- | :-- | :-- |
| **Reach** | Today: anyone on the LAN or tailnet — small, semi-trusted. After W3: anyone who learns the hostname, i.e. background scanners that find every new public host within hours | Estimated. Reach is the measure W3 changes, and it is the entire reason W2 exists |
| **Capability** | One unauthenticated HTTP POST, repeated ten times. Bottom rung of the ladder — no exploit, no client tampering, no credential | Measured from the handler (`web_server.rs:768-782`): no per-IP limit, no proof of work, no auth before the counter |
| **Motive** | Denial only: the attacker gains no data and no access. Approximately zero targeted motive — but scanners do not need one, and the same request is indistinguishable from a curious probe | Estimated |
| **Blast radius** | Remote Control off, persisted across restart, pairing code cleared. **Recoverable only by physical access to the Mac** — which is exactly the access the feature exists to avoid needing | Measured: `state.enabled.store(false)` + `persist_enabled(false)` + `pairing_code.clear()` |

Blast radius is recoverable but the recovery is the one thing the user cannot do remotely, which is what makes it worth fixing rather than accepting (`proportion.B1`). Chosen rung: **B3.2 — enforced once at the trust boundary that already exists**, i.e. inside `pair_handler`, the single funnel every pairing attempt passes through. No new middleware, no per-route guards.

Shape:

1. **The penalty stops being a shutdown.** Bad codes lock **pairing** — subsequent `/pair` calls are refused for a cooling window — while `enabled` and every already-paired device stay untouched. A stranger can then cost the owner a delayed re-pair, never a walk to the Mac.
2. **The counter gets a key.** Global today, so one attacker starves everyone; count per source IP with a modest global ceiling behind it, so a distributed flood still cannot reach the old outcome.
3. **The code gets a second, longer form for the public path.** Six digits stay for LAN typing, where the strike counter and the network's own limits back them. A public origin additionally accepts a long random pairing secret carried in the URL (the shape `aki-mcp-sv`'s panel token already uses), so the phone pairs by opening a link rather than by typing into an endpoint the whole internet can also type into.
4. **`CorsLayer::permissive()` — removed outright, 2026-09-09.** Verdict after the review this item scheduled: the layer was deleted from `build_router` and `tower-http` dropped from `src-tauri/Cargo.toml` (it was the crate's only use site). It protected nobody and opened one real path. `/ws` is a WebSocket handshake, exempt from CORS by browser spec, so the layer never governed it. `/pair` is called same-origin only (`src/services/bridge.js:364` builds the URL from `window.location.origin`) and its token lives in `localStorage`, not a cookie — so permissive CORS bought the legitimate caller nothing while letting *any* web page the owner's browser happens to open script a 6-digit guessing run against the host. Removing it is rung **B3.1 — impossible by shape** (`proportion`): with no CORS headers the browser refuses the cross-origin read itself, with no code to maintain. Non-browser clients (curl, Postman) were never constrained by CORS and are still gated by the pairing code and the per-source cooling window from item 2.
   **Not covered here, deliberately:** DNS rebinding walks around CORS entirely (attacker.com rebinds to the LAN IP, the browser then treats the request as same-origin). The control for that class is a `Host`/`Origin` allowlist inside `pair_handler` — a different mechanism, not a gap in this one. Reopen if a rebinding attempt is ever observed or the public ingress mode ships to a hostname resolvable from outside the tailnet.

Existing tests in this file already assert close-code distinctness and host-token shape; W2 adds the equivalent: a bad-code storm must leave `enabled` true, and a paired device's token must still open a socket afterwards. Both are pure state assertions on `RelayState` — no live phone, matching the file's existing test style.

**Reopen trigger** (`proportion.C1`): if pairing ever grants anything beyond mirroring this one Mac session — a second user, a write path that is not already in the frozen protocol — the blast-radius row is wrong and this verdict is re-run.

## 5. W3 — the ingress mode

Rust + JS. Storage rides the existing app-data dir (`~/.aki/devsync/`, `app_paths.rs`), alongside `companion-server.json`, which already persists the remote on/off decision.

1. **Stored shape** — mode plus the origin it needs: `{ "mode": "tailscale" | "public" | "cloudflared", "origin": "https://…", … }`. New fields on a persisted struct carry `#[serde(default)]` (`CLAUDE.md` GLOBAL TAURI STACK) so an existing install loads as `tailscale` without a migration.
2. **Two commands**, both `async fn` + `spawn_blocking` for the file I/O and any CLI call (`CLAUDE.md` NEVER BLOCK THE UI), both registered in `lib.rs` **and** granted in `src-tauri/capabilities/default.json` — a missing grant is a silent no-op, which is the failure mode this stack has hit before.
3. **`get_companion_url` gains a third kind.** `lan` and `tailscale` stay derived from interfaces; `public` is read from storage and appended. The UI already renders the list generically (`AppHeader.vue:94-104`), so the kind label is the only presentational change.
4. **The companion needs nothing.** `bridge.js` is already origin-relative (`wss://<host>/ws` from an https page) — the property that made the Tailscale HTTPS mode work is exactly what makes a Cloudflare origin work. One consequence to surface in the UI, not to fix: **the device token is per-origin**, so moving to a new hostname costs one re-pair per device.
5. **In `public` mode the app touches Tailscale not at all** — no status probe, no serve command, and the modal shows the active origin instead of Tailscale controls (`aki-mcp-sv`'s panel does the same, and for the same reason: a Tailscale check reported as failing to a user who deliberately runs their own edge is a false alarm).

## 6. W4 — Remote Control settings modal

Vue, `src/components/modals/`, following the existing modal shape (`BaseModal.vue` + siblings). Holds: ingress mode picker, the public origin field, paired-device list with revoke (`list_paired_devices` / `revoke_device` are already implemented and have no UI — this is where they finally land), the HTTPS/Tailscale row, and whatever W2 exposes about a pairing lock.

The AppHeader dropdown keeps exactly what it has today minus the HTTPS row: on/off, pair code, URL rows, plus one entry point into the modal. That is a net **reduction** in dropdown rows, which is how this phase satisfies Extreme Narrow rather than straining it.

## 7. Files this touches

| File | Phase | What |
| :-- | :-- | :-- |
| `src-tauri/src/web_server.rs` | W1, W2, W3 | mount ownership, pairing gate, ingress storage + URL list |
| `src-tauri/src/lib.rs` | W3 | register the new commands |
| `src-tauri/capabilities/default.json` | W3 | grant them (silent no-op if forgotten) |
| `src/composables/useRemoteControl.js` | W1, W3 | foreign-mount state, ingress mode |
| `src/components/AppHeader.vue` | W4 | drop the HTTPS row, add the modal entry |
| `src/components/modals/RemoteSettingsModal.vue` | W4 | new |
| `docs/feat/remote-control.md` | W1–W4 | its Tailscale section becomes one mode of an ingress selector; stamp rewritten in the same edit (`docs.A4`) |
| `README.md`, `src/components/modals/IntroModal.vue` | W4 | feature changed → both are checked in the same task (`CLAUDE.md`) |
| `scripts/tailscale-serve-https.sh` | W1 | still valid as a CLI fallback; its header gains the "only touches its own mount" caveat |

## 8. Verification — what settles each phase, and what genuinely needs the Mac

Per `coding.B3`/`B5`, each item names the cheapest tier that settles it.

| Phase | Settled by | Tier |
| :-- | :-- | :-- |
| W1 mount parsing | Unit test over recorded `serve status --json` fixtures — ours, foreign, empty. The parser is a pure function; a live tailnet proves nothing extra | `cargo test --lib` on the Mac |
| W1 no blanket command | `grep -n '\-\-https=443' src-tauri/src/` returns nothing outside the fallback script | static |
| W2 penalty shape | New `RelayState` tests: bad-code storm leaves `enabled` true; an existing device token still authenticates after a lock | `cargo test --lib` |
| W3 serde default | Load an existing `companion-server.json` with no ingress fields and assert the `tailscale` default | `cargo test --lib` |
| W3 capability grant | **Settled, no change needed.** `capabilities/default.json` holds only `core:*`/`opener:*`/`dialog:*` — not one of the ten pre-existing `web_server::*` commands has an entry, and they work. Tauri v2 gates plugin permissions, not commands listed in `generate_handler!` | static — read `capabilities/default.json` + `lib.rs:128-139` |
| **IPC wire names** | **Settled.** The Rust lane serializes `pairLinkToken` / `foreignTarget` / `suggestedOrigin` (camelCase, matching the existing `hostToken` / `pairedAt`); the frontend lane had written the snake_case forms, which would have failed **silently** — no error, the pair link simply never appears. Fixed on the frontend side (`useRemoteControl.js:40,63,95,211`). Command names and argument names (`mode`/`origin`, `enable`, `id`) already matched | static — grep both sides |
| W3 URL list | Reading the render path; the list is plain data and this project has no typecheck step (JS, no `tsc`) | static — done |
| W4 modal | **Done.** `npm run build` green, 11.25s, after the wire-name fix above | `npm run build` — done |
| **Rust structural review** | **Done, compiled.** `cargo check` clean and `cargo test --lib` passes 211/211 on the Mac, including every W1/W2/W3 test named above (mount ownership, pairing-lock, ingress defaults) — the borrow checker and trait resolution have now been exercised, superseding the earlier static-only read | `cargo check` + `cargo test --lib` — done |
| **Real edge behaviour** | Whether a Cloudflare tunnel holds the permanently-open `/ws` socket as well as the tailnet does, and whether a phone re-pairs cleanly on the new origin | **Runtime, Mac + phone — the one genuine hand-off.** Not derivable statically: `aki-mcp-sv`'s own reliability claim for its request/response traffic is recorded as unverified, and a long-lived socket is a different question again |

Rust unit tests: `cargo test --lib --manifest-path src-tauri/Cargo.toml` — run on the Mac, 211/211 passing. Compilation itself needs no separate step — `npm run tauri dev` builds the Rust side.

Read-only diagnostic for the current mount state, runnable any time: `scripts/check-tailscale-mount.sh` (reports who owns `<magicdns>:443/` and whether Funnel is on; changes nothing).

## 9. The edge decision — closed

Settled in a `/akithink` session, recorded in `docs/research/remote-ingress-shared-vs-separate.md`:

**Share the domain and the configuration model; separate the subdomain, the tunnel, and the process.**

```
mcp.<domain>      -> tunnel A -> 127.0.0.1:9999   aki-mcp-sv
devsync.<domain>  -> tunnel B -> 127.0.0.1:1421   Aki Dev Sync
```

Two consequences bind W3 and are not optional to it:

- **The vocabulary is ported verbatim**, not re-designed: mode names, the single resolved `origin`, and the precedence order all match `aki-mcp-sv`. A second configuration model is the actual cost this plan exists to avoid — a second DNS record is not.
- **The sibling's config is read as a hint, never a dependency.** `~/.aki/mcpsv/ingress.json` is read best-effort to prefill `devsync.<domain>` when it names `mcp.<domain>`; unreadable or unexpected shape means no suggestion and no error. Dev Sync must run identically on a machine where `aki-mcp-sv` was never installed.

Rejected, with reasons in the decision record: **one shared tunnel** (losing both endpoints at once removes the route by which either could be repaired, and it forces edits to a public multi-platform repo to serve a macOS-only app) and **one shared subdomain** (same origin means the browser stops isolating a remote-control credential store from an app that grants an internet-side LLM shell access to this machine — bought for one DNS record, and not recoverable by later configuration).

Separation does not forbid consolidation: anyone wanting a single tunnel runs it themselves and points both apps at it via `public` mode, with neither app aware. The reverse would foreclose separation, which is why the direction was chosen this way.

## 10. Mac hand-off

Everything below needs a real phone; the dev box has none paired (`CLAUDE.local.md` — build boundary). Nothing is committed yet; the whole change sits in the working tree. It has now been through the Rust compiler on the Mac (§8, `cargo check` + `cargo test --lib` 211/211) — only the phone/edge runtime protocol below remains open. Layer architecture: `docs/arch/remote-ingress.md`.

### 10.1 Owner test protocol — the runtime claims nothing static could settle

Run on the Mac with a real phone. Steps 1–3 need `aki-mcp-sv` running with its Funnel on; the rest do not.

| # | Do this | Pass looks like | Fail means |
| :-- | :-- | :-- | :-- |
| 1 | With `aki-mcp-sv`'s Funnel up, run `bash scripts/check-tailscale-mount.sh` | Reports the 443 `/` mount owned by `aki-mcp-sv` (`:9999`) | The mount reader is wrong; W1's whole safety rests on it |
| 2 | In the app, open Remote settings and try the Tailscale HTTPS toggle | It **refuses and names `aki-mcp-sv`**. It must not turn on | The app still steals a mount it does not own |
| 3 | Turn Remote Control off entirely, then re-check `aki-mcp-sv`'s public URL in a browser | `aki-mcp-sv` still answers | **The original bug is not fixed** — this is the one step the whole plan exists for |
| 4 | Stop `aki-mcp-sv`. Turn Remote Control on, enable Tailscale HTTPS, open the tailnet URL on the phone, pair with the 6-digit code | Pairs, mirrors, terminal works | Regression in the path that already worked |
| 5 | In Remote settings, copy the pair link. Open it on the phone in a fresh tab | Pairs with **no code typed**; then press Back — the URL no longer contains `?pair=` | If the link shows empty, the `pairLinkToken` wire name broke again; if `?pair=` survives Back, the secret is sitting in the phone's history |
| 6 | On the phone, type a wrong pairing code ~12 times from a fresh device | Pairing gets refused with a retry-after; **the Mac's Remote Control stays ON**, and the phone from step 4 keeps working the whole time | The kill switch is still live — do not ship |
| 7 | Switch ingress mode to `public`, enter `https://devsync.<domain>/`, save. Point a `cloudflared` tunnel at `127.0.0.1:1421`. Open that origin on the phone over mobile data (Wi-Fi off) | Pairs and mirrors; the terminal stays live for several minutes without dropping | The `/ws` socket does not survive the Cloudflare edge — the one thing `aki-mcp-sv`'s own docs could not answer, since it only ever ran request/response traffic through it |
| 8 | Toggle Remote Control off and on twice, then quit and relaunch the app | Ingress mode and origin are still what you set in step 7 | `persist_server_state` is dropping fields — the exact bug the old `{"enabled": …}` writer would have caused |
| 9 | In Remote settings, revoke the phone from step 4 | It disappears from the list, and that phone must pair again on next connect | The revoke path was never wired to the UI before this change, so it has never been exercised |
