import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { execSync } from 'child_process';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '..');
const { version } = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf-8'));

const now = new Date();
const buildNum = process.env.BUILD_NUM || (String(now.getHours()).padStart(2, '0') + String(now.getMinutes()).padStart(2, '0'));

// Scan default-target, explicit arm-triple, and universal output dirs.
// `tauri build --target aarch64-apple-darwin` emits to the triple dir, not
// target/release — without it the rename silently no-ops.
const dmgDirs = [
  path.join(root, 'src-tauri/target/release/bundle/dmg'),
  path.join(root, 'src-tauri/target/aarch64-apple-darwin/release/bundle/dmg'),
  path.join(root, 'src-tauri/target/universal-apple-darwin/release/bundle/dmg'),
];

const archMap = { aarch64: 'arm', universal: 'uni' };

let renamed = false;

for (const dmgDir of dmgDirs) {
  if (!fs.existsSync(dmgDir)) continue;
  for (const file of fs.readdirSync(dmgDir)) {
    if (!file.endsWith('.dmg') || !file.startsWith('Aki Dev Sync_')) continue;
    const archMatch = file.match(/_([a-z0-9]+)\.dmg$/i);
    const rawArch = archMatch ? archMatch[1].toLowerCase() : 'universal';
    const arch = archMap[rawArch] ?? rawArch;
    const newName = `Aki-DevSync-v${version}.${buildNum}-${arch}.dmg`;
    const newPath = path.join(dmgDir, newName);
    fs.renameSync(path.join(dmgDir, file), newPath);
    console.log(`✅ Renamed: ${file} → ${newName}`);
    renamed = true;
    if (process.platform === 'darwin') {
      try { execSync(`open -R "${newPath}"`); } catch { /* not on macOS */ }
    }
  }
}

if (!renamed) {
  const appDirs = [
    path.join(root, 'src-tauri/target/release/bundle/macos'),
    path.join(root, 'src-tauri/target/aarch64-apple-darwin/release/bundle/macos'),
    path.join(root, 'src-tauri/target/universal-apple-darwin/release/bundle/macos'),
  ];
  for (const appDir of appDirs) {
    if (!fs.existsSync(appDir)) continue;
    for (const file of fs.readdirSync(appDir)) {
      if (!file.endsWith('.app')) continue;
      const appPath = path.join(appDir, file);
      console.log(`✅ Built .app: ${appPath}`);
      renamed = true;
      if (process.platform === 'darwin') {
        try { execSync(`open -R "${appPath}"`); } catch { /* not critical */ }
      }
    }
  }
}

if (!renamed) {
  console.log('⚠️ No matching DMG files found to rename.');
}
