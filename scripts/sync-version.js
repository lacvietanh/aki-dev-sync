import fs from 'fs';
import path from 'path';

const packageJsonPath = path.join(process.cwd(), 'package.json');
const cargoTomlPath = path.join(process.cwd(), 'src-tauri', 'Cargo.toml');

try {
  const packageData = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
  const version = packageData.version;

  if (!version) {
    console.error('❌ Could not find version in package.json');
    process.exit(1);
  }

  let cargoToml = fs.readFileSync(cargoTomlPath, 'utf8');

  // Replace version under [package]
  const newCargoToml = cargoToml.replace(
    /(\[package\][\s\S]*?version\s*=\s*)"[^"]+"/,
    `$1"${version}"`
  );

  if (cargoToml !== newCargoToml) {
    fs.writeFileSync(cargoTomlPath, newCargoToml, 'utf8');
    console.log(`✅ Synced Cargo.toml version to ${version}`);
  } else {
    console.log(`✅ Cargo.toml version is already ${version}`);
  }
} catch (err) {
  console.error('❌ Error syncing version:', err.message);
  process.exit(1);
}
