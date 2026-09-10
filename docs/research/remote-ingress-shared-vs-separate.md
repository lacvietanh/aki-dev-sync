# Sharing an ingress with aki-mcp-sv — how far to share, and where sharing starts costing

**Start time:** 2026-08-22

## Initial purpose

Immediately after `docs/research/remote-ingress-tailscale-conflict.md` established *that* Aki Dev Sync's Remote Control and the sibling app `aki-mcp-sv` collide on one Tailscale mount, the owner asked the question that doc deliberately left open, and asked it as a value question rather than a technical one:

> If the two apps shared one way in, wouldn't that be genuinely better — one place to set up, one model to remember, for me and for anyone using one or both? A coherent ecosystem instead of two tools with scattered settings I can't keep in my head?

Recorded as a `/akithink` session because the shape being chosen is close to one-way (`think.A1`): two apps cabled together at the infrastructure layer cannot be separated again without changing both repos, one of which is public and multi-platform.

Context at the time: Dev Sync remote had gone unused for months; on returning to it the owner found it broken and could not reconstruct why. That experience — *the tool failed at the moment it was needed, and the reason was unrecallable* — is the real motivation and outranks any aesthetic preference for consolidation.

## Strategy

Run the five-phase protocol with `METHOD-deep-think.md` as the toolbox, and treat the owner's own preference as an option requiring attack rather than a conclusion to justify (`think.B3` anti-sycophancy). Size the verdict by blast radius rather than by frequency of annoyance (`proportion.B1`), because the asset being protected is the ability to reach the machine while away from it.

## Checklist

1. Split the request into its two independent axes before analysing either.
2. Excavate the goal chain to the ultimate goal; name conflicting goals rather than picking one silently.
3. Separate verified facts from real constraints from assumptions, and attack the assumption the whole question rests on.
4. Run all five critique lenses on **both** options, including the one already favoured.
5. Enumerate the consolidated model's mechanics concretely enough to price it — including the variant the owner raised last (one shared subdomain).
6. Converge into a decision record with the reopen triggers.

## Result

### R1 — the request contains two independent axes, and conflating them is what made it feel hard

- **(A) Shared infrastructure** — one tunnel, one hostname, one `cloudflared` process. A network-topology question.
- **(B) Shared configuration model** — same vocabulary, same storage convention, same precedence ladder, same UI language. A DX/UX question.

Everything the owner described wanting — not memorising two worlds, settings that do not feel scattered, an ecosystem rather than two tools — lives in **(B)**, and (B) is achievable **without** (A). The reverse does not hold: shared infrastructure with divergent configuration models is exactly as hard to remember as today, plus a resource the two apps now contend for.

### R2 — the goal chain, and where it conflicts with itself

immediate: use Remote Control without depending on Tailscale, and without the two apps fighting → intermediate: someone who already set up `aki-mcp-sv` can enable Dev Sync without learning a second world; the owner does not have to hold two models in memory → **ultimate: when away from the Mac and it is needed, the tool works — rather than becoming a setup project.** A remote that must be remembered to be switched on is absent precisely when it matters most.

Two genuine conflicts, neither resolved by preference:

- **Consolidate** vs **each app must run alone.** Anyone using only one of the two must not pay for the other. Trading "two things to remember" for "one thing to remember plus one implicit dependency" is a bad trade — an implicit dependency costs far more when it breaks than a remembered fact costs while it holds.
- **Dependency direction is not symmetric.** `aki-mcp-sv` runs on Windows/Linux/macOS and is a public repo with outside users; Dev Sync is macOS-only. So Dev Sync may never be a precondition of `aki-mcp-sv`. Any consolidation must be one-directional: Dev Sync adopts `aki-mcp-sv`'s model, never the reverse.

### R3 — the assumption the question rested on is contradicted by the evidence that produced the question

