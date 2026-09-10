# Remote Control ingress — the Tailscale collision with aki-mcp-sv, and what a swappable edge would cost

**Start time:** 2026-08-22

## Initial purpose

Owner report, 2026-08-22. Since building `aki-mcp-sv` (`~/aki/Nodejs/aki-mcp-sv` — an MCP server that gives claude.ai/ChatGPT/Grok access to this machine over HTTPS), Tailscale Funnel proved awkward there, so that project moved to its own domain behind a Cloudflare tunnel. Aki Dev Sync's Remote Control has gone unused for a while; on returning to it the owner found the two **conflict**, and tested it: they share one Tailscale DNS name and appear to fight over the same public entry.

Two criteria, stated by the owner and treated here as fixed requirements, not options to re-litigate:

1. Remote Control must be usable **without depending on Tailscale**, while **keeping Tailscale as one option** — plausibly needing its own settings modal, since the feature now has more than a toggle's worth of configuration.
2. When `aki-mcp-sv` is running the two must either **share the DNS name coherently or stay cleanly separate** — the problem the owner names is not a missing feature but *too many possible arrangements*, with no decision recorded.

Security is explicitly in scope but not a blocker in the owner's framing: exposure is already key-gated, and `aki-mcp-sv`'s public posture (one port, specific routes) is the model to copy. This doc tests that assumption rather than accepting it — see F4.

This is a research doc: it establishes what the collision actually is, what the option space really contains, and what each option costs. No code was changed and no plan was written.

## Strategy

Read both codebases' ingress paths against the same question — *what does this write into the shared Tailscale serve config, and what does it assume about the origin?* — then read `aki-mcp-sv`'s already-shipped answer (it solved this exact problem in v1.8.0) rather than designing a new one. Finally, size Dev Sync's authentication against public exposure, since criterion 1 moves it from a tailnet to the open internet.

## Checklist

1. Read `src-tauri/src/web_server.rs`'s Tailscale block — enable, disable, and status detection.
2. Read `~/aki/Nodejs/aki-mcp-sv/scripts/tailscale.js` + `start.js` — what it writes and how it detects.
3. Establish from the installed `tailscale` CLI (1.92.3) whether `serve` and `funnel` share one config and one mount point.
4. Read `aki-mcp-sv`'s ingress precedence ladder and its plan doc (`docs/plan/done/cloudflare-tunnel-ingress.md`).
5. Read Dev Sync's pairing/auth gate (`/pair`, `/ws`, the strike counter) and judge it against an internet-reachable origin.
6. Establish where a non-Tailscale origin would have to enter Dev Sync's own code (URL discovery, UI, persistence).

## Result

### F1 — the collision is one mount point, and Dev Sync's OFF switch is the destructive half

