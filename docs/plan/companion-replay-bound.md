# Plan: Bound the companion scrollback-replay burst after unlimited tabs

/ akithink self-run (Opus 4.8, ~6 rounds). This records the decision for the one item left open by the terminal-stack / unlimited-tabs work and the audit-hardening batch. It is a separate concern from `docs/plan/audit-hardening.md`, not a P2 sub-item.

## 1. The real goal (goal excavation)

The stated task in the older `joyful-petting-quill` plan was "replace bulk `pushAllScrollbacks()` with per-tab/on-demand hydration". Climbing the goal chain shows that is a *mechanism*, not the goal:

per-tab hydration → so the reconnect burst does not grow with tab count → so a phone companion reconnects reliably **at any tab count** → so the "unlimited tabs" UX is honestly safe on the phone too, not only on the Mac host → **UX-supremacy: opening as many terminals as the user wants must never silently break the remote-control session.**

**Goal (one sentence):** the companion reconnect/resync replay must never exceed the outbox budget regardless of how many tabs are open, and must fail loudly rather than loop silently.

## 2. Why this is a real task (not the audit's "accepted residual")

The audit downgraded finding A9 (unlimited tabs) to "accepted residual, no task", but A9 was about **backend** resource safety (threads/memory per tab). This is a **different axis**: the companion **replay transport**.

- `reset` frames are undroppable state frames — only terminal output may be coalesced (`web_server.rs` test `only_terminal_output_may_be_coalesced`), and a backlog of undroppable frames closes the connection (`a_backlog_of_undroppable_frames_closes_the_connection_instead_of_growing`).
- `src/services/ptyBridge.js` `pushScrollbacks` sends one `reset` frame per tab; called on companion connect (targeted via `frame.id`) and on congestion resync (`scheduleResync`, **broadcast to every companion**). Pre-fix (when named `pushAllScrollbacks`) it replayed *full* scrollback for **every** tab — the unbounded burst this plan targets; §6 records the as-built bounding.
- The old invariant R1/R2 (`docs/plan/done/1.21.1-terminal-limits-and-structure.md`) sized `MAX_TABS = 16` to fit the ~8 MiB `COMPANION_QUEUE_LIMIT_BYTES` with headroom. That invariant was the **only** guard on this burst, and removing `MAX_TABS` **voided it**. At `SCROLLBACK_CAP = 128 KiB` (~170 KB base64/full tab), ~24–49 near-full tabs breach the budget.

Failure mode when breached: the replay closes the connection mid-burst → phone reconnects → replays again → **silent reconnect loop**, no error surfaced. That is exactly the silent-no-op class this codebase forbids, and it violates UX-supremacy.

## 3. Decision — bound the burst, do NOT build a request/response protocol (yet)

The minimal sufficient control is **not** on-demand hydration. Inversion shows the thing that breaks reconnect-at-scale is *one unbounded burst of undroppable frames*, so the fix is to bound the burst by construction:

1. On connect/resync, hydrate **full scrollback only for the active (and a small MRU set of) tab(s)**, up to a byte budget that is a fraction of `COMPANION_QUEUE_LIMIT_BYTES`.
2. For every other tab, send the **empty-but-authoritative** `reset` frame the code already supports (`data || ''` at `ptyBridge.js:27` — the comment already notes "Empty reset still delivers authoritative size/liveness"). Size/liveness/tab-existence still land; only the historical bytes are deferred.
3. A cold tab hydrates its scrollback when it is first viewed on the companion — reuse the existing frame shape and the existing per-tab `pty_get_scrollback` path; add a companion→host request only if step 2 proves insufficient, not upfront.
4. Fix the `scheduleResync` **broadcast** amplification: a congested single companion must not trigger a full-replay burst to every companion.
5. Re-establish an invariant that holds for **any** tab count (burst ≤ budget by construction), replacing the deleted `MAX_TABS`-based R1/R2 — a test that reasserts it is the durable guard.

**Rejected — full per-tab on-demand hydration protocol now (new frame types, `COMPANION_ALLOWED_COMMANDS` entry, phone request-on-focus).** It is a medium protocol change on the delicate remote-control transport the audit flagged; it adds tab-switch latency and a new "tab never hydrates if the focus frame drops" failure; and it defends a scenario (24+ full tabs) nobody has measured. Reopen it only if step 2's empty-reset approach cannot deliver acceptable phone UX.

