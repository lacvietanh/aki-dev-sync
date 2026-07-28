# WS-C — Usage probe OOP: AG probe to shell, `Interpreter::Node` deleted, `agent_usage.rs` decomposed

Status: planning · Opened 2026-07-27 · Tracker: `docs/plan/backlog-jul27.md` (WS-C) · Owner: Architect

## 0. Why this exists — the accident being corrected

The Antigravity usage probe is written in Node for one reason that is documented in the artifact itself: `scripts/get-antigravity-usage.js`'s own header says it "replaces the buggy third-party NPM package `antigravity-usage`", and `package.json` still carries `"antigravity-usage": "^0.2.9"` as a dependency that **nothing imports** (verified: no `import`/`require` of it anywhere in `src/`, `src-tauri/`, `scripts/`). The probe inherited JavaScript from the package it displaced. It was never an architectural decision, and no other remote payload in this app is written in anything but POSIX sh.

That accident is what made the ESM/stdin defect possible at all. `agent_usage.rs`'s `NODE_BIN_RESOLVER_PREAMBLE` doc comment (lines 200-215) is 15 lines of reasoning that exists solely to answer "how do I resolve a binary for an interpreter whose stdin I must not touch": a shell preamble cannot be prepended to a stdin stream that a `node` process is about to read as JavaScript, and a shell that `exec`s into node mid-stream can silently swallow script bytes it has already buffered. The CC probe has none of that problem, because shell text concatenated ahead of more shell text is just shell text. Every one of those 15 lines is a cost paid for the language accident, and the port deletes the whole class.

Second-order cost, still latent today and worth naming because it is the strongest argument that the current arrangement is fragile: the JS is ESM (`import { exec } from 'child_process'`) and is fed to `node` on **stdin**, where Node's default module system is CommonJS. It works only because recent Node enables module-syntax detection by default. On a host with an older Node this probe fails with a syntax error and the failure lands in the `Ok(miss)` swallow, i.e. invisibly. *(Marked **unverified** — the exact Node version boundary has not been tested on either target; the point stands regardless, since the port removes the dependency rather than pinning it.)*

## 1. Scope, non-goals, MVP boundary

### In scope

- `scripts/get-antigravity-usage.js` → `scripts/get-antigravity-usage.sh`, POSIX sh, behaviour-preserving per §3's checklist.
- The JSON shaping that currently happens in JS moves to Rust (`serde_json`), matching how the CC probe already works. See §6.
- `Interpreter::Node`, `NODE_BIN_RESOLVER_PREAMBLE` and `run_remote_node_timeout` deleted from `src-tauri/src/agent_usage.rs`.
- `agent_usage.rs` (892 lines) split by domain; the script-transport funnel promoted out of it entirely.
- `docs/arch/usage-antigravity.md` rewritten to match; `docs/arch/usage-claudecode.md` §2 and §8 corrected; `docs/index.md` and `CHANGELOG.md [Unreleased]` updated.

### Non-goals — explicitly not touched

- **The IPC surface does not change.** Command names (`get_agent_usage`, `provision_agent_usage`, `logout_antigravity`, `logout_antigravity_cli`), argument names (`agentName`, `host`), and `AgentUsageResult`'s three fields (`host_answered`, `miss_reason`, `data{content,fetched_at,file_modified_at}`) stay byte-identical. `src/services/hostInvoke.js`'s default-deny allowlist and `src/services/bridge.js`'s per-command timeout map key on those literal strings, and `src-tauri/src/lib.rs`'s `invoke_handler!` list must come out of this workstream unmodified — that unchanged list is itself a check that the public surface did not move.
- **The AG payload shape does not change.** `AgentUsage.vue` and `agUsageCache.js` read `email`, `userTier.name`, `sourceType`, `models[].{label,remainingPercentage,resetTime}`, `quotaSummary.groups[].{displayName,buckets[].{window,bucketId,remainingFraction,resetTime}}`, `allAccounts[]`, and `timestamp`. Those are contract.
- **No Windows support in the ported script.** `detectOnWindows()` (wmic → PowerShell, JS lines 140-192) is dropped. The app ships macOS-only and the confirmed remote is Ubuntu; carrying a code path neither target can reach, in a language where it would have to be re-expressed as a PowerShell shell-out, is a cost with no consumer. Recorded as a deliberate deletion in §3, not an omission.
- **No new UI, no new badge, no new row.** Extreme Narrow.
- **No CC behaviour change.** `get-claudecode-usage.sh` and `provision-claudecode.sh` are untouched; the CC probe's parsing moves file but not logic.
- **No `jq` or `python3` dependency is added.** See §6.
- **The `logout_antigravity` macOS-only support-dir assumption is not fixed here.** It is a real gap (the `#[cfg]` non-mac branch guesses `~/.config/Antigravity IDE`) but it is a different feature from usage probing and would widen this diff for no test coverage.
- **Parallel RPC is not reproduced.** JS issues `GetUserStatus` and `RetrieveUserQuotaSummary` through `Promise.allSettled`; the port issues them sequentially. See §3 item P21 and §7's latency risk.

### MVP boundary

The port is complete when the Ubuntu remote — which has no `node` — produces a populated Antigravity card, and the Mac's card is byte-identical to what the JS produced for the same live state. Anything beyond that (parallelism, a retry ladder, richer diagnostics) is out of scope for this workstream.

## 2. Constraints that survive the refactor

These are inherited, not negotiated. Each is checked in §8's acceptance criteria.