Both apps write into the **same per-node Tailscale serve config**. `tailscale serve` and `tailscale funnel` are two front-ends over one config: both default to `--https 443`, both mount at path `/` unless given `--set-path`, and `funnel` differs only by additionally setting `AllowFunnel` for that port (verified against the installed CLI's own help, 1.92.3).

- `aki-mcp-sv` runs `tailscale funnel --bg 9999` (`scripts/tailscale.js:38`) → `https://<magicdns>:443/` → `127.0.0.1:9999`, public.
- Aki Dev Sync runs `tailscale serve --bg http://127.0.0.1:1421` (`src-tauri/src/web_server.rs:1013-1017`) → `https://<magicdns>:443/` → `127.0.0.1:1421`, tailnet-only.

One host, one port, one path — so whichever ran last owns `/`, and the other silently stops being reachable at the URL it printed. That is the conflict the owner observed, and it is symmetric: neither app is misbehaving on its own terms.

**The asymmetric part is the teardown.** Dev Sync disables with `tailscale serve --https=443 off` (`web_server.rs:1017`), which clears the whole 443 handler set on that node rather than only the handler Dev Sync installed — and `useRemoteControl.js:79-83` fires it automatically whenever Remote Control is switched off. So turning Dev Sync's remote off can take `aki-mcp-sv`'s public endpoint down with it, with no message in either app. Dev Sync's own detection is honest but blind to this: `tailscale_serve_on()` (`web_server.rs:978-983`) just greps `serve status` text for `127.0.0.1:1421`, so while `aki-mcp-sv` owns `/` Dev Sync correctly reports HTTPS "off" — and enabling it then quietly steals the mount.

Nothing here needs a running Mac to establish: the two command strings and the CLI's documented mount semantics settle it (`coding.B3`). What *is* unverified is one sub-detail — whether `serve --bg` also clears `AllowFunnel` for 443 or only replaces the proxy target (i.e. whether the stolen mount stays publicly reachable or becomes tailnet-only). One command on the Mac decides it: `tailscale funnel status --json` before and after. It changes the blast-radius wording, not the conclusion.

**Coexistence on Tailscale is technically possible and not recommended:** `--set-path` mounts one app under a sub-path of the same hostname. It fails for Dev Sync in particular, because the companion is a SPA whose assets, `/ws` and `/pair` are all served origin-relative from `/` (`build_router`, `web_server.rs:452-470`; `bridge.js` builds `wss://<host>/ws`). Serving it under `/devsync` would need a base-path-aware build *and* a prefix-stripping router — real work, to buy a shared hostname the owner has not asked for. Also worth knowing: Funnel can only use ports 443/8443/10000, so "just use another port" is not available on that edge.

### F2 — `aki-mcp-sv` already shipped the shape criterion 1 asks for; it is portable, not novel

`scripts/start.js:30-73` resolves **one value** — `origin` — through a precedence ladder, and nothing downstream knows which edge produced it:

`--tunnel <cred.json> --origin <host>` (app spawns `cloudflared`) > `PUBLIC_ORIGIN` env (bring your own edge) > saved panel choice (`~/.aki/mcpsv/ingress.json`) > Tailscale Funnel (default).

Its plan doc (`docs/plan/done/cloudflare-tunnel-ingress.md`) states the property that makes this cheap: *"ingress is a swappable edge"* — OAuth, gatekeeper and bridge were unchanged when Cloudflare was added, because they only ever read `origin`. The trade it records honestly: Funnel buys zero-config and pays in reliability; a named Cloudflare tunnel buys a stable hostname and pays in setup (account + domain + `cloudflared login` + credentials JSON).

Dev Sync's equivalent of `origin` does not exist yet. Today the URL list is **derived from network interfaces only** (`get_companion_url`, `web_server.rs:913-930`): it enumerates addresses and labels them `lan` or `tailscale` by IP range, and there is no place for an origin the machine cannot see on an interface. That is the one structural gap: a public HTTPS origin is a **configured fact, not a discoverable one**, so it has to be stored (the `~/.aki/devsync/` dir and `companion-server.json` already exist for exactly this class of state) and merged into that list as a third kind. The Tailscale toggle then stops being a feature and becomes one mode of an ingress selector — which is also what makes a dedicated settings modal (criterion 1) the right container rather than more rows in the AppHeader dropdown, where the Extreme Narrow rule (`CLAUDE.md`) is already straining at three.

### F3 — the DNS-name question has three arrangements, and only one needs no new coupling

Ranked by what they cost, given `aki-mcp-sv` spawns its own single-service `cloudflared tunnel run --url http://127.0.0.1:9999` (no config file, no multi-ingress rules):

| Arrangement | What it needs | Verdict |
| :-- | :-- | :-- |
| **Separate hostname, separate tunnel** — Dev Sync gets its own subdomain + its own credentials JSON, forwarding to `127.0.0.1:1421` | A second DNS route on the same Cloudflare account; each app keeps spawning its own `cloudflared` | **Recommended.** Zero coupling: either app may run alone, in any order, with no shared state and nothing to arbitrate. Costs one DNS record. |
| **Separate hostname, one shared tunnel** — one `cloudflared` with a config file carrying two ingress rules | Neither app may spawn `cloudflared`; both must run in "external edge" mode (Dev Sync: the `PUBLIC_ORIGIN` equivalent), and something outside both must own the tunnel's lifecycle | Viable and tidier operationally, but it makes each app's remote depend on a process neither owns — a new failure mode to explain when it is down. |
| **Same hostname, split by path** | Base-path-aware SPA + prefix-stripping router in Dev Sync (F1) | Rejected: most work, most fragile, no benefit the first row does not already give. |

The owner's "quá nhiều hướng lựa chọn" is therefore answerable: the choice collapses to row 1 unless a reason appears to consolidate tunnels, and row 2 stays available later without redesign because both apps would already be in bring-your-own-origin mode.

Tailscale is not removed in any row. It stays as the mode that needs no domain — and, once the ingress is a selector rather than a hard-wired toggle, Dev Sync gains the ability to say *"Tailscale mount is owned by something else"* instead of silently taking it.

### F4 — before the origin is public, one existing guard becomes a remote kill switch

Dev Sync's gate is: a 6-digit pairing code minted per enable, exchanged at `POST /pair` for a per-device token; `/ws` requires that token; `role=host` additionally requires a per-process 128-bit secret handed only to the Tauri webview. Against a LAN or a tailnet, that is a sound and well-argued design (`docs/feat/remote-control.md` § Security).

Public exposure changes the value of exactly one part of it. `pair_handler` (`web_server.rs:768-782`) counts bad codes in a **single global counter**, and at `MAX_PAIR_FAILURES = 10` it does not throttle the caller — it **switches Remote Control off and persists that off**:

```rust
state.enabled.store(false, Ordering::SeqCst);
state.pairing_code.lock()…clear();
relay().persist_enabled(false)
```

Correct as anti-brute-force on a trusted network, where the only people who can reach the endpoint are already inside. On an internet-reachable origin it inverts: any unauthenticated stranger — a background scanner, not a targeted attacker — can post ten wrong codes and permanently disable the phone's access until someone walks to the Mac and turns it back on. The lockout survives a restart by design, so waiting does not heal it. This is a **denial of service reachable by anyone who learns the hostname**, and it is the one thing that must change *before* the public mode ships, not after.

Two other items are lower but worth stating in the same breath: the 6-digit code's 10⁶ space is only safe *because* of that strike counter, so any relaxation of the counter must come with a longer secret for the public path (`aki-mcp-sv`'s panel-token/OAuth pattern is the local precedent); and `CorsLayer::permissive()` (`web_server.rs:454`) is harmless behind a token on a LAN and is a wider posture than it needs on a public origin.

