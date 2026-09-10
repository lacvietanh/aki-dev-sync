#!/usr/bin/env bash
# Expose Aki Dev Sync over HTTPS on tailnet for standalone PWA installation (Android Chrome requires secure context for WebAPK; proxies TLS 443 to local 127.0.0.1:1421).
#
# RUN THIS ON MAC (not remote dev box) with Remote Control ON so Axum listens on 127.0.0.1:1421.
#
# RUN ONCE: --bg persists config across reboots in tailscaled; re-run only after `tailscale serve reset`/off.
#
# Tailnet-private (serve) vs public (funnel): only serve is needed; funnel is public exposure and should be off.
#
# Tailscale admin console prereqs (https://login.tailscale.com/admin/dns): MagicDNS ON, HTTPS Certificates ON.
#
# NOTE: Device tokens are per-origin in browser; accessing via https://<machine>.ts.net requires one-time pairing.
#
# CAUTION - the 443 "/" mount is per-node and shared: `tailscale serve` and `tailscale funnel` write the same
# config, so a sibling app (e.g. aki-mcp-sv's Funnel) can already hold it. The app itself now touches only its
# own handler; the `off` line below does NOT - it clears the node's whole 443 handler set, killing any sibling
# endpoint too. Check who owns it first: scripts/check-tailscale-mount.sh. Why: docs/research/remote-ingress-tailscale-conflict.md F1.
set -euo pipefail

# Proxy https://<machine>.<tailnet>.ts.net/ -> http://127.0.0.1:1421 in background (fallback: --https=443).
tailscale serve --bg http://127.0.0.1:1421
# tailscale serve --bg --https=443 http://127.0.0.1:1421

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