1. **Never block the UI** (`CLAUDE.md` GLOBAL TAURI STACK; `tauri.A1`). All four `#[tauri::command]`s stay `async fn` with the blocking body inside `tauri::async_runtime::spawn_blocking`. The split must not accidentally inline a synchronous body into an `async fn` while moving code. Audit before closing: `grep -n "#\[tauri::command\]" -A3 src-tauri/src/agent_usage/*.rs`.
2. **PATH-resolution cold-start race** (`tauri.A2`). The fix pattern must stay in **one shared preamble at the single dispatch funnel**, never scattered per call site. After the port, exactly one preamble constant survives (`CLAUDE_BIN_RESOLVER_PREAMBLE`) and it becomes a caller-supplied parameter to the funnel rather than something the funnel knows about — see §4 and §6.
3. **macOS-only app, possibly-Linux remote.** The bundle is a `.dmg`; the remote is confirmed Ubuntu. Every script must branch on `uname`, and static path candidate lists stay mac-first with Linux added only where a Linux target is actually exercised (here: `ss`/`netstat` port discovery, which the JS already had).
4. **`ssh host sh` is `dash`, not `bash`.** `docs/arch/usage-claudecode.md` §5.4 records that a bare `set -o pipefail` kills dash with exit 2 and zero output. `scripts/lint-remote-scripts.js` is the standing guard and the new script must be added to its `REMOTE_SCRIPTS` list.
5. **One host, one concurrent script.** `host_lock` serializes every feature's traffic to a host. The AG probe already goes through it and must keep doing so after the interpreter change.
6. **Every non-zero AG exit is a transient monitor condition, swallowed to a `miss`.** `docs/arch/usage-antigravity.md` records that surfacing these as IPC `Err` *was* the "usage keeps erroring" instability. The port must not turn a new failure mode (no `curl`, no `ss`) into a banner.

## 3. Behaviour-parity checklist — JS probe → shell probe

Every distinct behaviour of `scripts/get-antigravity-usage.js`, with the layer that owns it after the port. A reviewer ticks this table; nothing here is "verified by reading the diff".

**Where** column: `SH` = the new `.sh`, `RS` = `antigravity_payload.rs` (the Rust assembler), `—` = deliberately dropped.

