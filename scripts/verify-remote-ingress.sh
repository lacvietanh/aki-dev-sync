#!/usr/bin/env bash
# Static/compile-time gate for docs/plan/remote-ingress-rework.md before the Mac hand-off (§10).
# Everything here is settled WITHOUT a live tailnet or phone (§8) - it does not touch Tailscale,
# does not start the app, and changes nothing. What still needs the Mac is listed in the plan §10.1.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

RS_SRC="src-tauri/src/web_server.rs"
JS_HOOK="src/composables/useRemoteControl.js"

failed=0
note() { echo "   $1"; }
pass() { echo "✓ $1"; }
fail() { echo "✗ $1"; failed=1; }

echo "── verify-remote-ingress: static checks (docs/plan/remote-ingress-rework.md §8) ──"

# 1. Rust compiles and the new RelayState/PairGate/ingress tests pass.
if command -v cargo >/dev/null 2>&1; then
  if cargo test --quiet --lib --manifest-path src-tauri/Cargo.toml web_server:: 2>&1 | tee /tmp/verify-remote-ingress-cargo.log | tail -20; then
    pass "cargo test --lib (web_server::*)"
  else
    fail "cargo test --lib (web_server::*) — see /tmp/verify-remote-ingress-cargo.log"
  fi
else
  note "cargo not found — Rust build/test boundary is the Mac only (CLAUDE.local.md); skip here."
fi

# 2. W1: the blanket `serve --https=443 off` command must not exist anymore — disable is always scoped with --set-path=/ and gated on mount ownership.
if grep -n -- '"--https=443", *"off"' "$RS_SRC" >/dev/null 2>&1; then
  fail "W1: unscoped 'serve --https=443 off' still present in $RS_SRC — must carry --set-path=/"
else
  pass "W1: no blanket 'serve --https=443 off' left in $RS_SRC"
fi

# 3. W1: both enable and disable route through the single mount_owner() decision point.
if grep -q 'fn mount_owner()' "$RS_SRC" && \
   grep -q 'let owner = mount_owner();' "$RS_SRC"; then
  pass "W1: set_tailscale_https consults mount_owner() before acting"
else
  fail "W1: set_tailscale_https no longer visibly routes through mount_owner()"
fi

# 4. W2: the pairing penalty must never disable the server anymore.
if awk '/async fn pair_handler/,/^}/' "$RS_SRC" | grep -q 'enabled.store(false'; then
  fail "W2: pair_handler still disables the server on a bad code"
else
  pass "W2: pair_handler never disables the server"
fi

# 5. W3: both new commands are registered in lib.rs (a missing grant/registration is a silent no-op).
LIB_RS="src-tauri/src/lib.rs"
for cmd in get_remote_ingress set_remote_ingress; do
  if grep -q "web_server::$cmd" "$LIB_RS"; then
    pass "lib.rs registers web_server::$cmd"
  else
    fail "lib.rs is missing web_server::$cmd in generate_handler! — silent no-op from the frontend"
  fi
done

# 6. IPC wire names: every camelCase field the Rust side serializes must appear verbatim on the JS side.
for field in pairLinkToken foreignTarget suggestedOrigin ingressMode ingressOrigin; do
  rs_hit=$(grep -c "rename = \"$field\"" "$RS_SRC")
  js_hit=$(grep -c "$field" "$JS_HOOK")
  if [ "$rs_hit" -ge 1 ] && [ "$js_hit" -ge 1 ]; then
    pass "wire name '$field' present on both Rust and JS sides"
  else
    fail "wire name '$field' mismatch — rust hits=$rs_hit js hits=$js_hit"
  fi
done

# 7. capabilities/default.json: confirmed by the plan as needing no entry for web_server::* commands (Tauri v2 gates plugin permissions, not commands in generate_handler!) — assert that premise still holds so a future Tauri upgrade that changes it doesn't go unnoticed.
CAPS="src-tauri/capabilities/default.json"
if grep -q 'web_server::' "$CAPS" 2>/dev/null; then
  note "capabilities/default.json now names web_server:: commands — the 'no grant needed' premise in the plan §8 has changed, re-check it"
else
  pass "capabilities/default.json still holds no web_server:: entries (unchanged premise)"
fi

echo
if [ "$failed" -eq 0 ]; then
  echo "✓ All static checks green. Remaining work needs the Mac: docs/plan/remote-ingress-rework.md §10.1."
else
  echo "✗ One or more static checks failed — see above."
fi
exit "$failed"