The owner's "học cách bảo mật của aki-mcp-sv (chỉ cấp cổng nhất định, route nhất định)" holds at the edge — a tunnel that forwards only `127.0.0.1:1421` is exactly right — but it does not carry over as protection here, because Dev Sync legitimately needs *all* of its routes (`/`, `/pair`, `/ws`, static assets) exposed to the phone. Route-narrowing at the edge cannot substitute for fixing F4; a Cloudflare Access policy in front is an optional second gate, not a replacement either.

## Verification

Static reading of both trees, plus the installed `tailscale` 1.92.3 CLI's own help output for the serve/funnel mount semantics in F1. Command strings, mount points, the precedence ladder, the URL-discovery gap and the strike counter are all read directly from source and are quoted above with their locations.

Not established here, and deliberately so:

- Whether `serve --bg` clears `AllowFunnel` (F1) — one `tailscale funnel status --json` on the Mac, before and after.
- Which hostname and tunnel `aki-mcp-sv` currently uses on the Mac: `~/.aki/mcpsv/ingress.json` does not exist on this dev box, so the concrete names in F3 are the owner's to fill in.
- Whether a Cloudflare edge actually holds a long-lived WebSocket (Dev Sync's `/ws` is permanently open, unlike `aki-mcp-sv`'s request/response traffic) better than Funnel. `aki-mcp-sv`'s own plan doc marks the reliability claim unverified for its own workload; for a persistent socket it is a different question again, and the honest answer is that only running it settles it.

Corroborating links:

- `docs/feat/remote-control.md` — the shipped security model and the Tailscale HTTPS toggle this doc re-examines.
- `docs/plan/done/remote-control.md` §7.1a — why URLs are discovered from interfaces, which is the assumption F2 breaks.
- `~/aki/Nodejs/aki-mcp-sv/docs/plan/done/cloudflare-tunnel-ingress.md` — the shipped precedent, including the zero-config trade it names.
- `~/aki/Nodejs/aki-mcp-sv/README.md` § Exposing to the internet — the user-facing shape of the three-mode picker.

## Decision

**Follow-up work**, in an order set by what is destructive rather than by what is visible:

1. **Stop the cross-app teardown** (small, independent of everything else). Dev Sync must never run a blanket `serve --https=443 off`: read `serve status --json`, and disable only the handler whose proxy target is `127.0.0.1:1421`; if something else owns the mount, leave it alone and say so in the UI. This is worth doing even if no other item ships, and it is the only item that fixes damage the app can cause today.
2. **Introduce an ingress mode** — `tailscale` (current behaviour) | `public origin` (owner runs the edge; Dev Sync only stores and displays it) | optionally `cloudflared` (Dev Sync spawns it, mirroring `aki-mcp-sv`). One stored value resolving to one origin, merged into `get_companion_url`'s list as a new kind. Port the precedence ladder rather than inventing one.
3. **Fix F4 before any public mode is reachable.** The strike counter must throttle rather than self-disable — and if a public origin is enabled, the pairing secret must be longer than six digits on that path. These two ship *together with* item 2 or item 2 does not ship.
4. **A Remote Control settings modal** as the container for the above; the AppHeader dropdown stays as the on/off + code + URL summary it is today.

**Decision recorded for criterion 2:** separate hostname, separate tunnel (F3 row 1), unless the owner says the second Cloudflare DNS record is the objection — in which case row 2 becomes the fallback and item 2's "public origin" mode is what makes it work.

Cross-references: `docs/feat/remote-control.md` (must be updated by items 1–2, since it documents the current toggle as the whole story); `docs/plan/backlog.md` (not yet amended). No plan doc exists for this yet — items 1–4 are scoped here but not scheduled.
