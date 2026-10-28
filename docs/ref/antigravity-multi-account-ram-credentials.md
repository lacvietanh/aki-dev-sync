# Reference: AGY CLI Multi-Account Session & In-RAM Credential Lifecycle

**Date:** 2026-07-22  
**Topic:** AGY CLI (`/Users/aki/.local/bin/agy`) Credential Management & Multi-Account Isolation  
**Location:** `docs/ref/antigravity-multi-account-ram-credentials.md`  

---

## 1. Overview & Behavioral Discovery

When operating Google Antigravity CLI (`agy`) across multiple terminal windows or subagents:
- **In-RAM Credential Persistence**: Each running `agy` process loads OAuth2 tokens and session state into memory (RAM) upon initialization.
- **Session Isolation**: Once an `agy` session starts, its active credentials remain pinned to that specific process memory space for the duration of the session, even if the primary account files on disk (`~/.gemini/oauth_creds.json` or `google_accounts.json`) are edited, switched, or logged out.
- **Concurrent Multi-Account Support**: As a direct result, multiple terminal windows can concurrently run `agy` sessions authenticated under **different Google accounts** simultaneously without interfering with each other's active access tokens.

---

## 2. Technical Breakdown

```text
 Terminal Session 1 (Account A)         Terminal Session 2 (Account B)
┌──────────────────────────────┐       ┌──────────────────────────────┐
│ agy Process (PID 1024)       │       │ agy Process (PID 2048)       │
│  └─ In-RAM Token: Acc A      │       │  └─ In-RAM Token: Acc B      │
│  └─ Statusline: email A      │       │  └─ Statusline: email B      │
└──────────────┬───────────────┘       └──────────────┬───────────────┘
               │                                      │
               ▼                                      ▼
 Google Antigravity Endpoints           Google Antigravity Endpoints
 (Quota & Usage: Acc A)                 (Quota & Usage: Acc B)
```

### Key Behaviors:

1. **Cold Start vs Active Process**:
   - On cold start, `agy` reads initial credentials from `~/.gemini/` configuration files on disk.
   - Once initialized, access tokens and refresh tokens are cached in process RAM.

2. **Disk Switch Immunity**:
   - Changing active account pointers in GUI tools (such as Aki-Dev-Sync) or replacing `oauth_creds.json` changes the default account for *new* `agy` invocations.
   - Already-running `agy` interactive sessions continue executing under their initial in-RAM account until the process terminates.

3. **Statusline Payload Isolation**:
   - When `agy` invokes `~/.gemini/antigravity-cli/statusline.sh`, it passes the statusline JSON payload representing **that specific process's session**.
   - Fields such as `.email`, `.quota` (`gemini-5h`, `gemini-weekly`), `.context_window`, and `.model` accurately reflect the specific account bound to that process in RAM.

---

## 3. Implications for Aki-Dev-Sync & Statusline Customizer

1. **Per-Slot Account Viewing**:
   - In Aki-Dev-Sync's 2x2 usage grid, different slots can monitor different active accounts simultaneously without assuming a single global account pointer.
2. **Defensive Account Handling**:
   - If an account is logged out on disk while an `agy` process continues running, statusline scripts and usage monitors must degrade gracefully without throwing errors if disk files are removed.
3. **Session Cache Reset**:
   - To force an `agy` terminal session to pick up a newly selected account, the user must exit and restart the `agy` CLI process in that terminal.

---

## 4. Antigravity desktop shares the CLI sign-in (2026-10-19, agy 1.4.2)

- **Shared:** Antigravity desktop (`/Applications/Antigravity.app`) and `agy` CLI use one sign-in. The account signed in last on either surface is the one both use on their next start. Example: sign in A on desktop, then B on the CLI; quit and reopen desktop and it runs as B.
- **Pinned while running:** like an `agy` session (§ 2), a running desktop app keeps the account it started with until it restarts.
- **Separate:** Antigravity IDE (`Antigravity IDE.app`) keeps its own account.
- **What Aki Dev Sync shows:** the `desktop` frame reports the desktop's in-RAM account, the `cli` frame the on-disk account (`agy -p "/usage"`), the `ide` frames the IDE's. When desktop and CLI agree, the account appears once, with the desktop icon (its frame comes first in the per-email merge of `antigravity_payload.rs`; the `cli` frame only fills a tier or quota the desktop frame lacks).
- **Trail:** owner observation 2026-10-19; probe output and `lsof` evidence in `docs/research/antigravity-multi-instance-cli-discovery-jul22.md` § Amendments.
