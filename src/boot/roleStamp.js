// Seam T host role stamp — docs/plan/remote-control.md §9 (S-1).
//
// MUST be the FIRST import in main.js, before any import that transitively loads
// services/bridge.js. ES module imports evaluate depth-first in source order, so a first-position
// side-effect import runs before the App.vue subtree (which pulls in bridge.js via
// AppHeader → useRemoteControl → utils/tauri). bridge.js reads `window.__AKI_ROLE__` at
// module-eval time, so the marker must already be set by then.
//
// Why an external module and NOT an inline <head> script: the host webview's CSP is
// `script-src 'self'` (src-tauri/tauri.conf.json) with no 'unsafe-inline', so an inline script is
// CSP-blocked on the host — the exact place this must run. A same-origin module script is allowed.
//
// Tauri injects `__TAURI_INTERNALS__` into its OWN webview before any app script; a plain browser
// (the phone companion, served the same bundle by axum) never has it, so the marker stays unset and
// bridge.js defaults to companion — the safe direction (a companion can never be mis-stamped host).
if (typeof window !== 'undefined' && window.__TAURI_INTERNALS__) {
  window.__AKI_ROLE__ = 'host'
}
