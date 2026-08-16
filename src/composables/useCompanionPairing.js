// Companion pairing state (docs/plan/done/remote-control.md §7.1, §9 ENV-1).
import { ref, computed, watch } from 'vue'
import { isHost, connectionState, hasDeviceToken, pairDevice, clearDeviceToken } from '../services/bridge'

// True until phone gets an accepted token; tunes gate heading without blocking code input.
const needsPairing = ref(!isHost && !hasDeviceToken())
const busy = ref(false)
const error = ref('')

// Ready gate for dashboard mounting: host is always ready; companion waits for open socket.
const ready = computed(() => isHost || connectionState.value === 'open')

if (!isHost) {
  watch(connectionState, (s) => {
    if (s === 'unpaired') {
      // Close 4001: rejected token on reconnect falls back to fresh code entry.
      clearDeviceToken()
      needsPairing.value = true
      if (!busy.value) error.value = 'This device is not paired with the Mac. Enter the code to pair.'
    } else if (s === 'host-off') {
      // Close 4002: remote control off on host; keep token while reconnecting.
      needsPairing.value = !hasDeviceToken()
      if (!busy.value) {
        error.value = needsPairing.value
          ? 'Remote control is off on the Mac. Turn it on there, then enter the code.'
          : 'Remote control is off on the Mac — waiting for it to come back on.'
      }
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
    // pairDevice stores token and reconnects; gate hides once socket opens.
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
