#!/usr/bin/env bash
# Report WHO owns this node's https://<magicdns>:443/ mount, and whether Funnel is on for it.
#
# Read-only: runs only `tailscale status/serve status/funnel status` and never writes serve config.
# Two apps share one serve config on a Tailscale node — Aki Dev Sync (`serve` -> 127.0.0.1:1421) and
# aki-mcp-sv (`funnel` -> 127.0.0.1:9999) both default to https:443 path "/", so the last one to run
# owns it. Background: docs/research/remote-ingress-tailscale-conflict.md.
#
# RUN THIS ON MAC (the node that actually serves). Exit 0 always — this reports state, it does not judge it.
set -uo pipefail

TS_BIN=""
for p in /opt/homebrew/bin/tailscale /usr/local/bin/tailscale /Applications/Tailscale.app/Contents/MacOS/Tailscale; do
  [ -x "$p" ] && TS_BIN="$p" && break
done
[ -z "$TS_BIN" ] && TS_BIN="$(command -v tailscale || true)"
if [ -z "$TS_BIN" ]; then
  echo "tailscale: not installed (nothing is serving this node)"
  exit 0
fi

echo "tailscale binary: $TS_BIN"
"$TS_BIN" status --json 2>/dev/null | python3 -c '
import json,sys
try: s=json.load(sys.stdin)
except Exception: print("status: unreadable (is tailscaled running?)"); sys.exit()
print("node:", (s.get("Self") or {}).get("DNSName","?").rstrip("."), "| backend:", s.get("BackendState","?"))
'

"$TS_BIN" serve status --json 2>/dev/null | python3 -c '
import json,sys
try: c=json.load(sys.stdin)
except Exception: print("serve: no config (nothing mounted)"); sys.exit()
web=c.get("Web") or {}
if not web: print("serve: no web handlers")
for hostport,cfg in web.items():
    funnel = bool((c.get("AllowFunnel") or {}).get(hostport))
    for path,h in (cfg.get("Handlers") or {}).items():
        target=h.get("Proxy") or h.get("Path") or h.get("Text") or "?"
        owner=("aki-dev-sync" if ":1421" in str(target) else "aki-mcp-sv" if ":9999" in str(target) else "other")
        print("mount %s%s -> %s  [%s]  funnel=%s" % (hostport, path, target, owner, "on" if funnel else "off"))
'
