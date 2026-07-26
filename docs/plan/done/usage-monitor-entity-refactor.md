# Usage monitors as independent entities — an OOP refactor

> **Status: DONE — shipped and tagged in `1.20.0`.** Static checks in §6 items 1-5 are clean
> (`usageSourcesStore` deleted, cache no longer parsed by hand, retarget-and-wipe path removed,
> `npm run lint:scripts` / `npx vite build` pass). **Still genuinely open** — this box cannot run the
> packaged app, and per `CHANGELOG.md`'s `[1.20.0]` "Not yet verified on a Mac" note no live
> two-machine test has been run: §6 items 6-12 (two hosts under two different Claude accounts
> rendering simultaneously, retargeting one slot leaving others untouched, the AG same-account
> collision case, per-monitor power toggle, one-SSH-round-trip-per-interval for co-targeted slots,
> reload/mirror persistence) remain unverified runtime behavior, not unfinished code. Base: the
> four-fixed-sources model shipped in `1.19.0`.
> Trigger: "tôi cần theo dõi cả claude remote host A và claude remote host B, mỗi host mỗi acc claude khác nhau."

## 0. The report, and what it actually is

The user asked to watch Claude Code on **two different remote hosts at once**, each signed into a
different Claude account. That is impossible today, and not because of a bug — because the *machine*
was never part of a monitor's identity.

| Layer | Keyed by | Consequence |
| :-- | :-- | :-- |
| `agent_usage.rs` (`get_agent_usage`, `host_lock`, `cc_auth_force_needed`) | `(agent, host)` — **correct** | Rust is stateless per call and already per-host. Nothing to fix. |
| `AgentUsageSection.vue` | `(agent, local\|remote)` — four hardcoded singletons | Exactly one remote AG monitor and one remote CC monitor can exist. |
| `sshStore.selectedSshHost` | one global ref | Both remote monitors read it. Retarget it and **both** jump hosts together. |
| AG cache (`aki-antigravity-usage-cache-v2`) | `email:sourceType`, with `host` as *metadata* | Same account on two hosts ⇒ the second write **overwrites** the first; the first host then fails its scope check and renders empty. |

So the missing thing is one dimension, not one line. The fix is to make the monitored machine part
of the entity's identity, and let everything else fall out of that.

---

## 1. Flow audit (`METHOD-flow-audit`)

**Flow target.** From "user wants to see agent A's quota on machine M" to "a number is on screen."

**Intended flow.** A monitor observes one agent on one machine, forever. A display slot chooses which
monitor to look at. Nothing else moves.

**Actual flow.** A monitor observes one agent on *whatever machine the global picker currently names*.
Changing the picker silently re-points every remote monitor, and each one reacts by **discarding its
data** (`useAgentUsage.js` `realHostChange` ⇒ `data.value = null`).

**Breakpoints.**
1. Two hosts cannot coexist — there is only one remote host in the model.
2. Switching hosts is destructive, so even *sequential* comparison of host A and host B loses A.
3. The AG cache's only per-host notion is an advisory `host` field on an entry keyed without it, so
   two hosts sharing one account collide.
4. `listAgAccounts()` dedups by email across the **whole** store and `delete`s the losers — with two
   hosts on one account it actively destroys the other host's entry. This is precisely the
   multi-entity blast-radius class the project's own regression guard was written for after 1.9.3.

**Artificial enforcement — guards that exist only because the shape is wrong.**
`lastNonNullHost`, the `realHostChange` computation, the "toggled off vs switched host" disambiguation,
the `!hostRef.value` early-returns scattered through `checkUsage` / `restartPollTimer` / `onWake` /
the manual-refresh watcher, and the provision/breaker reset block inside the host watcher. Every one
of them exists to cope with a monitor whose target can change underneath it.

**Root shape problem.** *Ownership.* The monitor does not own its target; an ambient global does.
Identity and configuration were conflated: "which agent on which machine" (identity, immutable) was
stored in the same place as "is it currently on" (configuration, mutable).

**Native-flow redesign.** Make the target immutable and part of the identity. A monitor is created
*for* a host and never retargeted. `enabled` becomes the only thing that varies. Then:
- two hosts coexist trivially — they are two objects;
- nothing is ever wiped, because nothing ever changes target;
- all the enforcement listed above **deletes**, replaced by one `enabled` check.

**Fastest validation.** Two slots, host A and host B, different accounts, both showing their own
numbers at the same time — and after retargeting one slot back and forth, the other slot's data never
blinks. Mac-side (§6).

---

## 2. The entity model

