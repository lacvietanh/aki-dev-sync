#!/usr/bin/env bash
# Settling test for ITEM-2 (#1) of 2026.07.30 terminal/usage/UI council: "terminal resize must sync both remote browser and app".
# Run on macOS paired with companion (CLAUDE.local.md: dev box cannot build/run Tauri).
# Council finding: docs/plan/done/backlog-jul27.md (akiflow 2026.07.30-0213-terminal-usage-ui-backlog, checklist.md ITEM-2).
# Invariants verified: host is sole resize authority (src-tauri/src/pty.rs TIOCSWINSZ); FRAME_PTY_RESIZE protected (src/services/hostInvoke.js COMPANION_ALLOWED_COMMANDS, src-tauri/src/web_server.rs, pushAllScrollbacks snapshot).
# Purpose: verifies companion font-zoom repaint behavior (manual test protocol printing numbered steps).
set -euo pipefail

cat <<'EOF'
── PTY resize propagation — settling test (#1 / ITEM-2) ────────────────────

Needs: the Mac running the app, plus ONE companion (paired phone, or a second browser tab open to
the app's Remote Control URL) both viewing the SAME PTY tab.

PART A — Mac window resize must reach the companion
  1. On the Mac, open an in-app terminal tab and confirm the SAME tab is visible on the companion.
  2. Resize the Mac app window (drag an edge, or drag the terminal stack's own splitter).
  3. On the companion, confirm the terminal grid visibly reflows (more/fewer columns or rows) to
     match. This should happen automatically, no companion-side action needed.
  PASS = companion grid changes without you touching the companion.
  FAIL = companion grid stays the old size → real defect, reopen ITEM-2 with this evidence.

PART B — Companion's own font zoom must repaint without any resize()/fit() call
  4. On the companion ONLY, open its own devtools/inspector console (Safari Web Inspector on iOS,
     or the browser's own devtools if using a second browser tab — NOT the Mac app's console).
  5. Zoom the companion's terminal font (however that control is exposed on that surface — pinch
     zoom, a font-size control, or a keyboard shortcut if the companion UI has one).
  6. Watch the console while zooming. Confirm the glyphs visibly resize AND check whether any
     resize()/fit() call is logged (xterm's own resize/fit calls, or app-level equivalents — search
     the console output for "resize" or "fit").
  PASS = glyphs resize correctly, and no resize()/fit() call appears — this is expected: pure CSS
         font-size scaling repaints on its own, no Tauri/PTY involvement needed.
  FAIL = glyphs DO NOT resize (font-size changed but the canvas/grid looks stale, or characters are
         cut off/overlapping) → this is a ONE-LINE fix: call `term.refresh(0, term.rows - 1)` right
         after the font-size write on the companion's zoom handler. Report which surface failed
         (Safari/iOS vs a second browser tab) since the two use different render paths.

Report back: PART A pass/fail, PART B pass/fail + which failure mode if B fails, and which
companion surface you tested (paired phone vs second browser tab).
─────────────────────────────────────────────────────────────────────────────
EOF
