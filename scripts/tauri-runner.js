#!/usr/bin/env node
// Smart Tauri CLI runner: finds free port (>=1420) & injects --config devUrl for dev; passes through other subcommands unchanged.

import net from 'net';
import { execFileSync, spawn } from 'child_process';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import { resolve, dirname } from 'path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');
const tauriBin = resolve(root, 'node_modules/.bin/tauri');

// Companion relay ports from src-tauri/src/web_server.rs PORT: release, and the debug build that runs beside it. Vite/HMR must bind neither.
const RELEASE_RELAY_PORT = 1421;
const DEV_RELAY_PORT = 1431;
const RELAY_PORTS = [RELEASE_RELAY_PORT, DEV_RELAY_PORT];
// A dev build needs its own identifier, or the single-instance plugin hands it to the installed app (docs/research/agent-control-channel.md § R2).
const DEV_IDENTIFIER = 'aki.devsync.dev';

const [subcommand, ...rest] = process.argv.slice(2);

// Pre-flight: always run sync-version + check-env before executing any Tauri command.
try {
  execFileSync(process.execPath, [resolve(__dirname, 'sync-version.js')], { stdio: 'inherit', cwd: root });
  execFileSync(process.execPath, [resolve(__dirname, 'check-env.js')], { stdio: 'inherit', cwd: root });
} catch {
  process.exit(1);
}

function spawnTauri(args, env = process.env) {
  const proc = spawn(tauriBin, args, { stdio: 'inherit', cwd: root, env });
  proc.on('close', code => process.exit(code ?? 0));
  proc.on('error', err => { console.error('[tauri-runner] error:', err.message); process.exit(1); });
}

if (subcommand !== 'dev') {
  spawnTauri(subcommand ? [subcommand, ...rest] : rest);
} else {
  function isPortFree(port) {
    return new Promise(resolve => {
      const srv = net.createServer();
      srv.once('error', () => resolve(false));
      srv.once('listening', () => srv.close(() => resolve(true)));
      srv.listen(port, '127.0.0.1');
    });
  }

  async function findFreePort(base, range = 20) {
    for (let p = base; p < base + range; p++) {
      // A relay port is free before app launch but claimed by Axum later; Vite binding it kills remote control.
      if (RELAY_PORTS.includes(p)) continue;
      if (await isPortFree(p)) return p;
    }
    throw new Error(`No free port in range ${base}-${base + range - 1}`);
  }

  // TAURI_FORCE_PORT pins dev port (e.g. SSH port-forward) instead of auto-picking free port from 1420.
  const forcedPort = process.env.TAURI_FORCE_PORT ? parseInt(process.env.TAURI_FORCE_PORT, 10) : null;
  if (RELAY_PORTS.includes(forcedPort)) {
    console.error(`[tauri-runner] TAURI_FORCE_PORT=${forcedPort} is reserved by the companion relay — pick another port.`);
    process.exit(1);
  }
  const devPort = forcedPort || await findFreePort(1420);
  // devPort+1 is 1421 in the normal (devPort=1420) case — a relay port, so skip to devPort+2.
  const hmrPort = RELAY_PORTS.includes(devPort + 1) ? devPort + 2 : devPort + 1;
  console.log(`[tauri-runner] dev port=${devPort} hmr=${hmrPort}${forcedPort ? ' (forced)' : ''}`);

  // Runtime override via --config JSON merge, derived from tauri.conf.json so nothing is restated (Vite reads TAURI_DEV_PORT in vite.config.js).
  const baseConfig = JSON.parse(readFileSync(resolve(root, 'src-tauri/tauri.conf.json'), 'utf8'));
  const devCsp = baseConfig.app.security.csp.replaceAll(`:${RELEASE_RELAY_PORT}`, `:${DEV_RELAY_PORT}`);
  // npm run dev:agent: hidden so it never covers the owner's work, unthrottled so the hidden host keeps serving the relay (docs/ref/agent-control.md).
  const agentMode = Boolean(process.env.AKI_DEVSYNC_AGENT);
  const windows = agentMode
    ? baseConfig.app.windows.map(w => ({ ...w, visible: false, focus: false, backgroundThrottling: 'disabled' }))
    : baseConfig.app.windows;
  const configOverride = JSON.stringify({
    identifier: DEV_IDENTIFIER,
    build: { devUrl: `http://localhost:${devPort}` },
    app: { windows, security: { csp: devCsp } },
  });

  spawnTauri(['dev', '--config', configOverride, ...rest], {
    ...process.env,
    TAURI_DEV_PORT: String(devPort),
    TAURI_DEV_HMR_PORT: String(hmrPort),
  });
}
