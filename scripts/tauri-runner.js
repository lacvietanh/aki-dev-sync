#!/usr/bin/env node
// Smart Tauri CLI runner:
// - For `dev`: finds a free port starting from 1420, sets TAURI_DEV_PORT for Vite,
//   and passes --config override to Tauri CLI so both sides agree on the port.
// - For all other subcommands (build, icon, etc.): passes through unchanged.
// Replaces the inline script chain in package.json "tauri" entry.

import net from 'net';
import { execFileSync, spawn } from 'child_process';
import { fileURLToPath } from 'url';
import { resolve, dirname } from 'path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');
const tauriBin = resolve(root, 'node_modules/.bin/tauri');

// Args passed by npm: `npm run tauri dev` → argv = ['dev', ...]
const [subcommand, ...rest] = process.argv.slice(2);

// Pre-flight: always run sync-version + check-env (same as the old inline chain).
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
  // Pass through to tauri CLI unchanged (build, icon, help, etc.)
  spawnTauri(subcommand ? [subcommand, ...rest] : rest);
} else {
  // Dev: find a free port first, then run tauri dev with port injected.
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
      if (await isPortFree(p)) return p;
    }
    throw new Error(`No free port in range ${base}-${base + range - 1}`);
  }

  const devPort = await findFreePort(1420);
  const hmrPort = devPort + 1;
  console.log(`[tauri-runner] dev port=${devPort} hmr=${hmrPort}`);

  // Tauri CLI: override devUrl at runtime so it matches the port Vite will bind.
  // Vite reads TAURI_DEV_PORT from env (set below) - see vite.config.js.
  // Using --config JSON merge instead of editing tauri.conf.json keeps that file clean.
  const configOverride = JSON.stringify({
    build: { devUrl: `http://localhost:${devPort}` },
  });

  spawnTauri(['dev', '--config', configOverride, ...rest], {
    ...process.env,
    TAURI_DEV_PORT: String(devPort),
    TAURI_DEV_HMR_PORT: String(hmrPort),
  });
}
