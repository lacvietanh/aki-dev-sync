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
// exists as a module: before the entity refactor this cache lived inside useAgentUsage.js and was
// additionally hand-parsed straight out of localStorage by AgentUsageSlot.vue, so there was no one
// place that could guarantee the scoping (design.A6 - no reaching into another module's internals).
//
// THE ENTITY IS `(host, email, sourceType)` - all three, always.
//
// Antigravity signs the SAME Google account into more than one session on one machine: the IDE
// (`ide`) and the Gemini Core desktop/CLI pair (`desktop_cli`). Those sessions have SEPARATE quotas,
// so they are separate entities that merely share an email - and `scripts/get-antigravity-usage.js`
// pushes both into one `allAccounts` payload. Every dedup, sweep and lookup in this file therefore
// compares the whole triple; matching on email alone renders one session's quota under the other's
// label, which is a WRONG NUMBER on the card the user reads to decide whether to keep working.
//
// Store shape (v3):
//   { accounts: { "<host>|<email>:<sourceType>": { data, fetchedAt, host }, ... },
//     lastActiveKeyByHost: { "<host>": "<email>:<sourceType>", ... } }
//
// WHY THE HOST IS IN THE KEY, not just metadata on the value (v2's shape). The same Google account
// is routinely signed in on the local Mac and on a remote host at once. Under v2 both wrote the one
// key `email:sourceType`, so whichever polled last overwrote the other's reading - and the loser's
// scope check then rejected its own former entry and rendered an empty card. Metadata cannot
// separate two records that share a key; only the key can.
//
// WHY THE POINTER IS A KEY, not an email. `lastActiveKeyByHost` used to hold a bare email
// (`lastActiveEmailByHost`), which cannot name one of two same-email entities - so the AG-offline
// fallback resolved it by "first match wins" and could show the CLI session's numbers under the IDE
// card. It now stores the full entity handle; a legacy bare email is upgraded on load only when it
// is unambiguous, and dropped otherwise (see `normalizeStore`).
const CACHE_KEY_V1 = 'aki-antigravity-usage-cache';       // legacy single-blob key
const CACHE_KEY_V2 = 'aki-antigravity-usage-cache-v2';    // per-account, host as advisory metadata
const CACHE_KEY = 'aki-antigravity-usage-cache-v3';       // per-account PER HOST
const EVICTION_TTL_SEC = 10 * 86400;                      // 10 days

/** `local` for the machine running the app, else the SSH host string. Never empty. */
export const LOCAL_HOST = 'local';

/** An account with no declared session type is the IDE one - the only kind AG had before Gemini Core. */
const DEFAULT_SOURCE_TYPE = 'ide';

/**
 * The host-free handle for one account entity: `email:sourceType`. This is what the dropdown hands
 * back and what a slot pins, so it is defined here beside the key it must round-trip through
 * (design.A1) - a slot already knows its host, and a pin should survive the slot being pointed
 * somewhere else and back.
 */
export function entityKey(email, sourceType) {
  return `${email}:${sourceType || DEFAULT_SOURCE_TYPE}`;
}

function accountKey(host, email, sourceType) {
  return `${host}|${entityKey(email, sourceType)}`;
}

/** Splits a v3 key back into its parts. Returns null for a key that is not in v3 form. */
function parseKey(k) {
  const bar = k.indexOf('|');
  if (bar < 0) return null;
  const host = k.slice(0, bar);
  const rest = k.slice(bar + 1);
  const colon = rest.lastIndexOf(':');
  if (colon < 0) return { host, email: rest, sourceType: DEFAULT_SOURCE_TYPE };
  return { host, email: rest.slice(0, colon), sourceType: rest.slice(colon + 1) };
}

function emptyStore() {
  return { accounts: {}, lastActiveKeyByHost: {} };
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
  if (v2.lastActiveEmail) store.lastActiveKeyByHost[LOCAL_HOST] = v2.lastActiveEmail;
  return normalizeStore(store);
}

function migrateV1(v1) {
  const store = emptyStore();
  const email = v1?.data?.email;
  if (email) {
    const sourceType = v1?.data?.sourceType || 'ide';
    store.accounts[accountKey(LOCAL_HOST, email, sourceType)] = { data: v1.data, fetchedAt: v1.fetchedAt, host: LOCAL_HOST };
    store.lastActiveKeyByHost[LOCAL_HOST] = entityKey(email, sourceType);
  }
  return store;
}

/**
 * Brings any stored blob up to the current invariants WITHOUT touching account records:
 *   1. `lastActiveKeyByHost` exists;
 *   2. every pointer in it is a full `email:sourceType` handle, never a bare email.
 *
 * A pre-fix v3 blob (and a v2 one) holds bare emails under `lastActiveEmailByHost`. A bare email is
 * upgraded only when this host holds exactly ONE session for it; with two (IDE + desktop_cli) the
 * pointer is dropped instead of guessed, because the guess would put one session's quota on the
 * other's card. The pointer is rewritten by the very next successful poll, so the cost of dropping
 * it is one poll interval of "no cached fallback" - and nothing else in the store is affected: this
 * function never adds, removes or edits an account record.
 */
function normalizeStore(store) {
  const legacy = store.lastActiveEmailByHost;
  const pointers = { ...(store.lastActiveKeyByHost || {}) };
  if (legacy && typeof legacy === 'object') {
    for (const [host, email] of Object.entries(legacy)) {
      if (!pointers[host] && email) pointers[host] = email;
    }
    delete store.lastActiveEmailByHost;
  }
  store.lastActiveKeyByHost = {};
  for (const [host, pointer] of Object.entries(pointers)) {
    if (!pointer) continue;
    if (pointer.includes(':')) { store.lastActiveKeyByHost[host] = pointer; continue; }
    const candidates = entriesForHost(store, host).filter(([k]) => parseKey(k).email === pointer);
    if (candidates.length === 1) store.lastActiveKeyByHost[host] = entityKey(pointer, parseKey(candidates[0][0]).sourceType);
  }
  return store;
}