```
UsageMonitor            ── identity: agentId + host, immutable, id = `${agentId}@${host}`
  ├─ owns             data / loading / error / stale / isCached / cachedAt
  ├─ owns             poll timer, circuit breaker, provision state, wake subscription
  ├─ reads            enabled  (usageMonitorStore, keyed by this.id)
  ├─ reads            locked   (policy: CC-local is locked off in Proxy mode)
  └─ persists via     agUsageCache, scoped to this.host

UsageMonitorRegistry    ── multiton: id → UsageMonitor, created on first request, session-lived
                           two slots naming the same (agent, host) share ONE monitor → no double polling

UsageSlot               ── a VIEW onto a monitor. Owns only: which scope (local/remote),
                           which agent, and — new — which remote host IT looks at.
```

Two objects, one relation: **a slot points at a monitor; a monitor points at nothing.** That is the
whole design. Everything below is bookkeeping in service of it.

### Why a multiton and not one instance per slot

Polling cost is per `(agent, host)`, not per viewer. Slot C and slot D both showing `claudecode@hostA`
must produce one SSH round trip, not two — which is the property the current shared-source design
already has and must not lose. A registry keyed by identity preserves it by construction rather than
by a coordination rule someone has to remember (`design.A8`).

### Why the host moves onto the slot, not into a second global

`selectedSshHost` stays exactly where it is and keeps its current job — it is the SSH-config modal's
picker and the **default** for a slot that has never chosen. What changes is that the per-slot
`host-select-mini` dropdown, which already exists in the slot header, now writes the slot's own
target instead of the global. **Zero new DOM** — the Extreme Narrow rule is not touched.

### Naming (`design.A7`)

`UsageMonitor`, not `RemoteSource`: it is named for the role (something that watches a quota), not for
the concrete value it happened to have (remote-ness). `agentId@host` is the identity everywhere —
store key, registry key, cache scope, log tag — one spelling, one place, `design.A1`.

---

## 3. What each file becomes

| File | Change |
| :-- | :-- |
| `src/composables/agUsageCache.js` | **new** — the AG per-account cache, extracted whole out of `useAgentUsage.js` and made host-scoped (v3, §4). Extraction is forced, not cosmetic: `AgentUsageSlot.vue` currently reads `localStorage['aki-antigravity-usage-cache-v2']` and hand-parses it, reaching straight into another module's internals (`design.A6`). Now there is one owner and a narrow API. |
| `src/store/usageMonitorStore.js` | **new** — `monitorEnabled` map (`id → bool`) + `setMonitorEnabled(id, v)` action. Mirrored, because monitor on/off is a Mac setting (1.19.0's decision, unchanged). |
| `src/store/usageSlotStore.js` | **new** — `slotTargets` map (`slotId → {scope, localAgent, remoteAgent, remoteHost}`) + `setSlotTarget(slotId, patch)` action. Mirrored — a slot's target must follow to the phone, which is what moving it out of component-local `localStorage` buys. |
| `src/store/usageSourcesStore.js` | **deleted** — its four fixed flags become four entries in the keyed map, seeded once at boot (§5). |
| `src/composables/usageMonitorRegistry.js` | **new** — `getMonitor(agentId, host)`, the multiton. |
| `src/composables/useAgentUsage.js` | signature `(agentId, host, enabledRef, lockedRef)` with a **plain string** host. Deletes: the `hostRef` watcher, `realHostChange`, `lastNonNullHost`, `onUnmounted`. |
| `src/components/AgentUsageSection.vue` | stops constructing sources entirely; becomes pure tier layout. |
| `src/components/AgentUsageSlot.vue` | resolves its own monitor from the registry; host picker writes the slot's target. |
| `src-tauri/**` | **untouched.** Already per-host and correct. |

### `onUnmounted` must go

Today `useAgentUsage` registers `onUnmounted` to clear its timer. Under a registry the monitor is
created lazily inside *whichever* component first asked for it — so that hook would bind a
session-shared object's lifetime to one accidental component, and unmounting that slot would stop a
monitor another slot is still watching. Monitors are session-lived, like the stores they read; the
wake-subscriber and timer live as long as the app does.

---

## 4. AG cache v3 — the part with real blast-radius risk

**v2 shape:** `{ accounts: { "<email>:<sourceType>": { data, fetchedAt, host } }, lastActiveEmail }`

**v3 shape:** `{ accounts: { "<host>|<email>:<sourceType>": { data, fetchedAt, host } }, lastActiveEmailByHost: { "<host>": "<email>" } }`

Three separate bugs die here, all of them invisible until a second host exists:

1. **Collision.** Same account on two hosts wrote one key. Host B's poll overwrote host A's entry;
   host A's scope check then rejected its own former entry and showed empty. Host is now in the key.
2. **`lastActiveEmail` was global.** One host's account switch moved the other host's fallback pointer.
   Now per-host.
3. **`listAgAccounts()` dedup deleted across hosts.** It kept the newest record *per email* over the
   whole store and `delete`d every other — i.e. host A's entry was destroyed by host B's fresher poll
   of the same account. Dedup is now scoped inside one host's partition, and the function takes the
   host it is listing for.

Per the project's **Regression Guard — Multi-entity State**: every function here is scoped to one
host's partition by construction and none of them can touch another host's entries. The v2→v3
migration re-keys existing entries under their recorded `host` (or `local` when a legacy entry has
none, which is the only value it can have had — the remote path was broken until 1.20.0), so no
existing reading is dropped.

## 5. Migration of the four legacy enabled flags

`usageMonitorStore` seeds itself once, at module load, from the four keys `usageSourcesStore` wrote,
resolving `remote` against whatever host is selected at that moment:

| legacy key | becomes |
| :-- | :-- |
| `aki-src-ag-enabled` | `antigravity@local` |
| `aki-src-cclocal-enabled` | `claudecode@local` |
| `aki-src-agremote-enabled` | `antigravity@<selectedSshHost>` |
| `aki-src-ccremote-enabled` | `claudecode@<selectedSshHost>` |

A monitor with no entry defaults to **on**, matching every current source's default. That means
pointing a slot at a new host starts monitoring it immediately, which is the whole point of the
request; an unreachable one is handled by the existing consecutive-failure breaker, not by the default.

## 6. Definition of done

Static, doable here:

1. `grep -rn "usageSourcesStore" src` — no hits (file deleted, all four flags migrated).
2. `grep -rn "aki-antigravity-usage-cache" src` — only `agUsageCache.js`. No component parses the
   cache by hand any more.
3. `grep -rn "realHostChange\|lastNonNullHost" src` — no hits; the retarget-and-wipe path is gone,
   not merely bypassed (`flow.B8`: the old guards should become *unnecessary*, not disabled).
4. `grep -rn "isHost\|__TAURI_INTERNALS__" src/components src/store` — unchanged; no new env branch.
5. `npm run lint:scripts` and `npx vite build`. **Both now run and pass clean** — `lint:scripts`
   confirms both remote scripts POSIX-sh safe; `npx vite build` completes in ~7s with no errors
   (only pre-existing chunk-size warnings from the mermaid/cytoscape dependency, unrelated to this
   refactor).

Mac-side, user-triggered, **unverified until then**:

6. Tier 2 on. Slot C → REMOTE/CC/hostA, slot D → REMOTE/CC/hostB, two different Claude accounts.
   Both render their own numbers **simultaneously**, and neither shows the other's.
7. With both live, retarget slot C to hostA→hostB→hostA. Slot D's reading never blanks (the
   multi-entity check: act on one entity, confirm every other entity survived untouched).