| # | Behaviour (JS line ref) | Where | Notes / risk |
|---|---|---|---|
| P1 | Process list via `ps auxww` on darwin (L68) | SH | Port uses `ps auxww` on **both** OSes, not JS's `ps aux` for Linux. Deliberate superset: `ps aux` can truncate, and a truncated line loses the csrf token silently. |
| P2 | Match only the six native `language_server*` binary names (L46-53) | SH | Fixed list, iterated; do **not** collapse to a `language_server` prefix — the JS comment records that generic matching collided with Volar's `language-server.js`. |
| P3 | Line must also contain `--csrf_token` or `--extension_server_port` (L78) | SH | |
| P4 | pid = whitespace field 2; commandLine = fields 11.. (L120-128) | SH | `awk '{print $2}'` / `awk '{for(i=11;i<=NF;i++) …}'`. |
| P5 | Skip lines with <11 fields; skip non-numeric pid (L121-126) | SH | |
| P6 | De-duplicate by pid across the whole scan (L74, L81, L101) | SH | One `seen_pids` string, `case " $SEEN " in *" $pid "*)`. |
| P7 | `type=desktop` when the line has `Antigravity.app` but **not** `Antigravity IDE.app`; else `ide` (L82-86) | SH | Order-sensitive: test the negative first. |
| P8 | `--csrf_token` extracted in both `=value` and ` value` forms, quotes stripped, case-insensitive (L194-206) | SH | **Highest-risk item.** A missed token yields HTTP 401 from `GetUserStatus`, which the flow swallows as a quiet miss — a silent, total failure. Must have its own unit check in S2 against a real captured `ps` line. |
| P9 | `--extension_server_port` extracted the same way, parsed as int (L130, L135) | SH | |
| P10 | `agy` CLI processes: line contains `agy`, not `get-antigravity-usage`, not `grep`; commandLine is exactly `agy`, or starts `agy `, or contains `/bin/agy` or `/agy `; `type=cli`, no csrf, no port (L95-113) | SH | Keep the `grep` exclusion even though the port avoids `grep` in the pipeline — a user's own `grep agy` in another terminal would otherwise match. |
| P11 | Windows detection via wmic then PowerShell (L140-192) | — | **Dropped** — see §1 Non-goals. |
| P12 | Port list seeded with `extensionServerPort` then `+1`, **before** discovered ports (L212-216) | SH | Order matters: it decides which port answers first and therefore which `baseUrl` is used. |
| P13 | darwin discovery: `lsof -nP -iTCP -sTCP:LISTEN -a -p <PID>`, ports from `:(\d+)\s+\(LISTEN\)` (L220-230) | SH | |
| P14 | Linux discovery: `ss -tlnp` filtered on `pid=<PID>,`, falling back to `netstat -tlnp` filtered on `<PID>/`; ports from `:(\d+)\s` (L252-271) | SH | Both may be absent on a minimal Ubuntu — see §6's tool-missing exit code. |
| P15 | Windows discovery via `netstat -ano` (L231-249) | — | Dropped with P11. |
| P16 | A failing `lsof`/`ss`/`netstat` is **non-fatal**; the seeded ports are still probed (L273-275) | SH | This is why the script must not use `set -e`. See §6. |
| P17 | Ports de-duplicated, filtered to `0 < p < 65536`, **insertion order preserved** (L278) | SH | |
| P18 | Probe ports in order; first success wins; per port try HTTPS **then** HTTP (L284-304) | SH | |
| P19 | Probe = `POST /exa.language_server_pb.LanguageServerService/GetUnleashData`, body `{"wrapper_data":{}}`, headers `Content-Type: application/json` + `Connect-Protocol-Version: 1` + `X-Codeium-Csrf-Token` only when a token exists, timeout **500 ms**, TLS certificate **not** verified (L306-344) | SH | `curl -sk --max-time 0.5`. Sub-second `--max-time` is curl ≥7.32 — **unverified** on the Ubuntu remote's curl build; fallback is `--max-time 1`, at the cost of doubling the worst-case probe walk. |
| P20 | Accept **only** HTTP 200 or 401 as "this is the Connect API" (L282, L324) | SH | 401 counts as success on purpose: the port is right, the token is wrong. |
| P21 | `GetUserStatus` and `RetrieveUserQuotaSummary` issued in parallel via `allSettled`, each 2000 ms, `Accept: application/json`, body `{"metadata":{"ideName":"antigravity","extensionName":"antigravity","locale":"en"}}` (L347-455, L563-566) | SH | Ported **sequentially**. Semantics preserved (a failure of either is independently tolerated); only wall-clock changes. |
| P22 | Non-2xx, or a body that is not JSON, counts as that RPC having failed (L373-381) | SH emits code + body; RS decides | The shell records the HTTP status; Rust decides "usable" — putting the JSON-validity test where a JSON parser exists. |
| P23 | `GetUserStatus` failed → skip this process entirely (L571-575) | RS | |
| P24 | `userStatus = raw.userStatus ?? raw` (L577) | RS | Two response envelopes in the wild; both must still work. |
| P25 | No `email` in `userStatus` → skip this process (L578-581) | RS | |
| P26 | `extractQuota`: from `userStatus.cascadeModelConfigData.clientModelConfigs[]` build `models[]` with `modelId` (`modelOrAlias.model`, else `"unknown"`), `label`/`displayName` from `m.label` when it is a string, `quota.remainingPercentage = remainingFraction`, `quota.usedPercentage = 1 - remainingFraction`, `quota.resetTime`, `quota.timeUntilResetMs`, `isExhausted = (remainingFraction === 0)` (L503-534) | RS | Type-guarded in JS (`typeof … === 'number'`) — serde must reproduce the guard, not coerce. |
| P27 | `parseModelQuota`: `label` falls back `label → displayName → modelId`, so it is **never** null (L471-472) | RS | `AgentUsage.vue` calls `m.label.toLowerCase()` unguarded — a null label throws in the card's computed. Keep the fallback chain exactly. |
| P28 | `isAutocompleteOnly` = modelId contains `gemini-2.5`, or label/displayName contains `Gemini 2.5` (L478-480) | RS | Not read by the frontend today; ported anyway, since the whole object is persisted into `agUsageCache`'s localStorage. |
| P29 | `parseResetTime`: milliseconds until reset, `undefined` when the instant is past or unparseable (L457-467) | RS | Needs RFC-3339 parsing in Rust with **no new crate** — see §6's note. |
| P30 | Snapshot = `{timestamp: <ISO-8601 now>, method: "local", email, userTier: userTier ?? null, models: [], quotaSummary: quotaSummary ?? null}` (L484-500) | RS | `timestamp` must be injectable for the golden test to be deterministic — see §8 S3. |
| P31 | `quotaSummary = rawSummary.response ?? rawSummary ?? null` (L590) | RS | Passed through **verbatim**, never re-serialized field by field: `AgentUsage.vue` reaches into `groups[].buckets[]` with keys this app never enumerates. |
| P32 | `snapshot.sourceType = processInfo.type ?? "ide"` (L592) | RS | |
| P33 | De-duplicate snapshots on `email:type`, first wins (L594-598) | RS | |
| P34 | Zero processes → stderr `{"error":"Antigravity IDE or CLI process is not running."}`, exit 1 (L539-542) | SH | Exit code and the swallow-to-`miss` behaviour must be identical. |
| P35 | Zero usable snapshots → stderr `{"error":"Could not fetch user status from any running Antigravity process."}`, exit 1 (L601-604) | SH (no frames emitted) + RS | With the split, the shell emits no frames and exits 1; Rust never sees an empty-frame case. |
| P36 | `desktop` and `cli` snapshots sharing an email collapse: the first is kept and its `sourceType` becomes `desktop_cli`; `ide` snapshots are always kept separately (L607-622) | RS | Order-dependent on the `ps` scan order, which the shell must preserve end to end. |
| P37 | `primary = finalSnapshots[0]`; when >1, `primary.allAccounts = finalSnapshots.map({email, method, sourceType, userTier, quotaSummary, models, timestamp})` — a **subset** of fields, not the whole snapshot (L625-636) | RS | `agUsageCache.addReading` branches on `allAccounts` being a non-empty array. Emitting it for a single account would change cache-key behaviour. |
| P38 | stdout = `JSON.stringify(primary, null, 2)` (L637) | RS | Pretty-printing is cosmetic — the Rust side re-serializes with `serde_json::to_string` for the `content` field, exactly as CC's path already does. Not a parity break; recorded so nobody "fixes" it. |
| P39 | Any uncaught error → stderr `{"error": …}`, exit 1 (L638-641) | SH | |
| P40 | `DEBUG` env → `[DEBUG]` lines on stderr (L39-43) | SH | Replaced by the project's own `_log()` convention from `get-claudecode-usage.sh` — `[<ts>][SHELL:ag-usage] …` to stderr, **unconditionally**. `get_antigravity_usage` currently does not relay AG stderr at all; the port adds `log_shell_stderr("USAGE:antigravity", …)`, which is how AG's stderr becomes visible in `usage.log` the way CC's already is. |

Two items are **new, not ported**, and must be reviewed as additions rather than parity:

| # | New behaviour | Rationale |
|---|---|---|
| N1 | A dedicated exit code (`3`) for "a required tool is missing on this host" (`curl`, and both of `ss`/`netstat`/`lsof`) | Replaces `looks_like_path_miss`'s `exit_code == 127 \|\| stderr.contains("command not found")` text heuristic (`agent_usage.rs:675`), which existed only because a missing `node` surfaced that way. A permanent condition deserves a code, not a string match. |
| N2 | A `PATH` floor line at the top of the script | See §6.3. |

