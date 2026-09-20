#!/usr/bin/env node
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { extname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SOURCE_EXTENSIONS = new Set(['.vue', '.css', '.js', '.mjs', '.ts']);
const DEFAULT_CONFIG = {
  exceptions: [],
  sfcResidents: [],
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
    addMatches('hardcoded-visual-value', /(?:#[0-9a-fA-F]{3,8}\b|\b(?:rgb|rgba|hsl|hsla)\s*\([^)]*\))/g);
  }
  return findings;
}

function cssBlocks(source, file) {
  if (extname(file) === '.css') return [{ source, offset: 0 }];
  if (extname(file) !== '.vue') return [];
  return [...source.matchAll(/<style\b[^>]*>([\s\S]*?)<\/style>/gi)]
    .map((match) => ({ source: match[1], offset: match.index }));
}

function collectDefinitions(source, file) {
  const selectors = [];
  const tokens = [];
  for (const block of cssBlocks(source, file)) {
    for (const match of block.source.matchAll(/(^|})\s*([^@{}][^{}]*)\s*\{/gm)) {
      for (const selector of match[2].split(',').map((item) => item.trim()).filter(Boolean)) {
        selectors.push({ name: selector, file, line: lineNumber(source, block.offset + match.index) });
      }
    }
    for (const match of block.source.matchAll(/(--[\w-]+)\s*:/g)) {
      tokens.push({ name: match[1], file, line: lineNumber(source, block.offset + match.index) });
    }
  }
  return { selectors, tokens };
}

function duplicates(items) {
  const byName = new Map();
  for (const item of items) byName.set(item.name, [...(byName.get(item.name) || []), item]);
  return [...byName.entries()]
    .filter(([, origins]) => origins.length > 1)
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
  const duplicateSelectors = duplicates(selectors);
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
