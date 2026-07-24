#!/usr/bin/env bash
# Expose the Aki Dev Sync companion over HTTPS on your tailnet so a phone can install it as a real
# STANDALONE PWA. Android Chrome only offers "Install" (a standalone WebAPK) from a secure context;
# plain http://100.x.y.z:1421 is NOT secure, so it can only make a browser shortcut. `tailscale
# serve` gives you a valid TLS cert for <machine>.<tailnet>.ts.net on 443 and proxies to the local
# http app — page, /ws (WebSocket) and /pair all ride the same HTTPS origin.
#
# RUN THIS ON THE MAC (not the remote dev box), with the app's Remote Control toggle turned ON so
# axum is listening on 127.0.0.1:1421.
#
# RUN ONCE — `--bg` persists the serve config in tailscaled across reboots; you do NOT re-run it each
# time you open the app. Each session you only need the app running with Remote Control ON. Re-run
# only after `tailscale serve reset`/off.
#
# `serve` (this) = tailnet-private, which is all you need for your own phone. `funnel` = PUBLIC
# internet exposure — you do NOT need it; if it's on, turn it off with `tailscale funnel reset`.
# Turning Funnel off does not affect this serve config.
#
# One-time prereqs in the Tailscale admin console (https://login.tailscale.com/admin/dns):
#   • MagicDNS: ON
#   • HTTPS Certificates: ON
#
# NOTE: the phone's device token is stored per-ORIGIN in the browser. Reaching the app on the new
# https://<machine>.ts.net origin is a different origin from http://100.x:1421, so you'll pair once
# more there (enter the 6-digit code). That's expected browser behaviour, not a bug.
set -euo pipefail

# Proxy https://<machine>.<tailnet>.ts.net/  ->  http://127.0.0.1:1421  (runs in the background).
# If your tailscale is older and rejects this form, use the explicit variant on the next line.
tailscale serve --bg http://127.0.0.1:1421
# tailscale serve --bg --https=443 http://127.0.0.1:1421   # older CLI fallback

echo
echo "Now serving over HTTPS. Open THIS url on the phone (no :1421 — it's 443/https):"
tailscale serve status

cat <<'EOF'

Then on the phone:
  • iPhone Safari  → Share → Add to Home Screen   (already standalone even over http, but https here too)
  • Android Chrome → ⋮ menu → Install app / Add to Home screen → opens standalone

To stop serving:
  tailscale serve --https=443 off        # or:  tailscale serve reset
EOF
