import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { audit, scanSource } from '../audit-ui-architecture.mjs';

const fixture = (name) => new URL(`../fixtures/ui-audit/${name}`, import.meta.url);

await test('11 optional-index expressions are not arbitrary class values', () => {
  const findings = scanSource(readFileSync(fixture('false-positive.vue'), 'utf8'), 'false-positive.vue');
  assert.equal(findings.filter(({ rule }) => rule === 'arbitrary-class-value').length, 0);
});

await test('literal arbitrary class and static inline style are detected', () => {
  const source = readFileSync(fixture('positive.vue'), 'utf8');
  const findings = scanSource(source, 'positive.vue');
  assert.deepEqual(findings.map(({ rule }) => rule).sort(), ['arbitrary-class-value', 'static-inline-style']);
  assert(findings.some(({ value }) => value === 'w-[123px]'));
});

await test('audit reports CSS counts, origins, duplicates, and missing classifications', () => {
  const root = mkdtempSync(join(tmpdir(), 'aki-ui-audit-'));
  mkdirSync(join(root, 'src'));
  writeFileSync(join(root, 'src', 'shared.css'), ':root { --accent: #fff; }\n.same { color: var(--accent); }\n');
  writeFileSync(join(root, 'src', 'View.vue'), '<template><div /></template>\n<style>\n:root { --accent: #000; }\n.same { color: red; }\n.local { color: blue; }\n.more { color: green; }\n</style>\n');
  const report = audit(root);
  assert(report.metrics.sharedCssLines > 0);
  assert(report.metrics.sfcCssLines > report.metrics.sharedCssLines);
  assert(report.metrics.sharedOrigins.some(({ file }) => file === 'src/shared.css'));
  assert(report.metrics.sfcOrigins.some(({ file }) => file === 'src/View.vue'));
  assert(report.duplicateSelectors.some(({ name }) => name === '.same'));
  assert(report.duplicateTokens.some(({ name }) => name === '--accent'));
  assert.equal(report.architectureViolation, true);
});

await test('owned SFC classification satisfies the ratio gate', () => {
  const root = mkdtempSync(join(tmpdir(), 'aki-ui-audit-'));
  mkdirSync(join(root, 'src'));
  writeFileSync(join(root, 'src', 'shared.css'), '.base {}\n');
  writeFileSync(join(root, 'src', 'View.vue'), '<style>\n.one {}\n.two {}\n.three {}\n</style>\n');
  const report = audit(root, { exceptions: [], sfcResidents: [{ rule: 'sfc-resident', file: 'src/View.vue', owner: 'UI', reason: 'Scoped third-party override.' }] });
  assert.equal(report.architectureViolation, false);
  assert.equal(report.unclassifiedSfcOrigins.length, 0);
});
