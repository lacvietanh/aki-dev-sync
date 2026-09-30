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

await test('a token definition and a var() fallback are not hardcoded visual values, a bare literal is', () => {
  const css = ':root { --a: #fff; --b: rgba(0, 0, 0, 0.5); }\n.x { color: var(--a, #000); background: var(--b, rgba(1, 2, 3, 0.4)); border: 1px solid #abc; }\n';
  assert.deepEqual(scanSource(css, 'x.css').map(({ value }) => value), ['#abc']);
});

await test('selectors inside @media and keyframe steps are not duplicate definitions, a repeated @keyframes name across files is', () => {
  const root = mkdtempSync(join(tmpdir(), 'aki-ui-audit-'));
  mkdirSync(join(root, 'src'));
  writeFileSync(join(root, 'src', 'shared.css'), '.a { color: red; }\n@media (max-width: 1px) { .a { color: blue; } }\n@keyframes spin { 0% { opacity: 0; } 100% { opacity: 1; } }\n');
  writeFileSync(join(root, 'src', 'View.vue'), '<style>\n@keyframes spin { 0% { opacity: 0; } 100% { opacity: 1; } }\n.a.b { color: green; }\n</style>\n');
  const names = audit(root).duplicateSelectors.map(({ name }) => name);
  assert.deepEqual(names, ['@keyframes spin']);
});

await test('a selector repeated only inside one file is a visible cascade, not a load-order duplicate', () => {
  const root = mkdtempSync(join(tmpdir(), 'aki-ui-audit-'));
  mkdirSync(join(root, 'src'));
  writeFileSync(join(root, 'src', 'shared.css'), '.a { color: red; }\n.a { background: blue; }\n');
  assert.equal(audit(root).duplicateSelectors.length, 0);
});

await test('a duplicate selector with an owner and reason is reported as an exception, one without is still a violation', () => {
  const root = mkdtempSync(join(tmpdir(), 'aki-ui-audit-'));
  mkdirSync(join(root, 'src'));
  writeFileSync(join(root, 'src', 'shared.css'), '.a { color: red; }\n.b { color: red; }\n');
  writeFileSync(join(root, 'src', 'View.vue'), '<style>\n.a { color: blue; }\n.b { color: blue; }\n</style>\n');
  const report = audit(root, { duplicateExceptions: [{ name: '.a', owner: 'UI', reason: 'Two intentionally different variants.' }, { name: '.b', owner: 'UI' }] });
  assert.deepEqual(report.duplicateSelectors.map(({ name }) => name), ['.b']);
  assert(report.exceptions.some(({ rule, reason }) => rule === 'duplicate-selector' && reason.includes('intentionally')));
});

await test('a component-scoped token overridden in a media query is not a second theme source, a token defined twice in :root is', () => {
  const root = mkdtempSync(join(tmpdir(), 'aki-ui-audit-'));
  mkdirSync(join(root, 'src'));
  writeFileSync(join(root, 'src', 'shared.css'), ':root { --a: 1px; --b: 2px; }\n:root { --b: 3px; }\n.x { --k: 1; }\n@media (max-width: 1px) { .x { --k: 2; } }\n');
  assert.deepEqual(audit(root).duplicateTokens.map(({ name }) => name), ['--b']);
});
