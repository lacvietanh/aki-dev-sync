#!/usr/bin/env node
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { extname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SOURCE_EXTENSIONS = new Set(['.vue', '.css', '.js', '.mjs', '.ts']);
const DEFAULT_CONFIG = {
  exceptions: [],
  sfcResidents: [],
  duplicateExceptions: [],
};

function lineNumber(source, offset) {
  return source.slice(0, offset).split('\n').length;
}

function walk(directory) {
  const files = [];
  if (!statSync(directory).isDirectory()) return [directory];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.name === 'node_modules' || entry.name === 'target' || entry.name === 'dist' || entry.name === '.git') continue;
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...walk(path));
    else if (SOURCE_EXTENSIONS.has(extname(entry.name))) files.push(path);
  }
  return files;
}

function validException(exception) {
  return exception && exception.rule && exception.file && exception.owner && exception.reason;
}

function matchingException(finding, exceptions) {
  return exceptions.find((exception) => {
    if (!validException(exception) || exception.rule !== finding.rule) return false;
    if (exception.file !== finding.file) return false;
    return !exception.match || finding.value.includes(exception.match);
  });
}

function blank(text) {
  return text.replace(/[^\n]/g, ' ');
}

function balancedEnd(source, open) {
  let depth = 0;
  for (let i = open; i < source.length; i++) {
    if (source[i] === '(') depth++;
    else if (source[i] === ')' && --depth === 0) return i + 1;
  }
  return source.length;
}