## 4. Module map — current concerns → proposed files

### 4.1 What `agent_usage.rs` currently holds (evidence for the split)

| Lines | Concern |
|---|---|
| 1-11 | Module header, doc refs, imports |
| 13-23 | Four timeout constants (two transport, one CC-specific, one statusline-specific) |
| 25-43 | Three public entry wrappers (`run_remote_script`, `run_remote_node_timeout`, `run_remote_script_bounded`) |
| 45-55 | `host_lock` — per-host serialization registry |
| 57-59 | `is_local_host` |
| 61-75 | `polling_ssh` — the hardened SSH option set |
| 77-120 | `Interpreter` enum + `spawn` + `preamble` |
| 122-138 | `bounded_remote_sh` — the self-terminating remote wrapper |
| 140-198 | `CLAUDE_BIN_RESOLVER_PREAMBLE` — **Claude Code domain knowledge living inside the transport** |
| 200-215 | `NODE_BIN_RESOLVER_PREAMBLE` — **Antigravity domain knowledge living inside the transport** |
| 217-286 | `run_interpreter_timeout` — the funnel (lock, spawn, drain, stdin, poll, kill) |
| 288-331 | `AgentUsageResponse` / `AgentUsageResult` + three constructors — the IPC contract |
| 333-345 | `host_answered` — the reachability rule |
| 347-368 | `now_secs`, `preview` — shared formatting |
| 370-378 | `ab` — agent-name abbreviation for logs |
| 380-416 | `provision_agent_usage` command + `_sync` body (dispatch + CC implementation) |
| 418-437 | `get_agent_usage` command — agent dispatch |
| 439-450 | `log_shell_stderr` |
| 452-457 | `cc_auth_force_needed` — CC once-per-host latch |
| 459-464 | `ag_node_missing_once` — AG once-per-host latch |
| 466-645 | `get_claudecode_usage` — 180 lines: run probe, parse a five-part delimiter chain, parse cache JSON, inject metadata, summarize rate limits for the log |
| 647-705 | `get_antigravity_usage` — run probe, classify exit, pass stdout through |
| 707-748 | AG app constants, account-only Chromium paths, support-dir resolution, auth keys |
| 750-781 | `remove_antigravity_auth_rows` — sqlite3 row deletion |
| 783-827 | `logout_antigravity` command |
| 829-856 | `logout_antigravity_cli` command |
| 858-892 | Tests (reachability rule + result constructors) |

Describing this module honestly requires four "and"s: it *runs scripts on hosts* **and** *defines the frontend's answer envelope* **and** *implements two unrelated agent probes* **and** *signs the local Antigravity IDE out of its Google account*. `design.A3` says split. Two further pieces of evidence make the boundary concrete rather than aesthetic:

- `src-tauri/src/statusline.rs:12` does `use crate::agent_usage::run_remote_script_bounded;`. The statusline customizer is not a usage feature. A module that another feature must import *through* an unrelated domain name is `design.A6`'s "stable boundaries" failing out loud.
- `logout_antigravity` touches no host, no script, no timeout, and no `AgentUsageResult`. It shares nothing with the rest of the file but the word "antigravity".

### 4.2 Proposed files

`remote_shell.rs` is promoted to a **top-level sibling** of `agent_usage/`, not a child, because it has a consumer outside usage.

| File | One-sentence responsibility (must contain no "and") | ~lines | Holds |
|---|---|---|---|
| `src-tauri/src/remote_shell.rs` | Runs one script on one host — locally or over SSH — within a hard time bound. | ~185 | Transport timeouts, `host_lock`, `is_local_host`, `polling_ssh`, `Shell` enum (`Plain` / `Bounded(u64)`, was `Interpreter`), `bounded_remote_sh`, the funnel `run(host, shell, preamble, script, timeout)`, plus `run(…)`/`run_bounded(…)` convenience wrappers. |
| `src-tauri/src/agent_usage/mod.rs` | Exposes the usage IPC surface and routes each request to the agent that owns it. | ~70 | The `#[tauri::command]`s `get_agent_usage` and `provision_agent_usage`, their `validate_remote_host` + `spawn_blocking` shells, the two-arm agent match, `pub use` re-exports of the two logout commands so `lib.rs` is untouched. |
| `src-tauri/src/agent_usage/probe_result.rs` | Defines one answer to the frontend, and the rule for whether the host answered at all. | ~95 + tests | `AgentUsageResponse`, `AgentUsageResult`, `hit`/`miss`/`unreachable`, `host_answered`, `now_secs`, and the three existing tests moved verbatim. |
| `src-tauri/src/agent_usage/probe_log.rs` | Formats probe activity for the usage log. | ~40 | `preview`, `ab`, `log_shell_stderr`. |
| `src-tauri/src/agent_usage/claudecode.rs` | Obtains Claude Code's usage from a host, including installing the hook that produces it. | ~265 | `CLAUDE_BIN_RESOLVER_PREAMBLE`, `CLAUDE_CALL_TIMEOUT_SECS`, both `include_str!`s, `cc_auth_force_needed`, `probe(host)`, `provision(host)`, and `probe`'s internals split so no function carries an "and": `parse_probe_output`, `inject_metadata`, `log_rate_limits`. |
| `src-tauri/src/agent_usage/antigravity.rs` | Obtains Antigravity's usage from a host and classifies why a poll produced nothing. | ~115 | `include_str!` of the new `.sh`, `probe(host)`, the exit classification (transient vs. the N1 permanent code), `ag_tool_missing_once`. |
| `src-tauri/src/agent_usage/antigravity_payload.rs` | Turns raw Connect-RPC responses into the account snapshot the card reads. | ~165 + tests | Frame splitting, P24-P33 and P36-P37, an injected clock, and the golden-fixture test. **Pure — no I/O, no host, no subprocess**, which is the whole reason it is its own file. |
| `src-tauri/src/agent_usage/antigravity_logout.rs` | Signs this Mac's Antigravity out. | ~120 | App name, account-only Chromium paths, support-dir resolution, auth keys, `remove_antigravity_auth_rows`, both logout commands. |

