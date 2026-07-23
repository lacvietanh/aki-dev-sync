# Research: Multi-CLI Statusline Account Isolation & OOP Adapter Architecture

**Date:** 2026-07-22  
**Target CLIs:** Antigravity CLI (`agy`), Claude Code CLI (`claude`)  
**Related Plan:** [`docs/plan/done/1.18.0-ag-statusline-and-slot-persistence.md`](../plan/done/1.18.0-ag-statusline-and-slot-persistence.md) (tên cũ khi nghiên cứu này được viết: `v1.18-enhancements.md`)  

---

## 1. Context & Motivation

When working with multiple accounts across different terminal tabs or windows, developers need clear visibility into which account is active in their current terminal session. The Statusline Customizer is expanded in v1.18.0 to support deploying statuslines to both Claude Code and AGY CLI.

This research analyzes how account identity is retrieved, process isolation guarantees per terminal tab, and the OOP Adapter Pattern used in Rust to generate statuslines cleanly.

---

## 2. CLI Account Extraction Mechanisms & Isolation Proof

### 2.1 Antigravity CLI (`agy`) — Process RAM Isolation (100% Per-Tab Accuracy)

- **Mechanism**: When `agy` executes in an interactive terminal tab, it retains the active account token and metadata in its **process RAM**.
- **Execution Flow**:
  1. Each terminal tab runs an independent `agy` process with its own PID.
  2. Whenever `agy` invokes `~/.gemini/antigravity-cli/statusline.sh`, it pipes a process-specific JSON payload directly via `stdin`.
  3. The JSON payload contains `.account.email` (or `.email`), representing that specific process's session.
- **Verification**: Verified in [`docs/ref/antigravity-multi-account-ram-credentials.md`](../ref/antigravity-multi-account-ram-credentials.md#L43-L46).
- **Extraction JQ Filter**: `(.account.email // .email // "")`
- **Result**: **100% per-tab process isolation**. Terminal Tab A (Acc A) and Terminal Tab B (Acc B) independently render their own correct account emails.

### 2.2 Claude Code (`claude`) — Dual-Strategy & Disk Fallback Limitation

- **Mechanism**: Claude Code invokes `~/.claude/statusline-command.sh` via `stdin`.
- **Payload Inspection**: Claude Code's native JSON payload currently includes session metrics (`cwd`, `model`, `cost`, `context_window`, `rate_limits`), but does **not** natively include an `email` field in its standard schema.
- **Dual-Strategy**:
  1. Primary: Attempt `(.account.email // .user.email // empty)` from `stdin` payload.
  2. Fallback: If `stdin` payload lacks email, read `oauthAccount.emailAddress` from `$HOME/.claude.json`.
- **Known Limitation**: Because of the disk fallback to `$HOME/.claude.json`, Claude Code statusline reflects the **globally active disk account**. If a developer overrides tokens per tab via `ANTHROPIC_API_KEY`, Claude Code statusline will display the global disk account until Anthropic adds native email fields to their `stdin` payload.

---

## 3. Architecture - SUPERSEDED (2026-07-23, Phase 2.2)

> **This section described the `StatuslineTarget` Adapter, which no longer exists.** It is kept
> because the account-isolation findings above are what forced the redesign.
>
> **What replaced it:** there is no longer one script *per target*. ONE physical script is installed
> byte-identically at both paths and works out which CLI is running it from its own invocation path
> (`$0` contains `/.gemini/` -> AGY, otherwise Claude Code). The per-CLI account fallback in §2 is a
> single `if` inside that script, not two Rust code paths. `statusline.rs` no longer generates shell
> at all: it patches a config region into the checked-in template `src-tauri/src/statusline-unified.sh`.
> See `docs/feat/statusline-customizer.md` and `docs/ref/statusline-unified-spec.md` §8.
>
> **Why the adapter had to go.** Its premise - "each target needs its own JQ extraction and its own
> account branch" - turned out to be false: a live `diff` of the two deployed scripts found they
> differed by exactly one 12-line block. Two near-identical generators meant every fix had to be
> written twice, and one copy kept getting missed - the `3p-*` quota branch existed in one and not
> the other, and `resets_at` was parsed but never displayed. The Open/Closed argument below bought
> extensibility the product never used, at the price of the drift it actually suffered from.

The superseded design, for the record:

```
                  ┌────────────────────────┐
                  │   StatuslineConfig     │  (Shared UI config: fields, colors, thresholds)
                  └───────────┬────────────┘
                              │
                              ▼
                 ┌──────────────────────────┐
                 │ Trait: StatuslineTarget  │  (Polymorphic Interface)
                 └────────────┬─────────────┘
                              │
             ┌────────────────┴────────────────┐
             ▼                                 ▼
┌─────────────────────────┐       ┌─────────────────────────┐
│   ClaudeCodeTarget      │       │      AgyCliTarget       │
├─────────────────────────┤       ├─────────────────────────┤
│ - ~/.claude/            │       │ - ~/.gemini/.../        │
│ - Rate limits parsing   │       │ - Quota fraction parse  │
│ - Disk fallback account │       │ - RAM session account   │
└────────────┬────────────┘       └────────────┬────────────┘
             │                                 │
             └────────────────┬────────────────┘
                              ▼
                 ┌──────────────────────────┐
                 │    BaseScriptRenderer    │  (Shared ANSI Color & Field Painter)
                 └──────────────────────────┘
```

### Advantages:
1. **Single Responsibility**: Target-specific JQ extraction is encapsulated within each concrete `StatuslineTarget` implementation.
2. **Open/Closed Principle**: Future CLI targets can be added by implementing `StatuslineTarget` without altering existing target logic.
3. **Shared UI**: The Vue frontend maintains a single intuitive Statusline Customizer interface.