The intuition reads *"this broke because the two are separate."* The incident says the opposite: it broke **because they were already sharing** — one Tailscale 443 mount, used by both, **owned by neither**. The pain is not insufficient consolidation; it is consolidation with no declared owner.

That reframes what is actually scarce. The hostname is not what anyone has to remember — it lives in a Cloudflare dashboard, not in a head. The **model** is what has to be remembered: what the modes are called, where the value is stored, which source wins, how to turn it off. One extra DNS record costs approximately zero recall; a second configuration model costs a great deal.

### R4 — critique, both directions

**Steelman for consolidating:** one `cloudflared` is one place to log, restart, and upgrade; multi-hostname on a single tunnel is a first-class Cloudflare feature, not a hack; forcing someone with a working tunnel to create a second one genuinely is friction.

**Attack on the favoured option:** "shared model" is a promise with **no enforcement mechanism** — two repos, two languages, no shared package. It will drift: a fourth mode or a renamed field in `aki-mcp-sv` and Dev Sync's suggestion silently goes wrong. The only shape that survives this is Dev Sync reading the sibling's config **as a hint only**, failing silent, never depending on it to function.

**Inversion — to guarantee failure:** let two apps write one global resource with no owner · let A's teardown erase B's config · make A require B to be running · make the user remember a start order. The first two were **already happening**; both belong to the *shared-without-an-owner* family. That is direct evidence against deepening the sharing before ownership is fixed.

**Pre-mortem, consolidated:** a `config.yml` typo while adding a third project, or an `aki-mcp-sv` upgrade that resumes spawning its own tunnel and contends for the same tunnel ID — and **MCP and Remote Control die together, while away from the machine.** Losing both at once means losing the route by which either could be repaired.
**Pre-mortem, separate:** six months on, two DNS records and a moment's confusion about which is which. Cost: minutes. **No loss of access.** These are not the same class, and irreversibility outranks frequency.

**Second-order:** consolidating at the app layer *requires editing `aki-mcp-sv`* (teaching it not to spawn its own tunnel) — pushing complexity into a public, multi-platform repo to serve a macOS-only app, and charging its outside users for a feature they do not have. Separating costs that repo nothing. And the felt sense of "one ecosystem" is carried by vocabulary, documentation, and a prefilled field — not by topology: `mcp.<domain>` + `devsync.<domain>` reads *more* like one system than one hostname carrying two paths.

### R5 — the shared-subdomain variant, priced separately

Raised last by the owner: one hostname split by path (`aki.<domain>/mcp`, `aki.<domain>/devsync`). Cloudflare routes it; the cost is not routing.

**The browser's isolation boundary is the origin — scheme + host + port. Path is not part of it.** One subdomain means one origin, therefore shared `localStorage`, cookies, service-worker scope, and CSP. Dev Sync's device token (`localStorage['aki-companion-device-token']`) becomes readable by anything running on the sibling's pages, and vice versa — and the sibling is an application that deliberately grants an internet-side LLM shell and filesystem access to this machine. That merges two attack surfaces which are currently, and freely, separated by the browser.

The engineering cost is real but secondary: every path in the companion is absolute from `/` today — `manifest.webmanifest` (`start_url`/`scope`/icons), `index.html`'s icon and manifest links, `main.js`'s `register("/sw.js")`, Vite's default `base`, and `bridge.js`'s WebSocket URL — and `cloudflared` does not strip the prefix before forwarding, so the axum router would have to be nested too. All of it must simultaneously keep working at LAN root (`http://192.168.x.x:1421/`), which forces the prefix to be dynamic at runtime rather than fixed at build. Every one of those failure modes is silent: a blank page or a PWA that will not install, with no error.

**What it buys over shared-domain-two-subdomains: one DNS record.** Paying for that with the isolation boundary between a remote-control credential store and an LLM-driven shell is not a trade — and unlike a topology choice, it is not recoverable by later configuration.

## Verification

This is a decision record, not a measurement. What it rests on:

