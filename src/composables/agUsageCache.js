// @docs docs/arch/usage-antigravity.md
// @docs docs/plan/done/usage-monitor-entity-refactor.md §4
//
// The Antigravity per-account usage cache, owned here and nowhere else.
//
// Antigravity can switch logged-in accounts on the same machine, so a reading is cached per account
// and a dropdown (AgentUsage.vue header) lets the user inspect a previous account's cached usage
// while another account is live. Claude Code deliberately has no equivalent - exactly one account
// per machine by design (docs/arch/usage-claudecode.md).
//
// EVERY entry point here is scoped to ONE host's partition. That is the whole reason this module
// exists as a module.
//
// THE ENTITY IS `(host, email)` - account is the unique truth.
//
// A Google account has ONE quota regardless of which local session (IDE, CLI, desktop) is used to
// read it. sourceType is transport metadata — it tells us HOW we connected, not WHICH quota we are
// looking at. Two entries for the same email on the same host are always the same quota, so storing
// them separately would produce duplicate rows in the dropdown and a false sense of two budgets.
//
// Store shape (v4):
//   { accounts: { "<host>|<email>": { data, fetchedAt, host }, ... },
//     lastActiveKeyByHost: { "<host>": "<email>", ... } }
//
// Migration: v3 used `<host>|<email>:<sourceType>` as the key. On first load, v3 entries are
// collapsed to `<host>|<email>`, keeping the entry with the largest fetchedAt per (host, email).
const CACHE_KEY_V3 = 'aki-antigravity-usage-cache-v3'; // email:sourceType in key - legacy
const CACHE_KEY    = 'aki-antigravity-usage-cache-v4'; // email only in key - current
const EVICTION_TTL_SEC = 10 * 86400; // 10 days

/** `local` for the machine running the app, else the SSH host string. Never empty. */
export const LOCAL_HOST = 'local';

/**
 * The host-free handle for one account entity: just the email address.
 * sourceType is no longer part of identity — one email = one account = one quota.
 */
export function entityKey(email) {
  return email || '';
}

function accountKey(host, email) {
  return `${host}|${email}`;
}

function parseKey(k) {
  const bar = k.indexOf('|');
  if (bar < 0) return null;
  const email = k.slice(bar + 1);
  if (!email) return null;
  return { host: k.slice(0, bar), email };
}

function emptyStore() {
  return { accounts: {}, lastActiveKeyByHost: {} };
}

function saveStore(store) {
  try { localStorage.setItem(CACHE_KEY, JSON.stringify(store)); } catch (_) {}
}

/** TTL eviction across all hosts. */
function prune(store) {
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

/** Every entry belonging to `host`, as [key, value] pairs. */
function entriesForHost(store, host) {
  return Object.entries(store.accounts).filter(([k]) => parseKey(k)?.host === host);
}

/**
 * v3 → v4: collapse `host|email:sourceType` keys to `host|email`.
 * When the same (host, email) has multiple sourceType variants, keep the freshest fetchedAt.
 */
function migrateFromV3(v3) {
  const store = emptyStore();
  for (const [k, v] of Object.entries(v3.accounts || {})) {
    const bar = k.indexOf('|');
    if (bar < 0) continue;
    const host = k.slice(0, bar);
    const rest = k.slice(bar + 1);
    // rest is `email:sourceType` or just `email`
    const colon = rest.lastIndexOf(':');
    const email = colon >= 0 ? rest.slice(0, colon) : rest;
    if (!email) continue;
    const newKey = accountKey(host, email);
    const existing = store.accounts[newKey];
    if (!existing || (v?.fetchedAt || 0) >= (existing?.fetchedAt || 0)) {
      store.accounts[newKey] = { data: v?.data, fetchedAt: v?.fetchedAt, host };
    }
  }
  // Migrate pointers: strip :sourceType suffix if present.
  for (const [host, pointer] of Object.entries(v3.lastActiveKeyByHost || {})) {
    if (!pointer) continue;
    const colon = pointer.lastIndexOf(':');
    const email = colon >= 0 ? pointer.slice(0, colon) : pointer;
    if (email) store.lastActiveKeyByHost[host] = email;
  }
  return store;
}

function loadStore() {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (parsed?.accounts) return prune(parsed);
    }
    const v3raw = localStorage.getItem(CACHE_KEY_V3);
    if (v3raw) {
      const v3 = JSON.parse(v3raw);
      if (v3?.accounts) {
        const store = migrateFromV3(v3);
        saveStore(store);
        localStorage.removeItem(CACHE_KEY_V3);
        return prune(store);
      }
    }
  } catch (_) {}
  return emptyStore();
}

/**
 * Record a live reading. `dataObj` is one account, or a multi-account payload with `allAccounts`.
 * Writes only into `host`'s partition.
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
    const key = accountKey(host, email);
    const existing = store.accounts[key];
    if (accObj.quotaSummary || !existing?.data?.quotaSummary || (fetchedAt || 0) >= (existing?.fetchedAt || 0)) {
      store.accounts[key] = { data: accObj, fetchedAt, host };
    }
  }

  if (dataObj.email) store.lastActiveKeyByHost[host] = dataObj.email;
  saveStore(store);
}

/**
 * The cached entry for ONE account on ONE host, or null.
 * Accepts a plain email or a legacy `email:sourceType` handle (sourceType is stripped).
 */
export function loadAgAccount(emailOrKey, host) {
  if (!emailOrKey || !host) return null;
  // Strip legacy :sourceType suffix if present.
  const colon = emailOrKey.lastIndexOf(':');
  const email = (colon > 0 && !emailOrKey.slice(colon + 1).includes('@'))
    ? emailOrKey.slice(0, colon)
    : emailOrKey;
  return loadStore().accounts[accountKey(host, email)] || null;
}

/** The email of the account last live on `host`, or null. */
export function lastActiveKeyFor(host) {
  return loadStore().lastActiveKeyByHost?.[host] || null;
}

/** Alias for `lastActiveKeyFor` — key and email are the same in v4. */
export function lastActiveEmailFor(host) {
  return lastActiveKeyFor(host);
}

/**
 * The account dropdown's contents for ONE host, newest first.
 * One row per email — no sourceType variants, because one email = one quota.
 */
export function listAgAccounts(host) {
  if (!host) return [];
  const items = [];
  for (const [key, record] of entriesForHost(loadStore(), host)) {
    const parsed = parseKey(key);
    if (!parsed?.email) continue;
    items.push({
      accountKey: parsed.email,
      email: parsed.email,
      sourceType: record.data?.sourceType || 'ide', // most recent session surface — icon metadata only, not identity
      fetchedAt: record.fetchedAt,
    });
  }
  return items.sort((a, b) => (b.fetchedAt || 0) - (a.fetchedAt || 0));
}