## 4. Sequencing & scope

- Do this **after** P2 lands and `audit-hardening.md` closes — the hydration touches `ptyBridge.js` and may touch `pty.rs`/`web_server.rs`, which the in-flight P2 agent is editing; running both now risks a file conflict.
- Keep `SCROLLBACK_CAP`, backend ring limits, and the 8 MiB outbox backpressure untouched — they bound individual resources, not tab count.
- One focused batch, its own tests; no UI redesign bundled.

## 5. Completion criteria

- A test simulates a large tab count reconnecting and proves the replay burst stays ≤ the budget and the connection is not closed.
- Congestion resync no longer broadcasts a full replay to uninvolved companions.
- A cold tab's scrollback still arrives when the user views it on the phone.
- Reopen trigger for the rejected full-protocol option: empty-reset hydration proves insufficient for real phone UX, or a measured case of the reconnect loop.

## 6. As-built (working tree, not committed)

Status of the §5 criteria as actually implemented. Verified: `npm run test:replay` → 9/9 pass; `npm run build` → green.

- **Burst bounded — done, as a tab-count cap (not a byte budget).** `pushScrollbacks` (`src/services/ptyBridge.js:21`) hydrates full scrollback for at most `REPLAY_SCROLLBACK_TAB_LIMIT = 4` tabs; every other tab still gets the authoritative empty `reset` (size/liveness/existence). This deviates from §3 step 1 (a fraction of `COMPANION_QUEUE_LIMIT_BYTES`): 4 × ~170 KB ≈ ~680 KB is well under the ~8 MiB budget, so a fixed count is a deliberately conservative proxy that avoids per-frame byte accounting. Priority is **active → pinned → list order to fill**; `pty_list_tabs` is id-sorted (not recency), so a prefix would strand the active tab history-less.
- **Selection extracted + unit-tested — done (selection layer only).** The pure selection is `selectHydratedTabs` in `src/services/replayHydration.js`, guarded by `scripts/tests/replay-hydration.test.mjs` (`npm run test:replay`, node's built-in runner, 9 cases). It proves that for any tab count the hydrated set is ≤ the limit and always contains the active tab, plus pinned/fill ordering and dedupe. **Scope honesty:** this tests the *selection*, not the transport — it does not exercise frame encoding, the outbox byte limit, or the connection-close path. §5's "connection is not closed" holds *by construction* (bounded tab count ⇒ bounded burst), not by a direct transport assertion.
- **Resync broadcast — resolved by design; not a code gap.** Two triggers exist. Connect/relay-requested replay is targeted (`pushScrollbacks(frame.id)`, `ptyBridge.js:111`; `scheduleResync(to)` threads the connection key). The congestion trigger — `if (!send(frame)) scheduleResync()` at `ptyBridge.js:92` — is intentionally untargeted: congestion is a **single shared host→relay uplink** signal (`bridge.js` `isSocketCongested` via `ws.bufferedAmount`), and the refused frame was a broadcast `pty_output` that *every* companion missed, so healing all of them is correct — not amplification to "uninvolved" companions. It is safe because (a) the burst is bounded to ≤ the cap and (b) `scheduleResync` pushes only once the uplink buffer has drained (`!isSocketCongested()`), re-arming until then, so it never piles a replay onto a congested socket. Scoping the heal to the dropped tab would *under-heal* (only one resync is scheduled even if several tabs dropped during the congestion window) → silent missing bytes, a regression. True per-companion scoping would need the relay to signal *which* connection is behind — exactly the §3-rejected protocol change — so it stays deferred by choice, not a bug. Hardened `pushScrollbacks` to `to: to ?? undefined` (was `||`) to match the `frame.id ?? null` connect site so a falsy connection key can't silently fall back to broadcast.
- **Cold tab hydrates on first phone view — not addressed here.** Still relies on the existing per-tab `pty_get_scrollback` path; no companion-side on-view refetch was added or verified. Open.
- **Stale note:** §4's "do this after P2 lands" caution is moot — the `pushScrollbacks` rework has already landed in the working tree.
