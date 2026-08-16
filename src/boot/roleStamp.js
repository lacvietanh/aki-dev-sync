// Stamp window.__AKI_ROLE__ as 'host' when Tauri internals are present before bridge eval.
if (typeof window !== 'undefined' && window.__TAURI_INTERNALS__) {
  window.__AKI_ROLE__ = 'host'
}
