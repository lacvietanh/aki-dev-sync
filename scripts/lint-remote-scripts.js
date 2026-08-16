// Layer 2 POSIX sh defense for ssh host sh payloads (see docs/arch/usage-claudecode.md §3c and docs/research/claudecode-usage-FINAL.md).
// Remote scripts execute under POSIX sh (dash) where bashisms cause silent exit 2; checks: regex scan, dash -n, and shellcheck.


import { spawnSync } from 'node:child_process';
import { readFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const SCRIPTS_DIR = dirname(fileURLToPath(import.meta.url));

// Keep in sync with include_str!(... .sh) payloads in src-tauri/src/agent_usage/ and remote_shell.rs piped to ssh host sh.
const REMOTE_SCRIPTS = [
  'get-claudecode-usage.sh',
  'provision-claudecode.sh',
  'get-antigravity-usage.sh',
];

// Runtime bashisms missed by syntax-only dash -n that break dash (pipefail handled separately below).
const BASHISM_PATTERNS = [
  { re: /\bset\s+-o\s+(errexit|nounset|xtrace|noglob)\b/, msg: '`set -o <long-name>` is a bashism - use the short form (`set -e`/`-u`/`-x`/`-f`) for POSIX sh' },
  { re: /\[\[\s/, msg: '`[[ ... ]]` is a bashism - use `[ ... ]` (test) for POSIX sh' },
  { re: /<<<\s*/, msg: '`<<<` here-string is a bashism - use a heredoc or `printf ... |` for POSIX sh' },
  { re: /\bfunction\s+[A-Za-z_]\w*\s*(\(\s*\)\s*)?\{/, msg: '`function name {` keyword is a bashism - use `name() {` for POSIX sh' },
  { re: /\b[A-Za-z_]\w*\+=/, msg: '`+=` append is a bashism - use `var="${var}..."` for POSIX sh' },
  { re: /\b[A-Za-z_]\w*=\(\s*[^)]*\)/, msg: 'array assignment `var=( ... )` is a bashism - POSIX sh has no arrays' },
];

// Strips full-line comments to avoid regex false positives while preserving line count for error reporting.
function stripCommentLines(src) {
  return src.split('\n').map((l) => (/^\s*#/.test(l) ? '' : l)).join('\n');
}

// Unguarded set -o pipefail force-exits dash (exit 2); only approved idiom is: ( set -o pipefail ) 2>/dev/null && set -o pipefail
function findUnsafePipefail(code, report) {
  code.split('\n').forEach((line, i) => {
    if (/\bset\s+-o\s+pipefail\b/.test(line) && !/\(\s*set\s+-o\s+pipefail\s*\)/.test(line)) {
      report(`line ${i + 1}: unguarded \`set -o pipefail\` - special-builtin usage error force-exits dash (exit 2, silent), and \`2>/dev/null || true\` does NOT rescue it. Use: \`( set -o pipefail ) 2>/dev/null && set -o pipefail\``);
    }
  });
}

function which(bin) {
  const r = spawnSync(bin, ['--version'], { stdio: 'ignore' });
  return !(r.error && r.error.code === 'ENOENT');
}

const hasDash = which('dash');
const hasShellcheck = which('shellcheck');

let failed = false;
let fileFailed = false;
const note = (m) => console.log(`\x1b[2m   ${m}\x1b[0m`);
const fail = (file, m) => { failed = true; fileFailed = true; console.log(`\x1b[31m   ✗ ${file}: ${m}\x1b[0m`); };

console.log('\x1b[36m%s\x1b[0m', '── lint-remote-scripts: POSIX sh check for `ssh host sh` payloads ──');
if (!hasDash) note('dash not found - skipping `dash -n` syntax check (regex scan still runs)');
if (!hasShellcheck) note('shellcheck not found - skipping deep lint (regex scan still runs)');

for (const name of REMOTE_SCRIPTS) {
  const path = join(SCRIPTS_DIR, name);
  fileFailed = false;
  if (!existsSync(path)) { fail(name, 'file not found'); continue; }

  const src = readFileSync(path, 'utf8');
  const code = stripCommentLines(src);

  // 1. Regex bashism scan (on comment-stripped code)
  for (const { re, msg } of BASHISM_PATTERNS) {
    const m = code.match(re);
    if (m) {
      const line = code.slice(0, m.index).split('\n').length;
      fail(name, `line ${line}: ${msg}`);
    }
  }
  findUnsafePipefail(code, (m) => fail(name, m));

  // 2. dash -n
  if (hasDash) {
    const r = spawnSync('dash', ['-n', path], { encoding: 'utf8' });
    if (r.status !== 0) fail(name, `dash -n failed: ${(r.stderr || '').trim()}`);
  }

  // 3. shellcheck -s sh
  if (hasShellcheck) {
    const r = spawnSync('shellcheck', ['-s', 'sh', '-S', 'error', path], { encoding: 'utf8' });
    if (r.status !== 0) fail(name, `shellcheck (errors):\n${(r.stdout || r.stderr || '').trim()}`);
  }

  if (!fileFailed) console.log(`\x1b[32m   ✓ ${name}\x1b[0m`);
}

if (failed) {
  console.log('\x1b[31m%s\x1b[0m', '✗ Remote-script lint FAILED - a dash-incompatible script would silently die over `ssh host sh`.');
  console.log('\x1b[31m%s\x1b[0m', '  See docs/research/claudecode-usage-FINAL.md');
  process.exit(1);
}
console.log('\x1b[32m%s\x1b[0m', '✓ All remote scripts are POSIX-sh safe.');
