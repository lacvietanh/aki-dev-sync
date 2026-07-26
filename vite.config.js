import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { readFileSync } from "fs";

const host = process.env.TAURI_DEV_HOST;

// Port is set by scripts/tauri-runner.js before Vite starts.
// Falls back to 1420 when running `npm run dev` standalone (no tauri-runner).
// Port the companion relay binds (src-tauri/src/web_server.rs). Vite must never sit on it.
const RELAY_PORT = 1421;
const devPort = parseInt(process.env.TAURI_DEV_PORT || '1420', 10);
if (devPort === RELAY_PORT) {
  throw new Error(`[vite] dev port ${RELAY_PORT} is reserved by the companion relay — the axum bind would fail and remote control would die silently.`);
}
// Guard the standalone `npm run dev` path too: devPort+1 is 1421 when devPort is 1420.
const rawHmrPort = parseInt(process.env.TAURI_DEV_HMR_PORT || String(devPort + 1), 10);
const hmrPort = rawHmrPort === RELAY_PORT ? rawHmrPort + 1 : rawHmrPort;

const now = new Date();
const buildNum = process.env.BUILD_NUM || (String(now.getHours()).padStart(2, '0') + String(now.getMinutes()).padStart(2, '0'));
const { version } = JSON.parse(readFileSync('./package.json', 'utf-8'));

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [vue()],
  define: {
    '__BUILD_TIME__': JSON.stringify(buildNum),
    '__APP_VERSION__': JSON.stringify(version),
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. port resolved by tauri-runner.js; strictPort so Vite fails fast on conflict
  server: {
    port: devPort,
    strictPort: true,
    host: host || false,
    // HMR stays DIRECT to Vite (docs/plan/done/remote-control.md §7.2): the Mac dev window loads Vite
    // on localhost and hot-reloads directly. The phone loads the SPA through axum's proxy on :1421
    // (PORT-1) but does NOT get live HMR in dev — manual refresh — since Vite is localhost-only.
    // An axum HMR websocket bridge was tried and removed: it forced even the Mac window's HMR
    // through the proxy and broke it (connection-refused spam). Reliable-first.
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: hmrPort,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
