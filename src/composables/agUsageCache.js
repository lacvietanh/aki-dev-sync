// @docs docs/arch/usage-antigravity.md
// @docs docs/plan/usage-monitor-entity-refactor.md §4
//
// The Antigravity per-account usage cache, owned here and nowhere else.
//
// Antigravity can switch logged-in accounts on the same machine, so a reading is cached per account
// and a dropdown (AgentUsage.vue header) lets the user inspect a previous account's cached usage
// while another account is live. Claude Code deliberately has no equivalent - exactly one account
// per machine by design (docs/arch/usage-claudecode.md).
//
// EVERY entry point here is scoped to ONE host's partition. That is the whole reason this module
// exists as a module: before the entity refactor this cache lived inside useAgentUsage.js and was
// additionally hand-parsed straight out of localStorage by AgentUsageSlot.vue, so there was no one
// place that could guarantee the scoping (design.A6 - no reaching into another module's internals).
//
// Store shape (v3):
//   { accounts: { "<host>|<email>:<sourceType>": { data, fetchedAt, host }, ... },
//     lastActiveEmailByHost: { "<host>": "<email>", ... } }
//
// WHY THE HOST IS IN THE KEY, not just metadata on the value (v2's shape). The same Google account
// is routinely signed in on the local Mac and on a remote host at once. Under v2 both wrote the one
// key `email:sourceType`, so whichever polled last overwrote the other's reading - and the loser's
// scope check then rejected its own former entry and rendered an empty card. Metadata cannot
// separate two records that share a key; only the key can.
const CACHE_KEY_V1 = 'aki-antigravity-usage-cache';       // legacy single-blob key
const CACHE_KEY_V2 = 'aki-antigravity-usage-cache-v2';    // per-account, host as advisory metadata
const CACHE_KEY = 'aki-antigravity-usage-cache-v3';       // per-account PER HOST
const EVICTION_TTL_SEC = 10 * 86400;                      // 10 days

/** `local` for the machine running the app, else the SSH host string. Never empty. */
export const LOCAL_HOST = 'local';

function accountKey(host, email, sourceType) {
  return `${host}|${email}:${sourceType || 'ide'}`;
}

/** Splits a v3 key back into its parts. Returns null for a key that is not in v3 form. */
function parseKey(k) {
  const bar = k.indexOf('|');
  if (bar < 0) return null;
  const host = k.slice(0, bar);
  const rest = k.slice(bar + 1);
  const colon = rest.lastIndexOf(':');
  if (colon < 0) return { host, email: rest, sourceType: 'ide' };
  return { host, email: rest.slice(0, colon), sourceType: rest.slice(colon + 1) };
}

function emptyStore() {
  return { accounts: {}, lastActiveEmailByHost: {} };
}

function saveStore(store) {
  try { localStorage.setItem(CACHE_KEY, JSON.stringify(store)); } catch (_) {}
}

// v2 -> v3: re-key every entry under the host it recorded. An entry with no `host` can only have
// come from a local instance - the remote AG probe was broken until 1.20.0 (agent_usage.rs's
// NODE_BIN_RESOLVER_PREAMBLE fix), so it never wrote a cache entry at all - which makes `local` the
// only honest value for a legacy entry, not a guess. No reading is dropped by the migration.
function migrateV2(v2) {
  const store = emptyStore();
  for (const [k, v] of Object.entries(v2.accounts || {})) {
    const host = v?.host || LOCAL_HOST;
    const email = v?.data?.email || k.split(':')[0];
    if (!email) continue;
    const sourceType = v?.data?.sourceType || (k.includes(':') ? k.split(':')[1] : 'ide');
    store.accounts[accountKey(host, email, sourceType)] = { ...v, host };
  }
  if (v2.lastActiveEmail) store.lastActiveEmailByHost[LOCAL_HOST] = v2.lastActiveEmail;
  return store;
}

function migrateV1(v1) {
  const store = emptyStore();
  const email = v1?.data?.email;
  if (email) {
    const sourceType = v1?.data?.sourceType || 'ide';
    store.accounts[accountKey(LOCAL_HOST, email, sourceType)] = { data: v1.data, fetchedAt: v1.fetchedAt, host: LOCAL_HOST };
    store.lastActiveEmailByHost[LOCAL_HOST] = email;
  }
  return store;
}

function loadStore() {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (parsed && parsed.accounts) return prune(parsed);
    }
    const v2raw = localStorage.getItem(CACHE_KEY_V2);
    if (v2raw) {
      const v2 = JSON.parse(v2raw);
      if (v2 && v2.accounts) {
        const store = migrateV2(v2);
        saveStore(store);
        localStorage.removeItem(CACHE_KEY_V2);
        return store;
      }
    }
    const v1raw = localStorage.getItem(CACHE_KEY_V1);
    if (v1raw) {
      const store = migrateV1(JSON.parse(v1raw));
      saveStore(store);
      localStorage.removeItem(CACHE_KEY_V1);
      return store;
    }
  } catch (_) {}
  return emptyStore();
}