- **Verified by reading source** (in the predecessor doc, with file:line): the shared Tailscale mount and the blanket teardown; `aki-mcp-sv`'s shipped three-mode precedence ladder; Dev Sync's interface-only URL discovery; the pairing strike counter's persisted shutdown. **Verified in this doc:** every companion path is absolute from `/` — `public/manifest.webmanifest` (`start_url: "/"`, `scope: "/"`, `/icon.png`), `index.html:7-9`, `src/main.js:13` (`register("/sw.js")`), Vite's default `base`.
- **Not measured, and load-bearing nowhere:** whether a Cloudflare edge holds Dev Sync's permanently-open WebSocket better than Tailscale Funnel does. The verdict does not depend on it — it is an argument for keeping the edge swappable, which both options provide.
- **Estimated, labelled as such** (`proportion.A5`): the reach of an internet-exposed origin, and the likelihood of an upgrade-induced tunnel contention. Neither is observed; both are stated as estimates.

Corroborating links: `docs/research/remote-ingress-tailscale-conflict.md` (the evidence this reasons over) · `~/aki/Nodejs/aki-mcp-sv/docs/plan/done/cloudflare-tunnel-ingress.md` (the shipped precedent whose "ingress is a swappable edge" property is being ported) · `docs/feat/remote-control.md` (the security model the origin change re-prices).

## Decision

**Action.** Share the **domain** and the **configuration model**; separate the **subdomain**, the **tunnel**, and the process.

```
mcp.<domain>      → tunnel A → 127.0.0.1:9999   aki-mcp-sv
devsync.<domain>  → tunnel B → 127.0.0.1:1421   Aki Dev Sync
```

Three mechanisms, so "shared model" is a mechanism rather than a promise:

1. Dev Sync adopts `aki-mcp-sv`'s vocabulary and precedence ladder verbatim (`tailscale` / `public` / optional `cloudflared`, one resolved `origin`), stored under `~/.aki/devsync/` — inside the `~/.aki/` namespace both apps already share.
2. Dev Sync reads `~/.aki/mcpsv/ingress.json` **as a hint only**: seeing `mcp.example.com`, it prefills `devsync.example.com`. Unreadable or unexpected → silent, no suggestion. Never a dependency. A shared domain is what turns this from guesswork into inference, and it is where the felt "one ecosystem" actually comes from.
3. Ownership of any shared resource becomes explicit: the app disables only the Tailscale mount it installed, and says so when another process holds it.

Tailscale stays as the mode requiring no domain — the people arriving via `aki-mcp-sv`'s zero-config Funnel path often have no domain at all, and no tunnel option serves them.

**Rejected — one shared tunnel:** blast radius (losing both at once removes the route by which either is repaired), an ownership question with no cheap answer, forced edits to a public multi-platform repo to serve a macOS-only app, and two very different traffic profiles behind one edge. Note that separation does **not** forbid consolidation: anyone who wants one tunnel can run it themselves and point both apps at it in `public` mode, with neither app knowing. The reverse is not true — consolidating at the app layer forecloses separation. One direction preserves both options.

**Rejected — one shared subdomain:** it is the shared-tunnel option *plus* the loss of the browser's origin isolation between a remote-control credential store and an LLM-driven shell, bought for one DNS record. Not recoverable by later configuration, so it is not in the "try it and change later" class.

**Assumptions to monitor** (reopen triggers, `proportion.C1`): `aki-mcp-sv`'s `ingress.json` changing shape, which would make the suggestion silently wrong — check on any release of that project touching ingress · Cloudflare's handling of a long-lived WebSocket, still unmeasured · the appearance of a genuine shared reverse proxy for the whole Aki ecosystem, which would void this verdict rather than bend it.

**Cross-references:** `docs/plan/remote-ingress-rework.md` §9 (the open decision this closes; W3 carries mechanisms 1–2 and W1 carries mechanism 3) · `docs/feat/remote-control.md` (current-state doc, updated by that plan).
