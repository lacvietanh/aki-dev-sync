import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { readFileSync } from "fs";

const host = process.env.TAURI_DEV_HOST;

// Port is set by scripts/tauri-runner.js before Vite starts.
// Falls back to 1420 when running `npm run dev` standalone (no tauri-runner).
const devPort = parseInt(process.env.TAURI_DEV_PORT || '1420', 10);
const hmrPort = parseInt(process.env.TAURI_DEV_HMR_PORT || String(devPort + 1), 10);

const now = new Date();
const buildDate = `${now.getFullYear()}.${String(now.getMonth() + 1).padStart(2, '0')}.${String(now.getDate()).padStart(2, '0')}`;
const buildNum = process.env.BUILD_NUM || (String(now.getHours()).padStart(2, '0') + String(now.getMinutes()).padStart(2, '0'));
const buildTime = `${buildNum.slice(0, 2)}:${buildNum.slice(2, 4)}`;
const { version } = JSON.parse(readFileSync('./package.json', 'utf-8'));

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [vue()],
  define: {
    '__BUILD_DATE__': JSON.stringify(buildDate),
    '__BUILD_TIME__': JSON.stringify(buildTime),
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