/** TTL eviction, applied across every host - an expired entry is expired regardless of whose it is. */
function prune(store) {
  if (!store.lastActiveEmailByHost) store.lastActiveEmailByHost = {};
  const nowSec = Math.floor(Date.now() / 1000);
  let changed = false;
  for (const [k, v] of Object.entries(store.accounts)) {
    if (v?.fetchedAt && (nowSec - v.fetchedAt) > EVICTION_TTL_SEC) {
      delete store.accounts[k];
      changed = true;
    }
  }
  if (changed) saveStore(store);
  return store;
}

/** Every entry belonging to `host`, as [key, value] pairs. The one place the partition is defined. */
function entriesForHost(store, host) {
  return Object.entries(store.accounts).filter(([k]) => parseKey(k)?.host === host);
}

/**
 * Record a live reading. `dataObj` is one account, or a multi-account payload with `allAccounts`.
 * Writes only into `host`'s partition: another host's entry for the same email is never read,
 * rewritten, or deleted here (project rule: Regression Guard - Multi-entity State).
 */
export function persistAgAccount(dataObj, fetchedAt, host) {
  if (!dataObj || !host) return;
  const store = loadStore();
  const accounts = Array.isArray(dataObj.allAccounts) && dataObj.allAccounts.length > 0
    ? dataObj.allAccounts
    : [dataObj];

  for (const accObj of accounts) {
    const email = accObj?.email;
    if (!email) continue;
    const canonicalKey = accountKey(host, email, accObj.sourceType);

    // Drop this host's older keys for the same email (an account whose sourceType changed, or a
    // pre-migration unsuffixed key). Scoped to this host's partition by construction.
    for (const [k] of entriesForHost(store, host)) {
      if (parseKey(k).email === email && k !== canonicalKey) delete store.accounts[k];
    }

    const existing = store.accounts[canonicalKey];
    if (accObj.quotaSummary || !existing?.data?.quotaSummary || (fetchedAt || 0) >= (existing?.fetchedAt || 0)) {
      store.accounts[canonicalKey] = { data: accObj, fetchedAt, host };
    }
  }

  if (dataObj.email) store.lastActiveEmailByHost[host] = dataObj.email;
  saveStore(store);
}

/**
 * The cached entry for one account on one host, or null. `keyOrEmail` accepts either the display
 * key the dropdown hands back (`email:sourceType`) or a bare email.
 *
 * A miss returns null rather than falling back to another host's entry - the honest empty state.
 * That is the literal "chỉ hiện của máy tôi" report: an unscoped lookup used to hand the remote
 * card whichever entry matched the email first, which was frequently the local Mac's reading.
 */
export function loadAgAccount(keyOrEmail, host) {
  if (!keyOrEmail || !host) return null;
  const store = loadStore();
  const emailPart = keyOrEmail.split(':')[0];
  const direct = store.accounts[`${host}|${keyOrEmail}`];
  if (direct) return direct;
  for (const [k, v] of entriesForHost(store, host)) {
    if (parseKey(k).email === emailPart) return v;
  }
  return null;
}

/** The email whose reading was last live on `host`. Per-host: one host's account switch must not move another's fallback. */
export function lastActiveEmailFor(host) {
  return loadStore().lastActiveEmailByHost?.[host] || null;
}

/**
 * The account dropdown's contents for ONE host, newest first.
 *
 * Dedup (one row per email, keeping the newest record) runs strictly inside this host's partition.
 * Doing it across the whole store - as this did before the entity refactor - meant a fresher poll of
 * account X on host B `delete`d host A's record of that same account, destroying data the user could
 * still see on screen.
 */
export function listAgAccounts(host) {
  if (!host) return [];
  const store = loadStore();
  let changed = false;

  const bestByEmail = new Map();
  for (const [key, v] of entriesForHost(store, host)) {
    const parsed = parseKey(key);
    const email = v.data?.email || parsed.email;
    if (!email) continue;

    const existing = bestByEmail.get(email);
    if (!existing) {
      bestByEmail.set(email, { key, record: v });
      continue;
    }
    const existingFetched = existing.record.fetchedAt || 0;
    const currentFetched = v.fetchedAt || 0;
    if (currentFetched > existingFetched || (currentFetched === existingFetched && v.data?.quotaSummary && !existing.record.data?.quotaSummary)) {
      delete store.accounts[existing.key];
      bestByEmail.set(email, { key, record: v });
    } else {
      delete store.accounts[key];
    }
    changed = true;
  }

  if (changed) saveStore(store);

  const items = [];
  for (const [email, { key, record }] of bestByEmail.entries()) {
    items.push({
      // The dropdown's own handle for a row. Host-free on purpose: a slot already knows which host
      // it is looking at, and the pinned-account preference it persists should survive the slot
      // being pointed somewhere else and back.
      accountKey: `${email}:${record.data?.sourceType || parseKey(key).sourceType}`,
      email,
      sourceType: record.data?.sourceType || parseKey(key).sourceType,
      fetchedAt: record.fetchedAt,
    });
  }
  return items.sort((a, b) => (b.fetchedAt || 0) - (a.fetchedAt || 0));
}