8. Same for AG with the **same Google account signed in on both hosts** — the collision case v3 exists
   for. Each card must show its own host's quota, not the other's.
9. Toggle one monitor's power off: only that monitor stops. The other host keeps polling.
10. Two slots pointed at the *same* (agent, host): confirm one SSH round trip per interval in
    `usage.log`, not two — the multiton property.
11. Reload the app: slot targets and per-monitor power states come back as left.
12. On a paired phone: slot targets and power states mirror; changing a slot's host from the phone
    retargets the Mac's slot.

## 7. Deliberately not in this refactor

* **Mirroring the readings themselves.** Each screen still fetches its own numbers — usage data lives
  in monitor-owned refs, not in `src/store/*.js`, so the mirror does not carry it (the open R-1 item
  in `docs/plan/done/remote-control.md` §8a, also listed in `docs/plan/done/1.20.0-terminal-and-remote-sync.md` §7).
  This refactor does not close it, but it does make it cheap for the first time: with monitors keyed
  by `agentId@host`, the target shape is a keyed `usageStore` map with exactly those keys. Left out
  because nothing in the reported problem depends on it and it is a workstream of its own.
* **More than one remote host in the SSH-config modal / sync features.** `selectedSshHost` keeps its
  existing meaning for project sync and diff. Only the usage monitors gained a per-slot target.
* **Narrowing what a store may export.** `services/intents.js` registers *every* exported function of
  a `src/store/*.js` module as a companion-invokable intent key, so the pure readers these two new
  stores export (`slotTarget`, `isMonitorEnabled`, `monitorId`) become reachable that way. They mutate
  nothing and return into a void — a non-issue next to the command surface `hostInvoke.js` already
  exposes by design (1.19.0's security note) — and keeping `slotTarget` beside the data it resolves is
  worth more than the theoretical tidiness of moving it. Noted rather than changed.
* **Disposing idle monitors.** A monitor for a host no longer displayed anywhere stays in the registry
  with its polling gated by `enabled`. Bounded by the number of SSH hosts; not worth a lifecycle.
