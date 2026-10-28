// @docs docs/arch/usage-antigravity.md
// @docs docs/arch/usage-claudecode.md §1
// @docs docs/plan/done/usage-monitor-entity-refactor.md §4
// Per-account usage cache, one per agent, scoped per host partition: several accounts take turns on one machine (AG account switch; Claude Code login slots), so readings are cached per (host, email) to inspect one account while another is live.
// Entity is `(host, email)`: one account = one quota across every surface; AG's sourceType is transport metadata, not identity (avoids duplicate dropdown rows/split budgets).
// Store shape: { accounts: { "<host>|<email>": { data, fetchedAt, host } }, lastActiveKeyByHost: { "<host>": "<email>" } }
const STORAGE_KEYS = {
  antigravity: 'aki-antigravity-usage-cache-v4',
  claudecode: 'aki-claudecode-usage-cache-v1',
};
// AG v3 keys were `<host>|<email>:<sourceType>`; they collapse to `<host>|<email>` keeping the freshest fetchedAt per (host, email).
const AG_CACHE_KEY_V3 = 'aki-antigravity-usage-cache-v3';
const EVICTION_TTL_SEC = 10 * 86400; // 10 days

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

function migrateFromAgV3(v3) {
  const store = emptyStore();
  for (const [k, v] of Object.entries(v3.accounts || {})) {
    const bar = k.indexOf('|');
    if (bar < 0) continue;
    const host = k.slice(0, bar);
    const rest = k.slice(bar + 1);
    const colon = rest.lastIndexOf(':');
    const email = colon >= 0 ? rest.slice(0, colon) : rest;
    if (!email) continue;
    const newKey = accountKey(host, email);
    const existing = store.accounts[newKey];
    if (!existing || (v?.fetchedAt || 0) >= (existing?.fetchedAt || 0)) {
      store.accounts[newKey] = { data: v?.data, fetchedAt: v?.fetchedAt, host };
    }
  }
  for (const [host, pointer] of Object.entries(v3.lastActiveKeyByHost || {})) {
    if (!pointer) continue;
    const colon = pointer.lastIndexOf(':');
    const email = colon >= 0 ? pointer.slice(0, colon) : pointer;
    if (email) store.lastActiveKeyByHost[host] = email;
  }
  return store;
}

function createAccountCache(agentId) {
  const storageKey = STORAGE_KEYS[agentId];

  function saveStore(store) {
    try { localStorage.setItem(storageKey, JSON.stringify(store)); } catch (_) {}
  }

  // TTL eviction across all hosts.
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

  function loadStore() {
    try {
      const raw = localStorage.getItem(storageKey);
      if (raw) {
        const parsed = JSON.parse(raw);
        if (parsed?.accounts) return prune(parsed);
      }
      const v3raw = agentId === 'antigravity' ? localStorage.getItem(AG_CACHE_KEY_V3) : null;
      if (v3raw) {
        const v3 = JSON.parse(v3raw);
        if (v3?.accounts) {
          const store = migrateFromAgV3(v3);
          saveStore(store);
          localStorage.removeItem(AG_CACHE_KEY_V3);
          return prune(store);
        }
      }
    } catch (_) {}
    return emptyStore();
  }

  // dataObj is one account or a payload carrying allAccounts; an account's own seenAt dates it, else the poll time.
  function persist(dataObj, fetchedAt, host) {
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
      const at = accObj.seenAt || fetchedAt;
      if (accObj.quotaSummary || !existing?.data?.quotaSummary || (at || 0) >= (existing?.fetchedAt || 0)) {
        // A reading without a tier (agy `/usage` before its statusline recorded one) is partial: the tier this account already showed stays.
        const knownTier = accObj.userTier == null ? existing?.data?.userTier : null;
        store.accounts[key] = { data: knownTier ? { ...accObj, userTier: knownTier } : accObj, fetchedAt: at, host };
      }
    }

    if (dataObj.email) store.lastActiveKeyByHost[host] = dataObj.email;
    saveStore(store);
  }

  // Cached entry for ONE account on ONE host (accepts plain email or legacy email:sourceType).
  function load(emailOrKey, host) {
    if (!emailOrKey || !host) return null;
    const colon = emailOrKey.lastIndexOf(':');
    const email = (colon > 0 && !emailOrKey.slice(colon + 1).includes('@'))
      ? emailOrKey.slice(0, colon)
      : emailOrKey;
    return loadStore().accounts[accountKey(host, email)] || null;
  }

  // Email of the account last live on host, or null.
  function lastActive(host) {
    return loadStore().lastActiveKeyByHost?.[host] || null;
  }

  // Account dropdown entries for ONE host (newest first, 1 row per email since 1 email = 1 quota).
  function list(host) {
    if (!host) return [];
    const items = [];
    for (const [key, record] of Object.entries(loadStore().accounts)) {
      const parsed = parseKey(key);
      if (!parsed?.email || parsed.host !== host) continue;
      items.push({
        accountKey: parsed.email,
        email: parsed.email,
        sourceType: record.data?.sourceType || 'ide', // AG's most recent session surface — icon metadata only, not identity
        fetchedAt: record.fetchedAt,
      });
    }
    return items.sort((a, b) => (b.fetchedAt || 0) - (a.fetchedAt || 0));
  }

  return { persist, load, lastActive, list };
}

const caches = {};

/** The per-account cache of one agent (`antigravity` | `claudecode`). */
export function accountCacheFor(agentId) {
  return caches[agentId] || (caches[agentId] = createAccountCache(agentId));
}
