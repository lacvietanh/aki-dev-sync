import "./boot/roleStamp"; // MUST stay first — stamps window.__AKI_ROLE__ before bridge.js reads it (docs/plan/done/remote-control.md §9)
import { createApp } from "vue";
import { isHost } from "./services/bridge";
import "@fortawesome/fontawesome-free/css/all.min.css";
import "./assets/main.css";
import App from "./App.vue";

createApp(App).mount("#app");

// With dragDropEnabled off, a file or link dropped on a page that does not accept it makes WKWebView load it in place of the app; the page accepts every outside drop and discards it.
const isOutsideDrag = (e) => [...(e.dataTransfer?.types || [])].some((t) => t === "Files" || t === "text/uri-list");
for (const type of ["dragover", "drop"]) {
  window.addEventListener(type, (e) => {
    if (isOutsideDrag(e)) e.preventDefault();
  });
}

// Minimal PWA: register network-passthrough service worker on companion in secure context (public/sw.js).
if (!isHost && "serviceWorker" in navigator && window.isSecureContext) {
  window.addEventListener("load", () => {
    navigator.serviceWorker.register("/sw.js").catch(() => {});
  });
}
