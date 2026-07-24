# Roadmap — Remote view subsystem (Explorer View + Terminal View)

> **Next round, NOT the current one.** The current round (`docs/plan/remote-control.md`) ships only
> the mirror/intent/native/pairing foundation plus a minimal `FileView` that renders `REPORT.html` on
> the phone. This file pins the *shape* of the two views that come after, so the foundation built now
> stays generic enough to carry them without rework.

## Why this exists

A phone controlling the Mac app has no Finder and no terminal. Two capabilities close that gap, both
**web-native** so the same code runs on the Mac window and the phone:

1. **Explorer View** — browse the project file tree, open a file into `FileView`.
2. **Terminal View** — a real terminal (xterm.js), VS Code-style, over the wire.

Both build on the current round's seams (state mirror, intent relay, native RPC, pairing). Neither
adds a second brain: the Mac still executes; the phone still mirrors + sends intents.

## What the current round must NOT foreclose

* `read_text_file(path)` and `FileView` are the **first members of a view subsystem** — keep them
  generic, not `REPORT.html`-specific. Explorer just points `FileView` at a different path.
* `FILE-1` (allow-list roots for reads) is the seed of the Explorer's permission boundary — build it
  now so Explorer inherits it.
* The relay already carries arbitrary framed messages; Terminal I/O is just two more message kinds
  (`PTY_INPUT` / `PTY_OUTPUT`) on the same socket — do not design the protocol so tightly that adding
  them means reworking the frame envelope.

## Explorer View (next round — sketch, not committed)

* **Native side:** `list_dir(path)` (async, `spawn_blocking`) → `[{name, kind:'file'|'dir', size}]`,
  confined to the same `FILE-1` allow-list. Directory listing is a *query* (seam N), not mirrored
  state — it's fetched on demand per navigation, and per SYNC-1 navigation is an **input event**
  (local to each screen), so two viewers browse independently.
* **Frontend:** a tree/list component; selecting a file calls `read_text_file` → `FileView`. Binary
  files (images) return a `data:` URI the same way icons do (ICON-1 pattern), never a fetch.
* **Open question for then:** editing/writing files from the phone (adds a `write_text_file` intent +
  the confirm-dialog pattern from §3.4) — decide when we get there; read-only first.

## Terminal View (next round — sketch, not committed)

* **Native side:** `portable-pty` spawns a shell; its output streams as `PTY_OUTPUT` frames, input
  arrives as `PTY_INPUT`. The PTY lives on the Mac (the only place a real shell makes sense); the
  phone is a dumb terminal surface.
* **Frontend:** `xterm.js` in a `TerminalView` component (same bundle, host + companion). Keystrokes
  are `PTY_INPUT` intents; rendered output is driven by `PTY_OUTPUT` frames.
* **SYNC-1 nuance:** terminal *output* is a data event (both screens see the same session — this is
  the point, a shared terminal). Terminal *input* keystrokes are forwarded as intents but are **not**
  echoed locally before the PTY echoes them — the PTY is the SSOT for what the terminal shows, exactly
  like every other bit of state. So two people watching the same terminal see identical output.
* **Security:** a remote shell over Tailscale is the highest-value target in the whole app. Gate
  Terminal View behind the paired-device token like everything else, and consider a per-session
  opt-in on the Mac before a companion may open a PTY. Decide the exact posture when building it.
* **NEVER BLOCK UI / cold-start PATH race:** the PTY spawn and every shell it runs are subprocess work
  — `spawn_blocking` and the shared PATH-resolution preamble (CLAUDE.md GLOBAL TAURI STACK) apply.

## Dependency order

```
current round: seams + pairing + FileView(REPORT.html)   ← must land first
        │
        ├── Explorer View   (list_dir + FileView on any path)
        └── Terminal View   (portable-pty + xterm.js + PTY_INPUT/OUTPUT)
```

Explorer and Terminal are independent of each other and can be built in parallel once the foundation
lands — but both depend on the current round's `FileView` + seam layer, so neither starts until that
is in and building clean on the Mac.
