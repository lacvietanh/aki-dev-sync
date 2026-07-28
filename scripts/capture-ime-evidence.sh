#!/usr/bin/env bash
# Prints the exact, copy-pasteable capture recipe for the Vietnamese-IME auto-restore residual bug
# (docs/research/terminal-vietnamese-ime-root-cause-3.md), and — if `pbcopy` is available — copies
# the console snippet straight to the clipboard so nothing has to be retyped inside Safari.
#
# RUN THIS ON THE MAC (the box that has OpenKey installed and can build/run the Tauri app). This
# dev box cannot build or run Tauri (`CLAUDE.local.md`), so the actual capture can only happen here.
#
# This script does NOT drive Safari or the app itself — nothing can script the Web Inspector from
# the outside. It only (a) starts the existing dev loop, (b) prints the numbered steps, and (c)
# stages the console snippet on the clipboard. The interactive part — opening the right Inspector
# window, typing the sequence, reading the dump — is still done by hand, once, in Safari.
#
# Investigation chain: docs/research/terminal-vietnamese-ime-root-cause-jul27.md -> -2.md -> -3.md.
# The `window.__akiIme` API this script quotes is defined in src/composables/useWkImeGuard.js.
set -euo pipefail

cat <<'EOF'
── Vietnamese IME auto-restore — evidence capture ──────────────────────────

0. This starts `npm run tauri dev` for you now (Ctrl-C to stop once you have your dump).
   Leave OpenKey's settings exactly as they are: spell-check ON, "Phục hồi phím với từ sai"
   (restore key on invalid word) ON. Do not change them for this capture.

1. Open the in-app terminal tab.

2. Safari -> Develop menu -> the machine's submenu -> the target titled "localhost"
   (devUrl http://localhost:1420).
   TRAP: do NOT open the entry titled "Main.html" (inspector-resource:///Main.html) — that is
   the Web Inspector's OWN UI, not the app. Picking it silently sets state nowhere the app can see.

3. In that inspector's console, confirm the target BEFORE anything else:

     __akiIme.status().page

   Must read 'Aki Dev Sync'. Any other value means you are in the wrong inspector window — go
   back to step 2.

4. Reset the ring for a clean repro:

     __akiIme.clear()

5. Click into the terminal. With OpenKey active exactly as configured, type this sequence,
   key by key, as fast as normal typing (do not press Enter yet):

     e x x i t

   (Telex reads the second `x` as an invalid word and auto-restores to raw "exit" — that restore
   is the bug: native apps and reportedly VS Code's terminal show "exit", this app shows only "e".)

6. Pull the evidence — this is a RETURNED value, so it prints regardless of console log filters
   or when the inspector attached:

     __akiIme.tail(20)

   (or `__akiIme.dump()` for the full ring if more context is wanted.)

7. In the returned array, find the keydown entry whose `key` is the two-letter ASCII restore
   payload (e.g. "ex") and read off its `keyCode` and `class` fields — see the discriminator
   table in this task's report / doc -3's new addendum section for what each value means.
   Also scan for any `beforeinput`/`input` entries between that keydown and the next keydown.

8. Optional A/B, to confirm the guard is even in play for this exact case (not assumed from the
   plain-syllable case it was validated against):

     localStorage['aki-ime-guard'] = 'off'

   then reopen the terminal tab and repeat step 5. If the symptom is UNCHANGED with the guard off,
   the guard is not where the loss happens at all — report that explicitly, it redirects the whole
   investigation rather than confirming any of the three ranked candidates.

9. Report back the exact `tail(20)` (or `dump()`) JSON output, plus whatever step 8 showed.

─────────────────────────────────────────────────────────────────────────────
EOF

# Only step 3 is staged, deliberately. Chaining `clear()` onto it would run the reset even when the
# answer to the target check is wrong — wiping the ring in whichever window you actually landed in,
# which is the one thing the target check exists to prevent.
SNIPPET="__akiIme.status().page"
if command -v pbcopy >/dev/null 2>&1; then
  printf '%s' "$SNIPPET" | pbcopy
  echo "(Step 3's console snippet is now on your clipboard: $SNIPPET)"
  echo
fi

echo "Starting the dev loop now (Ctrl-C when you have your dump)..."
npm run tauri dev
