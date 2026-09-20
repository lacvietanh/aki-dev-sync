import assert from 'node:assert/strict';
import test from 'node:test';
import { selectHydratedTabs, tabIdOf } from '../../src/services/replayHydration.js';

// pty_list_tabs is sorted by tab id; fixtures mirror that shape.
const listing = (...ids) => ids.map((id) => ({ id }));
const LIMIT = 4;

await test('tabIdOf falls back to 0 for a tab without a numeric id', () => {
  assert.equal(tabIdOf({ id: 7 }), 7);
  assert.equal(tabIdOf({}), 0);
  assert.equal(tabIdOf(null), 0);
  assert.equal(tabIdOf({ id: 'x' }), 0);
});

await test('the active tab is always hydrated even when it sorts last beyond the budget', () => {
  const hydrated = selectHydratedTabs(listing(1, 2, 3, 4, 5, 6, 7, 8, 9, 10), { activeId: 10, limit: LIMIT });
  assert.equal(hydrated.size, LIMIT);
  assert.ok(hydrated.has(10), 'the tab the user is viewing must carry history');
});

await test('pinned tabs are hydrated after the active tab and before fill', () => {
  const hydrated = selectHydratedTabs(listing(1, 2, 3, 4, 5, 6), { activeId: 1, pinnedIds: [5, 6], limit: LIMIT });
  assert.deepEqual([...hydrated], [1, 5, 6, 2]);
});

await test('fill preserves pty_list_tabs order for the remaining budget', () => {
  const hydrated = selectHydratedTabs(listing(1, 2, 3, 4, 5, 6, 7, 8, 9, 10), { activeId: 1, limit: LIMIT });
  assert.deepEqual([...hydrated], [1, 2, 3, 4]);
});

await test('the burst never exceeds the limit, no matter how many tabs are open', () => {
  const many = listing(...Array.from({ length: 50 }, (_, i) => i + 1));
  const hydrated = selectHydratedTabs(many, { activeId: 25, limit: LIMIT });
  assert.equal(hydrated.size, LIMIT);
  assert.ok(hydrated.has(25));
});

await test('an active id absent from the listing is not force-added', () => {
  const hydrated = selectHydratedTabs(listing(1, 2, 3), { activeId: 99, limit: LIMIT });
  assert.deepEqual([...hydrated].sort((a, b) => a - b), [1, 2, 3]);
  assert.ok(!hydrated.has(99));
});

await test('a tab that is both active and pinned does not waste budget', () => {
  const hydrated = selectHydratedTabs(listing(1, 2, 3, 4, 5, 6), { activeId: 1, pinnedIds: [1, 2], limit: 3 });
  assert.deepEqual([...hydrated], [1, 2, 3]);
});

await test('all tabs are hydrated when the count is within budget', () => {
  const hydrated = selectHydratedTabs(listing(1, 2, 3), { activeId: 2, limit: LIMIT });
  assert.equal(hydrated.size, 3);
});

await test('degenerate inputs hydrate nothing', () => {
  assert.equal(selectHydratedTabs([], { activeId: 1, limit: LIMIT }).size, 0);
  assert.equal(selectHydratedTabs(listing(1, 2), { activeId: 1, limit: 0 }).size, 0);
  assert.equal(selectHydratedTabs(null, { activeId: 1, limit: LIMIT }).size, 0);
});