function loadStore() {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      // prune BEFORE normalize: an expired record must not be what disambiguates a legacy pointer.
      if (parsed && parsed.accounts) return normalizeStore(prune(parsed));
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
    const sourceType = accObj.sourceType || DEFAULT_SOURCE_TYPE;
    const canonicalKey = accountKey(host, email, sourceType);

    // Alias sweep, scoped to THIS ONE entity: same host, same email, same sourceType, different key
    // string - i.e. an old unsuffixed key for the very record being written. It cannot reach a
    // sibling session, another account, or another host.
    //
    // It used to match on email alone, which made a two-session payload destroy itself: iteration 1
    // wrote `x@g.com:ide`, iteration 2 (`x@g.com:desktop_cli`) deleted it as "an older key for the
    // same email". Every poll left exactly one of the two on disk, and the card for the deleted one
    // then fell back to its sibling's numbers. `desktop_cli` is NOT a newer spelling of `ide` -
    // they are two live sessions with two separate quotas (project rule: Regression Guard -
    // Multi-entity State; the entity is the triple, so the sweep must compare the triple).
    for (const [k] of entriesForHost(store, host)) {
      const p = parseKey(k);
      if (k !== canonicalKey && p.email === email && p.sourceType === sourceType) delete store.accounts[k];
    }

    const existing = store.accounts[canonicalKey];
    if (accObj.quotaSummary || !existing?.data?.quotaSummary || (fetchedAt || 0) >= (existing?.fetchedAt || 0)) {
      store.accounts[canonicalKey] = { data: accObj, fetchedAt, host };
    }
  }

  // The primary snapshot names which session was live, so the pointer records the whole entity.
  if (dataObj.email) store.lastActiveKeyByHost[host] = entityKey(dataObj.email, dataObj.sourceType);
  saveStore(store);
}

/**
 * The cached entry for ONE entity on ONE host, or null.
 *
 * `keyOrEmail` is normally the full `email:sourceType` handle the dropdown hands back, in which case
 * the lookup is exact and a miss is simply a miss. There is deliberately NO email-only fallback:
 * returning the sibling session's record for a missing one is how an IDE-pinned card came to render
 * the CLI session's quota. A blank card is recoverable; a plausible wrong number is not, because the
 * user acts on it.
 *
 * A bare email (a pin stored before handles carried the session type) resolves only when this host
 * holds exactly one session for it - one candidate is knowledge, two is a coin toss.
 *
 * A miss also never falls back to another host's entry - the honest empty state. That is the literal
 * "chỉ hiện của máy tôi" report: an unscoped lookup used to hand the remote card whichever entry
 * matched the email first, which was frequently the local Mac's reading.
 */
export function loadAgAccount(keyOrEmail, host) {
  if (!keyOrEmail || !host) return null;
  const store = loadStore();
  const direct = store.accounts[`${host}|${keyOrEmail}`];
  if (direct) return direct;
  if (keyOrEmail.includes(':')) return null;
  const candidates = entriesForHost(store, host).filter(([k]) => parseKey(k).email === keyOrEmail);
  return candidates.length === 1 ? candidates[0][1] : null;
}

/** The full `email:sourceType` handle of the session last live on `host`, or null. Per-host: one host's account switch must not move another's fallback. */
export function lastActiveKeyFor(host) {
  return loadStore().lastActiveKeyByHost?.[host] || null;
}

/** Just the email of that session - for display and for matching against a live payload's `email`. */
export function lastActiveEmailFor(host) {
  const key = lastActiveKeyFor(host);
  return key ? key.split(':')[0] : null;
}

/**
 * The account dropdown's contents for ONE host, newest first. A pure read: it does not write, delete
 * or reorder anything in the store.
 *
 * One row per ENTITY, so an email signed into both the IDE and the desktop/CLI pair correctly shows
 * two rows with two quotas. There is no dedup step at all any more, because keys are unique per
 * `(host, email, sourceType)` by construction - dedup was only ever needed when the key was less
 * specific than the entity. What it used to do was worse than redundant: it kept the newest record
 * per EMAIL and `delete`d the rest, so listing the dropdown destroyed the sibling session's cached
 * reading (and, before the entity refactor, another host's). A function whose job is to list must
 * not be the thing that deletes; the TTL sweep in `prune` is the one place records expire.
 *
 * Identity comes from the KEY, never from `record.data` - the key is what `loadAgAccount` must
 * round-trip through, so a row advertising a handle derived from anything else could hand back a
 * handle that resolves to nothing.
 */
export function listAgAccounts(host) {
  if (!host) return [];
  const items = [];
  for (const [key, record] of entriesForHost(loadStore(), host)) {
    const { email, sourceType } = parseKey(key);
    if (!email) continue;
    items.push({
      // The dropdown's own handle for a row. Host-free on purpose: a slot already knows which host
      // it is looking at, and the pinned-account preference it persists should survive the slot
      // being pointed somewhere else and back.
      accountKey: entityKey(email, sourceType),
      email,
      sourceType,
      fetchedAt: record.fetchedAt,
    });
  }
  return items.sort((a, b) => (b.fetchedAt || 0) - (a.fetchedAt || 0));
}
