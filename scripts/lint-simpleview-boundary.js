// Architectural boundary guard (WI-6): ensures usePtyStream.js & ansiStrip.js never handle grid geometry, cursor addressing, or mount xterm.


import { readFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const SCRIPTS_DIR = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = join(SCRIPTS_DIR, '..');

const TARGET_FILES = [
  'src/composables/usePtyStream.js',
  'src/utils/ansiStrip.js',
];

const PATTERNS = [
  {
    re: /\bcols\b/,
    msg: "forbidden identifier 'cols' (SimpleView must not learn or handle grid geometry)",
  },
  {
    re: /\brows\b/,
    msg: "forbidden identifier 'rows' (SimpleView must not learn or handle grid geometry)",
  },
  {
    re: /@xterm\/xterm/,
    msg: "forbidden import/reference '@xterm/xterm' (SimpleView must not mount xterm)",
  },
  {
    re: /(?:\x1b|\\[xX]1[bB]|\\[uU]001[bB]|\\[0-7]{3})\\?\[H/i,
    msg: "forbidden cursor addressing pattern 'ESC[H'",
  },
  {
    re: /(?:\x1b|\\[xX]1[bB]|\\[uU]001[bB]|\\[0-7]{3})\\?\[\d*;\d*H/i,
    msg: "forbidden cursor addressing pattern 'ESC[<r>;<c>H'",
  },
];

// Strips full-line comments to avoid self-tripping pattern checks while preserving line count for error reporting.
function stripCommentLines(src) {
  return src
    .split('\n')
    .map((l) => (/^\s*(\/\/|\/\*|\*|#)/.test(l) ? '' : l))
    .join('\n');
}

let failed = false;

console.log('\x1b[36m%s\x1b[0m', '── lint-simpleview: checking SimpleView architectural boundaries ──');

for (const relPath of TARGET_FILES) {
  const fullPath = join(REPO_ROOT, relPath);
  if (!existsSync(fullPath)) {
    console.log(`\x1b[2m   – ${relPath} (missing, skipped)\x1b[0m`);
    continue;
  }

  const src = readFileSync(fullPath, 'utf8');
  const code = stripCommentLines(src);
  const lines = code.split('\n');
  let fileFailed = false;

  lines.forEach((line, i) => {
    for (const { re, msg } of PATTERNS) {
      if (re.test(line)) {
        failed = true;
        fileFailed = true;
        console.log(`\x1b[31m   ✗ ${relPath}:${i + 1}: ${msg}\x1b[0m`);
      }
    }
  });

  if (!fileFailed) {
    console.log(`\x1b[32m   ✓ ${relPath}\x1b[0m`);
  }
}

if (failed) {
  console.log('\x1b[31m%s\x1b[0m', '✗ SimpleView boundary lint FAILED — forbidden patterns detected.');
  process.exit(1);
}

console.log('\x1b[32m%s\x1b[0m', '✓ SimpleView boundary checks passed.');
