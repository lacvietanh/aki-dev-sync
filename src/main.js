import "./boot/roleStamp"; // MUST stay first — stamps window.__AKI_ROLE__ before bridge.js reads it (docs/plan/remote-control.md §9)
import { createApp } from "vue";
import { isHost } from "./services/bridge";
import "@fortawesome/fontawesome-free/css/all.min.css";
import "./assets/main.css";
import App from "./App.vue";

createApp(App).mount("#app");

// Minimal PWA: register the network-passthrough service worker (public/sw.js) so the companion can
// install as a standalone app. Companion-only — never inside the Tauri host webview — and
// secure-context-only: over plain http on a LAN IP `serviceWorker`/`isSecureContext` are absent, so
// this is a no-op there and standalone falls to the manifest + apple-* meta tags (index.html).
if (!isHost && "serviceWorker" in navigator && window.isSecureContext) {
  window.addEventListener("load", () => {
    navigator.serviceWorker.register("/sw.js").catch(() => {});
  });
}