/** A custom-property definition is where a literal belongs, and a `var(--x, literal)` fallback is not a second source of truth; neither counts as a hardcoded visual value. */
function maskTokenSites(source) {
  let masked = source.replace(/--[\w-]+\s*:[^;{}]*/g, blank);
  for (const match of [...masked.matchAll(/\bvar\(\s*--[\w-]+\s*,/g)]) {
    const end = balancedEnd(masked, match.index + 3);
    masked = masked.slice(0, match.index) + blank(masked.slice(match.index, end)) + masked.slice(end);
  }
  return masked;
}

export function scanSource(source, file = 'fixture.vue') {
  const findings = [];
  const addMatches = (rule, regex, valueIndex = 0) => {
    for (const match of source.matchAll(regex)) {
      findings.push({ rule, file, line: lineNumber(source, match.index), value: match[valueIndex] });
    }
  };

  if (extname(file) === '.vue') {
    addMatches('static-inline-style', /(?<![:@\w-])(?:style|container-style)\s*=\s*(["'])(?!\s*\{\{)[\s\S]*?\1/g);
    for (const match of source.matchAll(/(?<![:\w-])class\s*=\s*(["'])([\s\S]*?)\1/g)) {
      const classes = match[2];
      for (const arbitrary of classes.matchAll(/(?:^|\s)([^\s"']+-\[[^\]\n]+\])/g)) {
        findings.push({
          rule: 'arbitrary-class-value',
          file,
          line: lineNumber(source, match.index),
          value: arbitrary[1],
        });
      }
    }
  }

  if (['.vue', '.css', '.js', '.mjs', '.ts'].includes(extname(file))) {
    const visual = /(?:#[0-9a-fA-F]{3,8}\b|\b(?:rgb|rgba|hsl|hsla)\s*\([^)]*\))/g;
    for (const match of maskTokenSites(source).matchAll(visual)) {
      findings.push({ rule: 'hardcoded-visual-value', file, line: lineNumber(source, match.index), value: source.slice(match.index, match.index + match[0].length) });
    }
  }
  return findings;
}

function cssBlocks(source, file) {
  if (extname(file) === '.css') return [{ source, offset: 0 }];
  if (extname(file) !== '.vue') return [];
  return [...source.matchAll(/<style\b[^>]*>([\s\S]*?)<\/style>/gi)]
    .map((match) => ({ source: match[1], offset: match.index + match[0].indexOf(match[1], match[0].indexOf('>')) }));
}

/** Rules at brace depth 0 only: a selector inside `@media`/`@supports` is a variant of its base rule and a `@keyframes` step (`0%`, `from`) is not a selector, so neither can be a duplicate definition. */
function topLevelRules(css) {
  const clean = css.replace(/\/\*[\s\S]*?\*\//g, blank);
  const rules = [];
  let depth = 0;
  let start = 0;
  let quote = null;
  for (let i = 0; i < clean.length; i++) {
    const ch = clean[i];
    if (quote) {
      if (ch === '\\') i++;
      else if (ch === quote) quote = null;
    } else if (ch === '"' || ch === "'") quote = ch;
    else if (ch === '{') {
      if (depth === 0) rules.push({ prelude: clean.slice(start, i), start, bodyStart: i + 1 });
      depth++;
    } else if (ch === '}') {
      depth = Math.max(0, depth - 1);
      if (depth === 0) {
        start = i + 1;
        rules[rules.length - 1].bodyEnd = i;
      }
    } else if (ch === ';' && depth === 0) start = i + 1;
  }
  return rules;
}

function ruleNames(prelude) {
  const text = prelude.trim().replace(/\s+/g, ' ');
  if (/^@(?:-\w+-)?keyframes\b/.test(text)) return [text];
  if (text.startsWith('@')) return [];
  return text.split(',').map((item) => item.trim()).filter(Boolean);
}

function collectDefinitions(source, file) {
  const selectors = [];
  const tokens = [];
  for (const block of cssBlocks(source, file)) {
    for (const rule of topLevelRules(block.source)) {
      const line = lineNumber(source, block.offset + rule.start + rule.prelude.search(/\S|$/));
      for (const name of ruleNames(rule.prelude)) selectors.push({ name, file, line });
    }
    for (const rule of topLevelRules(block.source)) {
      if (!/^(?::root|html|@theme\b)/.test(rule.prelude.trim())) continue;
      const body = block.source.slice(rule.bodyStart, rule.bodyEnd ?? block.source.length);
      for (const match of body.matchAll(/(--[\w-]+)\s*:/g)) {
        tokens.push({ name: match[1], file, line: lineNumber(source, block.offset + rule.bodyStart + match.index) });
      }
    }
  }
  return { selectors, tokens };
}

function duplicates(items, { acrossFiles = false } = {}) {
  const byName = new Map();
  for (const item of items) byName.set(item.name, [...(byName.get(item.name) || []), item]);
  return [...byName.entries()]
    .filter(([, origins]) => (acrossFiles ? new Set(origins.map((origin) => origin.file)).size : origins.length) > 1)
    .map(([name, origins]) => ({ name, origins }));
}

export function audit(root, config = DEFAULT_CONFIG) {
  const base = resolve(root);
  const files = walk(base);
  const findings = [];
  const selectors = [];
  const tokens = [];
  const metrics = { sharedCssLines: 0, sfcCssLines: 0, sharedOrigins: [], sfcOrigins: [] };

  for (const absolute of files) {
    const file = relative(base, absolute).replaceAll('\\', '/');
    const source = readFileSync(absolute, 'utf8');
    findings.push(...scanSource(source, file));
    const definitions = collectDefinitions(source, file);
    selectors.push(...definitions.selectors);
    tokens.push(...definitions.tokens);
    if (extname(file) === '.css') {
      const lines = source.split('\n').length;
      metrics.sharedCssLines += lines;
      metrics.sharedOrigins.push({ file, lines });
    }
    for (const block of cssBlocks(source, file)) {
      if (extname(file) !== '.vue') continue;
      const lines = block.source.split('\n').length;
      metrics.sfcCssLines += lines;
      metrics.sfcOrigins.push({ file, lines });
    }
  }

  const exceptions = [];
  const violations = findings.filter((finding) => {
    const exception = matchingException(finding, config.exceptions || []);
    if (!exception) return true;
    exceptions.push({ ...exception, origin: `${finding.file}:${finding.line}` });
    return false;
  });
  const allowedDuplicates = new Map((config.duplicateExceptions || []).filter((entry) => entry.name && entry.owner && entry.reason).map((entry) => [entry.name, entry]));
  const duplicateSelectors = duplicates(selectors, { acrossFiles: true }).filter(({ name }) => !allowedDuplicates.has(name));
  for (const { name, origins } of duplicates(selectors, { acrossFiles: true })) {
    const allowed = allowedDuplicates.get(name);
    if (allowed) exceptions.push({ rule: 'duplicate-selector', origin: origins.map(({ file, line }) => `${file}:${line}`).join(', '), owner: allowed.owner, reason: allowed.reason });
  }
  const duplicateTokens = duplicates(tokens);
  const residents = config.sfcResidents || [];
  const residentFiles = new Set(residents.filter(validException).map((entry) => entry.file));
  const unclassifiedSfcOrigins = metrics.sfcOrigins.filter(({ file }) => !residentFiles.has(file));
  const architectureViolation = metrics.sfcCssLines > metrics.sharedCssLines && unclassifiedSfcOrigins.length > 0;

  return { metrics, violations, exceptions, duplicateSelectors, duplicateTokens, unclassifiedSfcOrigins, architectureViolation };
}

function loadConfig(root) {
  const path = join(root, 'scripts', 'ui-audit.config.json');
  try { return JSON.parse(readFileSync(path, 'utf8')); } catch (error) {
    if (error.code === 'ENOENT') return DEFAULT_CONFIG;
    throw error;
  }
}

function printOrigins(label, origins) {
  console.log(`${label}:`);
  if (!origins.length) console.log('  (none)');
  for (const origin of origins) console.log(`  ${origin.file}:${origin.line || origin.lines}${origin.lines ? ` (${origin.lines} lines)` : ''}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const root = resolve(process.argv[2] || join(fileURLToPath(new URL('.', import.meta.url)), '..'));
  const report = audit(root, loadConfig(root));
  console.log(`Shared CSS lines: ${report.metrics.sharedCssLines}`);
  printOrigins('Shared CSS origins', report.metrics.sharedOrigins);
  console.log(`SFC CSS lines: ${report.metrics.sfcCssLines}`);
  printOrigins('SFC CSS origins', report.metrics.sfcOrigins);
  console.log('Exceptions:');
  if (!report.exceptions.length) console.log('  (none)');
  for (const item of report.exceptions) console.log(`  ${item.rule} ${item.origin} — owner=${item.owner}; reason=${item.reason}`);
  console.log('Duplicate selectors:');
  if (!report.duplicateSelectors.length) console.log('  (none)');
  for (const item of report.duplicateSelectors) console.log(`  ${item.name}: ${item.origins.map((origin) => `${origin.file}:${origin.line}`).join(', ')}`);
  console.log('Duplicate tokens:');
  if (!report.duplicateTokens.length) console.log('  (none)');
  for (const item of report.duplicateTokens) console.log(`  ${item.name}: ${item.origins.map((origin) => `${origin.file}:${origin.line}`).join(', ')}`);
  for (const finding of report.violations) console.error(`${finding.file}:${finding.line} [${finding.rule}] ${finding.value}`);
  if (report.architectureViolation) {
    console.error(`SFC CSS exceeds shared CSS and ${report.unclassifiedSfcOrigins.length} SFC origin(s) lack an owner/reason classification.`);
  }
  if (report.violations.length || report.duplicateSelectors.length || report.duplicateTokens.length || report.architectureViolation) process.exitCode = 1;
}
