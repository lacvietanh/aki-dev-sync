import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { execSync } from 'child_process';

// Fix __dirname for ES modules
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Read package.json explicitly since import attributes are still experimental in some node versions
const pkgPath = path.resolve(__dirname, '../package.json');
const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));

const version = pkg.version;
// We know Tauri will build 'Aki Dev Sync_<version>_aarch64.dmg'
const dmgDir = path.resolve(__dirname, '../src-tauri/target/release/bundle/dmg');

if (!fs.existsSync(dmgDir)) {
  console.log(`Directory not found: ${dmgDir}. Skipping rename.`);
  process.exit(0);
}

const files = fs.readdirSync(dmgDir);
let renamed = false;

for (const file of files) {
  if (file.endsWith('.dmg') && file.startsWith('Aki Dev Sync_')) {
    const archMatch = file.match(/_([a-z0-9]+)\.dmg$/);
    let arch = archMatch ? archMatch[1] : 'universal';
    if (arch === 'aarch64') {
      arch = 'arm';
    }
    
    const newName = `Aki-DevSync-v${version}-${arch}.dmg`;
    const newPath = path.join(dmgDir, newName);
    fs.renameSync(path.join(dmgDir, file), newPath);
    console.log(`✅ Renamed build artifact: ${file} -> ${newName}`);
    renamed = true;

    // Auto-reveal the renamed dmg file in Finder if on macOS
    if (process.platform === 'darwin') {
      try {
        execSync(`open -R "${newPath}"`);
        console.log(`📂 Revealed in Finder: ${newName}`);
      } catch (err) {
        console.error('Failed to open DMG in Finder:', err);
      }
    }
  }
}

if (!renamed) {
  console.log('⚠️ No matching DMG files found to rename.');
}
