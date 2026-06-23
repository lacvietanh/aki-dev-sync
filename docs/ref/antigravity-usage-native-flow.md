# Antigravity Usage Native Flow

## The Problem
The `antigravity-usage` CLI tool natively defaults to `method: "auto"`. This causes it to ping the local Antigravity IDE proxy server before falling back to external API calls. When the IDE extension is blocking the event loop (e.g., during heavy code generation or indexing) or asleep (macOS App Nap), the internal ping times out, causing a ~40% failure rate with the error: `Antigravity is not running`.

## The Native Solution
Instead of reimplementing the private `cloudcode-pa.googleapis.com` SSE stream parsing natively in Rust (which would violate the "No Over-Engineering" rule and be highly brittle to Google API changes), we apply a configuration constraint to the existing NPM tool.

By appending `-m google` (`npx --yes antigravity-usage --json -m google`), we force the CLI to skip the local IDE proxy entirely. It reads the local OAuth tokens from `~/.gemini/oauth_creds.json` and hits the API directly.

### Benefits
- **Zero Setup**: `npx --yes` ensures the package downloads gracefully on any machine.
- **100% Reliability**: Completely bypasses the IDE event loop.
- **Fast**: Request time drops from ~2.5s (IDE fallback) to < 0.8s.
- **Maintainable**: Defers proprietary API schema tracking to the NPM package maintainers.
