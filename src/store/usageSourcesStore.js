// Mirrored on/off state for the three usage monitors (AG local, CC local, CC remote).
//
// These flags used to live as component-local refs inside AgentUsageSection's useToggleableSource,
// so the mirror (which globs store/*.js) never saw them and a companion's power toggle only flipped
// the phone's own copy. Monitoring on/off is a MAC setting — moving the flags here makes them
// mirror host→companion, and `setSourceEnabled` (an action) makes a phone toggle run on the host
// and mirror back. See docs/feat/remote-control.md control matrix ("Power AG/CC/ccRemote").
import { ref } from 'vue'
import { action } from '../services/action'

// One-time legacy seed (kept from AgentUsageSection): ccRemote used to piggyback the single
// `aki-remote-mode-enabled` flag before the split. Must run before the refs below read their key.
if (localStorage.getItem('aki-src-ccremote-enabled') === null) {
  const old = localStorage.getItem('aki-remote-mode-enabled')
  if (old !== null) localStorage.setItem('aki-src-ccremote-enabled', old)
}

const KEYS = {
  ag: 'aki-src-ag-enabled',
  ccLocal: 'aki-src-cclocal-enabled',
  ccRemote: 'aki-src-ccremote-enabled',
}

function seed(key, dflt) {
  const v = localStorage.getItem(key)
  return v !== null ? v === 'true' : dflt
}

export const agEnabled = ref(seed(KEYS.ag, true))
export const ccLocalEnabled = ref(seed(KEYS.ccLocal, true))
export const ccRemoteEnabled = ref(seed(KEYS.ccRemote, true))

const REFS = { ag: agEnabled, ccLocal: ccLocalEnabled, ccRemote: ccRemoteEnabled }

/** Set one monitor's enabled flag. On the host: mutate the mirrored ref + persist the host's
 *  localStorage (→ mirrors to every screen). From a companion: ships an intent; the host runs this
 *  and the new value comes back through the mirror — the phone never writes its own copy. */
export const setSourceEnabled = action('usageSourcesStore.setSourceEnabled', (key, value) => {
  const r = REFS[key]
  if (!r) return
  r.value = !!value
  localStorage.setItem(KEYS[key], String(!!value))
})
