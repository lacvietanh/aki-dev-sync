# Roadmap — Remote view subsystem (Explorer View + Terminal View)

> updated 2026-08-16 · v1.24.0

Relocated 2026-07-30 from `docs/plan/remote-views-roadmap.md`. It is a **target-state design, not an executable plan** — nobody is working it, it has no steps and no acceptance criteria, and half of it has already shipped, so `docs/plan/` was never the right home (`docs.A2`: `arch/` holds current *and* target state). It is also load-bearing beyond its own scope: the § "Terminal View — BUILT" section is where the host-sole-resize-authority rule is actually written down, which is what T-9 of `docs/plan/done/backlog-jul27.md` cited (tracked as T5 in `docs/plan/remaining-1.22.md`, closed 2026-08-15 — see that file's "Resolved (2026-08-15)" section). The pre-build sketch this section was built from moved to `docs/research/remote-views-terminal-sketch.md` — history, not current architecture.

> **Next round, NOT the current one.** The current round (`docs/plan/done/remote-control.md`) shipped the mirror/intent/native/pairing foundation and the backend half of file-view support: `read_text_file` (`src-tauri/src/web_server.rs`, FILE-1 allow-list) is registered and reachable via the generic `invoke` intent. **No frontend `FileView` component exists yet** — nothing in `src/` calls `read_text_file`, so `REPORT.html` does not currently render on the phone; the plan's §7.5 acceptance line describes intended, not shipped, behavior. This file pins the *shape* of the two views that come after, so the backend seam built now stays generic enough to carry them without rework once the frontend half lands.

## Why this exists

A phone controlling the Mac app has no Finder and no terminal. Two capabilities close that gap, both **web-native** so the same code runs on the Mac window and the phone:

1. **Explorer View** — browse the project file tree, open a file into `FileView`.
2. **Terminal View** — a real terminal (xterm.js), VS Code-style, over the wire.

Both build on the current round's seams (state mirror, intent relay, native RPC, pairing). Neither adds a second brain: the Mac still executes; the phone still mirrors + sends intents.

## What the current round must NOT foreclose

* `read_text_file(path)` is the **first member of a view subsystem** — keep it generic, not `REPORT.html`-specific, so a future `FileView` component (not yet built, see the callout above) can point it at any path.
* `FILE-1` (allow-list roots for reads) is the seed of the Explorer's permission boundary — build it now so Explorer inherits it.
* The relay already carries arbitrary framed messages; Terminal I/O is just two more message kinds (`PTY_INPUT` / `PTY_OUTPUT`) on the same socket — do not design the protocol so tightly that adding them means reworking the frame envelope.

## Explorer View (next round — sketch, not committed)

* **Native side:** `list_dir(path)` (async, `spawn_blocking`) → `[{name, kind:'file'|'dir', size}]`, confined to the same `FILE-1` allow-list. Directory listing is a *query* (seam N), not mirrored state — it's fetched on demand per navigation, and per SYNC-1 navigation is an **input event** (local to each screen), so two viewers browse independently.
* **Frontend:** a tree/list component; selecting a file calls `read_text_file` → `FileView`. Binary files (images) return a `data:` URI the same way icons do (ICON-1 pattern), never a fetch.
* **Open question for then:** editing/writing files from the phone (adds a `write_text_file` intent + the confirm-dialog pattern from §3.4) — decide when we get there; read-only first.

## Terminal View — BUILT in 1.20.0

> **Shipped (code complete, unverified on a Mac).** The sketch below was followed essentially as written: `portable-pty` on the Mac, `xterm.js` in a shared `TerminalView.vue`, `PTY_INPUT`/`PTY_OUTPUT` on the same socket, the PTY as SSOT with no local echo, gated behind the pairing token. Two things the sketch left open were decided during the build:
> * **Security posture** — no extra per-session opt-in on the Mac. A paired device could already run arbitrary commands via the DEV/BUILD command; a gate on the PTY alone would be theatre and would defeat the whole point (the phone is used when nobody is at the Mac). Recorded as a decision, not an omission — see the 1.20.0 plan T-7 and the CHANGELOG security note.
> * **Resize with two viewports** — the host is the sole resize authority; the companion never sizes the shared PTY. A third frame (`pty_resize`, host→companion only) echoes the authoritative size.
> * **Session lifecycle** — not in the sketch at all, and the first Mac run proved it is not optional: a shell that exits with nothing representing its death reads as a frozen app. A fourth frame (`pty_exit`) plus CLEAR/RESTART/KILL/OPEN controls were added in the same release. See §4.7.
>
> Full design and the deviations: `docs/plan/done/1.20.0-terminal-and-remote-sync.md` §4, §4.7 and §6a.
> Still deferred from here: multiple sessions/tabs, split panes, and redirecting the DEV/BUILD `Terminal.app` launchers into this view (they need per-project cwd + multi-session first).

### Original sketch — moved to `docs/research/remote-views-terminal-sketch.md`

The pre-build design this section was built from (native/frontend split, SYNC-1 semantics, security/blocking notes) now lives there as history; the "BUILT" section above is the current architecture.

## Dependency order

```
current round: seams + pairing + read_text_file backend (FileView frontend not yet built)
        │
        ├── Explorer View   (list_dir + FileView on any path)
        └── Terminal View   (portable-pty + xterm.js + PTY_INPUT/OUTPUT) — BUILT, see above
```

Explorer and Terminal were meant to be independent of each other, both depending only on the seam layer — Terminal View's actual build confirms that: it shipped in 1.20.0 with no `FileView` frontend ever having landed, so the "must land first" framing this diagram originally assumed does not hold. Explorer still needs `FileView` built before it can start.

