// Companion pairing state — docs/plan/remote-control.md §7.1 (pairing), §9 (ENV-1).
//
// Boundary module: reads `isHost` so `PairingGate.vue` never has to. Without this the phone had no
// way to submit the 6-digit code at all — `services/bridge.js` set `connectionState = 'unpaired'`
// and nothing in the UI consumed it, so a fresh device could never get a token.
//
// Host: `needsPairing` is permanently false and the gate renders nothing.
import { ref, computed, watch } from 'vue'
import { isHost, connectionState, hasDeviceToken, pairDevice, clearDeviceToken } from '../services/bridge'

// True only until the phone has an ACCEPTED token — used to tune the gate's heading text, NOT to
// decide whether the code input shows. PairingGate renders the code form for EVERY not-ready
// companion state (see ROBUST-1 below), so a stale or failing token can never trap the user on a
// dead "Connecting…" screen with no way to re-enter a code.
const needsPairing = ref(!isHost && !hasDeviceToken())
const busy = ref(false)
const error = ref('')

// The gate the WHOLE dashboard mounts behind (App.vue). The host is always ready. A companion is
// ready ONLY once its relay socket is actually open — before that, every mirrored store is empty
// and every `invoke` the dashboard fires on mount would go out over a closed socket and reject
// ("send dropped, socket not open"). Gating the subtree, not just overlaying PairingGate on top of
// a live dashboard, is what stops that burst structurally: the components never mount early.
const ready = computed(() => isHost || connectionState.value === 'open')

if (!isHost) {
  watch(connectionState, (s) => {
    if (s === 'unpaired') {
      // 4001 covers BOTH "this token is not paired" and "remote control is switched off on the
      // host" — the relay does not distinguish them on the wire, so neither does this text.
      // ROBUST-1: a rejected token would fail identically on every reconnect, so drop it and fall
      // back to fresh code entry rather than looping on a dead credential.
      clearDeviceToken()
      needsPairing.value = true
      if (!busy.value) error.value = 'Not paired, or remote control is off on the Mac. Enter the code to pair.'
    } else if (s === 'open') {
      needsPairing.value = false
      error.value = ''
    }
  })
}

/** Exchange the 6-digit code for a device token. Resolves true when the token is stored. */
async function submitCode(rawCode) {
  const code = String(rawCode || '').trim()
  if (busy.value) return false
  if (!/^\d{6}$/.test(code)) {
    error.value = 'Enter the 6 digits shown on the Mac.'
    return false
  }
  busy.value = true
  error.value = ''
  try {
    await pairDevice(code)
    // `pairDevice` stores the token and calls connect(); the watch above hides the gate once the
    // socket actually opens, so a token the host accepts but a socket that never opens keeps the
    // gate visible instead of dropping the user into a dead, empty app.
    return true
  } catch (e) {
    error.value = String(e && e.message ? e.message : e)
    return false
  } finally {
    busy.value = false
  }
}

export function useCompanionPairing() {
  return { ready, needsPairing, busy, error, connectionState, submitCode }
}