Every file is under 270 lines. Every name is role-based (`design.A7`): `probe_result`, not `types`; `antigravity_payload`, not `ag_utils`; `remote_shell`, not `exec`.

### 4.3 Three splits deliberately **not** made

`design.B1` forbids pre-splitting speculative modules as firmly as it forbids god-modules, so the negative decisions need recording too.

- **No `provision.rs`.** Provisioning has exactly one implementor: `provision_agent_usage_sync` returns `Ok(true)` immediately for Antigravity and `Err` for anything else. A module named for a generic capability that contains only Claude Code logic is a naming lie (`design.A7`). The dispatch stub lives with the other dispatch in `mod.rs`; the implementation lives with the probe it exists to feed (`usage-claudecode.md` §1: the statusline hook is the probe's *only* data source, so installing it and reading it are one responsibility, not two).
- **No `bin_preamble.rs`.** `CLAUDE_BIN_RESOLVER_PREAMBLE` stays in `claudecode.rs`, passed *into* the funnel as a parameter. This is strictly better than today: the transport stops knowing the word "claude" (`design.A6` — volatile domain detail pushed to the edge), while `tauri.A2`'s "one shared preamble at the single funnel" is preserved, because there remains exactly one place in the codebase that writes a preamble to a child's stdin.
- **No shared `once_per_host` helper.** `cc_auth_force_needed` and `ag_tool_missing_once` are the same five-line `OnceLock<Mutex<HashSet<String>>>` idiom, and after the split they sit in two different files — normally `design.A5`'s "the second paste is a mandatory STOP". Judged and rejected anyway: in Rust the extraction has to take the `&'static OnceLock` as a parameter, so each call site still declares its own static and the helper saves two lines while adding an indirection. Recorded here as a known duplicate with an explicit trigger: **extract at the third latch.**

## 5. The trait decision — `design.A2` Rule of Three, argued

**Verdict: no `UsageProbe` trait. Two concrete probe functions behind one dispatch match.**

The request names a trait, and two implementors is precisely the case the Rule of Three exists to adjudicate, so this is argued rather than asserted, and the `design.B3` critique gate is run below.

**The count.** `design.A2` requires "the same shape ≥3 times across ≥2 unrelated call sites." Here there are **two** implementors and **one** polymorphic call site — the `match` inside `get_agent_usage`. (`provision_agent_usage` also matches on agent name, but its Antigravity arm is a no-op stub, so it is not a second use of a probe abstraction.) A trait's payment comes from call sites that can be written once against the interface; with a single two-arm match there is nothing to write once. The `design.A2` risk-weighted exception — extract at the 2nd occurrence for auth, money, permissions, or data integrity — does not apply: usage display is documented as a best-effort monitor whose worst failure mode is a quiet card (`usage-antigravity.md`, "swallow any non-zero exit to Ok(None)").

**What the two probes actually share.** Both take a `host`, run a script, and return an `AgentUsageResult`. That is the entire overlap, and **all of it is already extracted**: `remote_shell::run` is the shared transport, `AgentUsageResult` + `host_answered` is the shared answer shape, `probe_log` is the shared logging. The commonality lives in shared *types and functions*, which is the cheaper mechanism, and a trait would add a second one over the top of it.

**What they do not share, and why it matters.** Claude Code reads a file that a hook wrote: the mtime is meaningful (`file_modified_at` drives the frontend's staleness badge), there is exactly one account by design, there is a `STALE_RESET` sentinel, and the payload arrives as a five-part delimiter chain that needs metadata injected into it. Antigravity queries a live RPC: there is no mtime (`fetched_at` and `file_modified_at` are set to the same value), there are N accounts with a dedup rule, and there is no cache, no sentinel, no metadata. A trait narrow enough for both — `fn probe(&self, host: &str) -> Result<AgentUsageResult, String>` — is a vtable wrapped around a two-arm match, and it *hides* the asymmetry the code most needs to keep visible. A trait rich enough to be useful (`fetch` / `parse` / `assemble`) would have to be modelled on one of the two and would deform the other; the mtime field alone shows which way that deformation would run.

**`design.B3` critique gate.**

- *Steelman not abstracting:* keeping two named functions costs one `match` arm per agent, is greppable, and lets each probe's error taxonomy stay in its own vocabulary (`STALE_RESET` vs. `no live AG session`). That is cheaper here, not merely simpler.
- *Attack the decision:* if a third agent (say Cursor or Codex) arrives, three `match` arms in two commands is the point where the duplication becomes real. **How we would know:** the third agent's probe needing the same scaffolding — validate host, spawn_blocking, run script, classify exit, wrap in a result — in the same order as the existing two. That is the trigger; when it fires, extract then. The module split delivered by this workstream is exactly what makes that later extraction mechanical instead of archaeological, which is the honest argument for doing the split now and the trait never-yet.
- *Smaller first:* the smallest version is what is proposed — no new abstraction at all, just moving what already exists into files named for what it does.

**Recorded trigger for revisiting:** a third agent, **or** any second call site that must iterate over agents generically (for example a "refresh all monitors" backend command). Neither exists today.

## 6. Shell strategy — JSON, `jq`, and `dash`

### 6.1 The JSON question, and the answer that avoids it

The JS probe parses JSON natively; shell cannot. The obvious ports are `jq` or `python3`. Both are rejected on this project's own evidence:

- **`jq` cannot be assumed present.** `docs/research/test-case-check-flow.md` records a real incident where a host without `jq` broke statusline Apply, and the unchecked `jq` dependency in `statusline.rs:319`/`:334` is a filed defect (E5). `docs/plan/done/statusline-customizer.md` already lists `jq` as a hard prerequisite that excludes Windows. Adding a `jq` requirement to the 30-second usage poll would put a documented-missing binary on the app's most frequent remote code path.
- **`python3` cannot be assumed present either.** `get-claudecode-usage.sh` guards every `python3` call with `|| echo 'STATUS:INTERPRETER_ERROR'` and carries an explicit `err=no_python3` branch, precisely because the interpreter may be absent — and §6b's comment records that an unguarded `VAR=$(python3 …)` under `set -e` once made a host "silently and permanently unmonitorable."

Trading `node` for `jq` would swap one unavailable interpreter for another and would not be a language unification at all.

**The strategy: the shell parses no JSON.** It emits raw RPC response bodies inside a delimiter frame, and `serde_json` — already a dependency (`src-tauri/Cargo.toml`) and already how the CC payload is handled — does the shaping in Rust. This is not a new pattern; it is the existing one. `get-claudecode-usage.sh` emits `<json>|||MTIME|||…|||SUBTYPE|||…|||TIER|||…|||AUTHINFO|||<json>` and `get_claudecode_usage` parses it with `serde_json`. The AG port adopts the identical convention (`design.A5`: compose with the shape that exists rather than invent a second one).

**Frame contract**, one block per successfully probed process, in `ps` scan order:

```
|||AGPROC|||<pid>
|||TYPE|||<ide|desktop|cli>
|||STATUSCODE|||<http status of GetUserStatus>
|||STATUS|||<raw GetUserStatus body>
|||SUMMARYCODE|||<http status of RetrieveUserQuotaSummary, or 0>
|||SUMMARY|||<raw RetrieveUserQuotaSummary body, or empty>
```

Rust splits on `|||AGPROC|||`, then on the inner markers, exactly as the CC path already splits its chain. A body that happens to contain the delimiter would corrupt the frame; that exposure is identical to the CC path's and is accepted on the same grounds — the delimiter is not a substring any Connect-RPC JSON can plausibly produce.

One genuinely new Rust need: **P29 needs an RFC-3339 timestamp parsed into milliseconds-until-reset**, and `chrono` is not a dependency. Adding a crate for one field is disproportionate. The `resetTime` strings Antigravity emits are fixed-width RFC-3339 UTC, so a small hand-rolled parse (already the shape of `AgentUsage.vue`'s `new Date(bucket.resetTime).getTime()`) suffices — and `timeUntilResetMs` is **not read by the frontend** (verified: no reference anywhere in `src/`), so a parse failure yielding `null` is behaviour-neutral. If the parse proves fiddly, emitting `null` is an acceptable MVP outcome that must be recorded rather than silently shipped.

### 6.2 `dash`, not `bash` — recorded explicitly

The script is delivered to `sh` on both paths: `sh` locally (macOS `/bin/sh`, i.e. bash in POSIX mode) and `ssh host sh` remotely (**`dash` on Ubuntu**). dash is the binding constraint; the local shell is a superset and will never catch a dash-only failure. Therefore:

- **Strict POSIX sh.** No `[[ ]]`, no arrays (the port list is a space-separated string iterated by `for p in $PORTS`), no `+=`, no `<<<`, no `function` keyword, no `set -o <long-name>`.
- **No `set -o pipefail`, guarded or otherwise.** The project's approved idiom `( set -o pipefail ) 2>/dev/null && set -o pipefail` exists, but the AG probe has no pipeline whose partial failure it needs to detect.
- **No `set -e`.** This is a deliberate divergence from `get-claudecode-usage.sh`, and the reason is P16: the AG probe's *normal* operation includes commands that are expected to fail — `lsof` exits non-zero when a pid has no listeners, `curl` exits non-zero for every closed port it walks past. Under `set -e` the first closed port would kill the probe. Every fallible command is guarded individually with `|| true` or an `if`, and the script's exit code is set deliberately at the end, because that code *is* the contract Rust reads.
- **Never read stdin.** The script itself arrives on stdin, so any construct that consumes stdin eats the rest of its own source. Concretely: every `while read` must take its input from a pipe (`printf '%s\n' "$x" | while IFS= read -r l`, the idiom already used at `get-claudecode-usage.sh:269`), never a bare `while read`; and `curl` must take its body via `-d '<literal>'`, never `-d @-`. This is the same class of buffered-stdin hazard that `NODE_BIN_RESOLVER_PREAMBLE`'s doc comment describes, and it does not disappear just because the interpreter is now `sh`.
- **`scripts/lint-remote-scripts.js` is the enforcement.** `get-antigravity-usage.sh` is added to `REMOTE_SCRIPTS`, and that file's comment "Keep in sync with `include_str!(... .sh)` calls in `src-tauri/src/agent_usage.rs`" is updated to the new module paths. The lint runs on `npm run dev` and `npm run tauri`, so a dash-killer cannot reach a build.

### 6.3 PATH and the `tauri.A2` race

The AG script invokes only system binaries — `ps`, `curl`, `lsof`/`ss`/`netstat`, `awk`, `sed`. None is user-installed, so none has the rc-sourcing timing dependency that `tauri.A2` targets: `/usr/sbin/lsof` is at `/usr/sbin/lsof` whether or not `.zshrc` has finished. What the script *is* exposed to is a **thin PATH** — a Finder-launched macOS app hands its children `/usr/bin:/bin:/usr/sbin:/sbin`, and a non-interactive `ssh host sh` gets whatever `/etc/environment` sets.

**Decision: a single `PATH` floor line at the top of the script** (`PATH="$PATH:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin"`), not a per-binary resolver in the funnel preamble. Rationale: the A2 fix pattern's purpose is to remove a *timing* dependency, and a static PATH floor removes it just as completely for fixed-location system tools, while a funnel preamble parameterized by "which tools does this script need" would be a generalization built for one caller (`design.B1`, no speculative pre-splitting). The rule's actual constraint — *do not scatter resolution across call sites* — is honoured: one line, one place, one script.

**Escalation trigger, recorded so it is not re-litigated:** if any target is found where a needed tool sits outside that floor, promote the resolution into the funnel preamble slot (which already exists, and which after this refactor takes a caller-supplied string) rather than adding a second ad-hoc line.

## 7. The two riskiest items in the port

1. **CSRF token extraction (P8).** JS uses two case-insensitive regexes covering `--csrf_token=value`, `--csrf_token value`, and quoted forms. The shell equivalent (`sed`/`awk` over the command line) is easy to get subtly wrong, and the failure is *silent*: a missing or truncated token makes `GetUnleashData` return 401 — which P20 still accepts as "correct port" — and then `GetUserStatus` returns 401, which P23 turns into "skip this process", which becomes an ordinary quiet miss. The card simply stays empty and nothing anywhere says why. Mitigation: S2 tests the extractor against a real captured `ps auxww` line for every argument form, and the S4 acceptance criterion is that the card **populates**, not merely that the probe exits 0.
2. **Probe latency under per-attempt `curl` spawning (P18-P21).** JS does one `http.request` per attempt inside a single process; the shell spawns a process per attempt, over (seeded ports + discovered ports) × (HTTPS, HTTP), then two more for the RPCs, then repeats the whole thing per detected process. With three surfaces live (IDE + desktop + CLI, the exact case `usage-antigravity.md` documents) this can be dozens of spawns per poll, against a 30-second `REMOTE_SCRIPT_TIMEOUT_SECS` and a poll interval of 30s. Two aggravating unknowns, both **unverified** until measured: whether the remote's curl honours a sub-second `--max-time 0.5` (P19), and how curl behaves attempting TLS against a plain-HTTP port versus how Node's `https` module does. Mitigation: S4 measures wall-clock probe time on both targets and records it; if it approaches half the bound, the fix is to stop probing remaining ports once a process yields a snapshot, not to raise the timeout.

## 8. Verification plan

`coding.B3`: everything below is either settled by a tool or marked **unverified — needs a runtime check**. Nothing is assumed.

### What `cargo check` / `cargo test` can prove

- The module split compiles: every moved item still resolves, `statusline.rs`'s import points at the new `remote_shell` path, `lib.rs`'s `invoke_handler!` list still names four existing commands.
- `Interpreter::Node`, `NODE_BIN_RESOLVER_PREAMBLE` and `run_remote_node_timeout` are gone with **no dangling reference** — the compiler is a complete oracle for this, which is why step S5 is safe to do mechanically.
- `include_str!("../../scripts/get-antigravity-usage.sh")` resolves. A missing file is a *compile error*, so a premature deletion of the `.js` before the `.sh` exists cannot silently ship.
- `serde` derives still apply to the unchanged `AgentUsageResult` (field names are the wire contract).
- `cargo test` runs the three moved reachability tests plus the new golden-fixture assembler test (S3).

### What `cargo check` cannot prove

- That the `.sh` runs at all, that it is dash-clean, that `curl` exists on either target, that the Connect RPC still answers, that the assembled JSON is what `AgentUsage.vue` reads, or that the multi-account dedup path behaves with real data.

### Static gates (this machine)

- `npm run lint:scripts` — regex bashism scan, `dash -n`, `shellcheck -s sh -S error` on the new script. **Note: this machine cannot build Tauri/Rust, so `cargo check`/`cargo test` are Mac-only steps.**

### Manual — on the Mac

| Check | Command / action | Passes when |
|---|---|---|
| M1 | `sh scripts/get-antigravity-usage.sh` | A frame is emitted for each live surface, exit 0. |
| M2 | **`sh < scripts/get-antigravity-usage.sh`** | Identical output to M1. This is the one that matters: it reproduces the real stdin delivery and is the only way to catch a construct that eats its own source (§6.2). |
| M3 | Golden diff | `node scripts/get-antigravity-usage.js` output equals the assembled JSON from the new pipeline, for the same live state. |
| M4 | ≥2 accounts present (`CLAUDE.md` Regression Guard) | With IDE + CLI (or two accounts) live, `allAccounts` lists both, the `desktop_cli` collapse (P36) fires correctly, and every *other* cached account in `agUsageCache` survives untouched. |
| M5 | Signed-out state | Sign out of Antigravity with the language server still running; the card falls back to the cached account, no error banner on any poll tick (`usage-antigravity.md`'s HTTP-500 `GetCascadeModelConfigData() is nil` case). |
| M6 | Latency | Wall-clock of one probe recorded, both with one surface live and with three. |
| M7 | UI thread | The card refreshes while the window is dragged/resized — no freeze (`tauri.A1`). |
| M8 | CC untouched | The Claude Code card shows identical numbers before and after the split. |

### Manual — on the Ubuntu remote (the load-bearing test)

| Check | Command / action | Passes when |
|---|---|---|
| L1 | `ssh <host> 'command -v node; echo rc=$?'` | Prints nothing and `rc=1`. **Capture this output** — it is the proof that whatever follows cannot possibly be running the old probe. |
| L2 | `ssh <host> 'command -v curl; command -v ss; command -v netstat; command -v lsof'` | Records exactly which discovery tools exist. **Currently unverified — must be run before S2 is considered designed**, since it decides whether P14's fallback ladder is sufficient or whether N1's exit-3 path is the common case. |
| L3 | `ssh <host> sh < scripts/get-antigravity-usage.sh` | Either a valid frame, or a clean exit 1 with a `[SHELL:ag-usage]` diagnostic. **Never** a dash syntax error, never exit 2 with empty output. |
| L4 | The app's REMOTE Antigravity card, host = the Ubuntu box | Populates with live numbers if Antigravity runs there; otherwise shows the ordinary empty state with a `miss` reason in `usage.log`. |
| L5 | `usage.log` with `--debug` | `SHELL:ag-usage` stderr lines are relayed (P40 — a capability AG did not have before). |

**Unverified until run, and recorded as such rather than assumed:**

- `curl`'s presence and version on the Ubuntu remote (L2). If absent, there is no clean POSIX substitute for POST-with-self-signed-TLS; the plan degrades to N1's exit 3 and a once-per-host ERROR line, and the remote AG feature is unavailable — that outcome must be reported to the user, not papered over.
- Whether the Ubuntu remote actually runs an Antigravity surface at all. If it does not, L4 degrades to "the script runs, detects nothing, exits 1 cleanly, no `node` involved" — which is still the load-bearing proof for the port even though it does not exercise the RPC path.
- Sub-second `--max-time` support (P19).
- The `desktop_cli` collapse (P36) against a remote whose `ps` ordering differs from the Mac's.

### One security note on the golden fixture

The S0 capture contains the account's real email address. **Redact it to `user@example.com` on both sides of the fixture before committing** (`coding.C4`: never commit identity into the repo). The CSRF token never appears in probe *output*, only in `ps` — do not capture a raw `ps` line into a committed fixture without redacting it too.

## 9. Execution order

Each step is independently reviewable and independently revertible. The ordering has one hard invariant: **the shell probe is proven equivalent before the Node path is removed**, so there is no commit in which the feature is broken.

| Step | Work | Acceptance criteria |
|---|---|---|
| **S0** | Baseline capture on the Mac, no code. Run `node scripts/get-antigravity-usage.js` with ≥2 surfaces live; save the output as the golden fixture (email redacted). Run L2 on the Ubuntu remote and record the tool inventory. | Fixture captured; L1 and L2 outputs recorded in this doc's status log. If L2 says no `curl`, **stop and report** — the design's central assumption has failed. |
| **S1** | Split only. Zero behaviour change: move code into `remote_shell.rs` + `agent_usage/{mod,probe_result,probe_log,claudecode,antigravity,antigravity_logout}.rs`; change `CLAUDE_BIN_RESOLVER_PREAMBLE` from something the funnel knows to something the caller passes; update `statusline.rs`'s import. The Node path is still fully intact. | `cargo check` clean, `cargo test` green (3 tests), `lib.rs`'s `invoke_handler!` list byte-identical, M7 + M8 pass, and `git diff --stat` shows moves rather than rewrites. |
| **S2** | Write `scripts/get-antigravity-usage.sh` (P1-P22, P34, P39, P40 + N1, N2). Add it to `lint-remote-scripts.js`'s `REMOTE_SCRIPTS` and update that file's sync comment. **Do not wire it to Rust.** | `npm run lint:scripts` green; M1, M2, L3 pass; the emitted `STATUS` body is valid JSON; the P8 extractor is checked against every argument form. |
| **S3** | Add `agent_usage/antigravity_payload.rs` (P23-P33, P35-P38) with an injected clock, plus its golden-fixture test. **Still not wired.** | `cargo test` proves the assembled JSON equals the S0 golden fixture, modulo the injected `timestamp`. |
| **S4** | Wire it: `antigravity::probe` switches to `Shell::Plain` + the new script + the assembler. `get-antigravity-usage.js` and `Interpreter::Node` **both remain on disk**, so a revert is one line. | M3, M4, M5, M6, L4, L5 pass. Latency recorded. If M3 diverges, fix here — this is the last step where the reference implementation is still available to diff against. |
| **S5** | Delete: `Interpreter::Node`, `NODE_BIN_RESOLVER_PREAMBLE`, `run_remote_node_timeout`, `scripts/get-antigravity-usage.js`, and the dead `"antigravity-usage"` entry in `package.json`. Rename `ag_node_missing_once` → `ag_tool_missing_once` and replace the `127`/"command not found" heuristic with N1's exit 3. | `cargo check` clean; `grep -rni "node" src-tauri/src/agent_usage/ src-tauri/src/remote_shell.rs` returns nothing outside unrelated prose; `grep -rn "antigravity-usage" package.json` empty; the app still works on both targets. |
| **S6** | Docs. Rewrite `docs/arch/usage-antigravity.md`: the Mermaid sequence diagram (`Node (Login Shell)` → `sh`), the "Execution Environment" section (which currently states `zsh -lc node` / `ssh <host> node`), and the Related Source Files table. Correct `docs/arch/usage-claudecode.md` §2's interpreter table (which names `Interpreter::Node`) and §8's file table. Update `docs/index.md`. Add the `[Unreleased]` CHANGELOG entry. Move this plan doc to `docs/plan/done/` and update `docs/plan/backlog-jul27.md`'s WS-C row. | `grep -rn "Interpreter::Node\|zsh -lc node\|ssh host node\|get-antigravity-usage.js" docs/ src-tauri/ src/` returns nothing outside `docs/plan/done/` history. |

**Interaction with the rest of the batch:** `backlog-jul27.md` states WS-C is independent of WS-A/B/D, and that WS-E's hard-wrap sweep must run **last** among code workstreams. S6's doc edits must therefore not pre-empt that sweep — write new lines unwrapped (`agent.C3`) and leave existing wrapping alone.
