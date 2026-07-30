#!/usr/bin/env bash
# Prints the settling test for ITEM-2 (#1) of the 2026.07.30 terminal/usage/UI council session:
# "terminal resize must sync both the remote browser and the app".
#
# RUN THIS ON THE MAC, paired with a phone/browser companion (Remote Control). This dev box cannot
# build or run Tauri (CLAUDE.local.md), so the actual verification can only happen here.
#
# Council finding (docs/plan/backlog-jul27.md WS-... / akiflow session
# 2026.07.30-0213-terminal-usage-ui-backlog, checklist.md ITEM-2): all four hypotheses for a real
# resize-sync defect were refuted by source reading — the host (Mac) is already the sole resize
# authority (src-tauri/src/pty.rs pty_resize -> real TIOCSWINSZ), the FRAME_PTY_RESIZE broadcast is
# structurally protected (src/services/hostInvoke.js COMPANION_ALLOWED_COMMANDS omits pty_resize,
# so a companion cannot even attempt to resize the shared PTY; src-tauri/src/web_server.rs's
# congestion-drop path never coalesces/drops pty_resize), and a fresh companion join already gets
# a correct-size snapshot via pushAllScrollbacks. No code fix was made because no static defect
# was found — this script exists to run the one check that source-reading cannot settle: whether
# the *repaint*, not the resize logic, silently fails on the companion's own font-zoom path.
#
# This script does NOT drive the app or a browser itself — nothing can script that from outside.
# It only prints the numbered steps; the interactive part is done by hand, once.
set -euo pipefail

cat <<'EOF'
── PTY resize propagation — settling test (#1 / ITEM-2) ────────────────────

Needs: the Mac running the app, plus ONE companion (paired phone, or a second browser tab open to
the app's Remote Control URL) both viewing the SAME PTY tab.

PART A — Mac window resize must reach the companion
  1. On the Mac, open an in-app terminal tab and confirm the SAME tab is visible on the companion.
  2. Resize the Mac app window (drag an edge, or toggle the terminal dock's own splitter).
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
