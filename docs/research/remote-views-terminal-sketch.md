# Remote views — original pre-build sketch for Terminal View

**Start time:** pre-2026-07-30 (original draft date not recorded; written while this content still lived in `docs/plan/remote-views-roadmap.md`, before that file's 2026-07-30 move to `arch/`).

**Initial purpose:** pin the intended shape of Terminal View (a shared remote PTY over `portable-pty` + `xterm.js`) before any of it was built, so the frontend/native split and the security posture had a target to build against.

**Strategy:** sketch the native side (PTY spawn, output streaming), the frontend side (`xterm.js` in a shared `TerminalView` component), the SYNC-1 input/output semantics (input is an intent, output is shared state, no local echo), and flag the open security/blocking questions to decide at build time.

**Checklist:** n/a — a design sketch, not an executed investigation.

**Result:** Terminal View shipped in 1.20.0 and followed this sketch essentially as written. Three points the sketch left open were decided during the actual build: the security posture (no extra per-session opt-in — gating the PTY alone would be theatre against the existing DEV/BUILD command), resize authority with two viewports (host is sole authority, a `pty_resize` frame echoes it), and session lifecycle (a `pty_exit` frame plus CLEAR/RESTART/KILL/OPEN controls, not present in the sketch at all).
- **Verification:** cross-checked against the shipped design doc, `docs/plan/done/1.20.0-terminal-and-remote-sync.md` §4, §4.7, §6a, which records the same three deviations.

**Decision:**
- **No action** — the sketch is fully superseded by the shipped design in `docs/arch/remote-views-roadmap.md` § Terminal View — BUILT. Kept here only as the historical baseline that section's "followed essentially as written" claim refers back to.
- **Cross-references:** `docs/arch/remote-views-roadmap.md` (current architecture — supersedes this sketch).

## Original sketch text (verbatim, moved out of `arch/` 2026-08-16)

* **Native side:** `portable-pty` spawns a shell; its output streams as `PTY_OUTPUT` frames, input arrives as `PTY_INPUT`. The PTY lives on the Mac (the only place a real shell makes sense); the phone is a dumb terminal surface.
* **Frontend:** `xterm.js` in a `TerminalView` component (same bundle, host + companion). Keystrokes are `PTY_INPUT` intents; rendered output is driven by `PTY_OUTPUT` frames.
* **SYNC-1 nuance:** terminal *output* is a data event (both screens see the same session — this is the point, a shared terminal). Terminal *input* keystrokes are forwarded as intents but are **not** echoed locally before the PTY echoes them — the PTY is the SSOT for what the terminal shows, exactly like every other bit of state. So two people watching the same terminal see identical output.
* **Security:** a remote shell over Tailscale is the highest-value target in the whole app. Gate Terminal View behind the paired-device token like everything else, and consider a per-session opt-in on the Mac before a companion may open a PTY. Decide the exact posture when building it.
* **NEVER BLOCK UI / cold-start PATH race:** the PTY spawn and every shell it runs are subprocess work — `spawn_blocking` and the shared PATH-resolution preamble (CLAUDE.md GLOBAL TAURI STACK) apply.
