<!--
  Remote Control settings — the container the feature outgrew (docs/plan/remote-ingress-rework.md §6).

  Ingress mode, the Tailscale HTTPS row moved out of the AppHeader dropdown, the pairing link, and the paired-device list.
-->
<template>
  <BaseModal :show="show" @close="$emit('close')" container-style="max-width: 520px">
    <template #title>
      <i class="fa-solid fa-tower-broadcast mr-1"></i> Remote Control Settings
    </template>

    <div class="modal-body">
      <div class="rs-row">
        <span class="rs-label"><i class="fa-solid fa-route"></i> Ingress</span>
        <div class="rs-modes">
          <button
            v-for="m in MODES"
            :key="m.id"
            type="button"
            class="rs-mode"
            :class="{ on: mode === m.id }"
            :disabled="ingressBusy"
            :title="m.hint"
            @click="selectMode(m.id)">
            {{ m.label }}
          </button>
        </div>
      </div>

      <template v-if="mode === 'public'">
        <div class="rs-row">
          <span class="rs-label"><i class="fa-solid fa-globe"></i> Origin</span>
          <input
            v-model="originDraft"
            class="rs-input"
            spellcheck="false"
            placeholder="https://devsync.example.com"
            title="The HTTPS address your own edge (Cloudflare tunnel, reverse proxy) forwards to this Mac" />
        </div>
        <p class="rs-note">The device token is tied to the origin — each phone pairs again after a hostname change.</p>
      </template>

      <template v-else>
        <div class="rs-row" :title="httpsTitle">
          <span class="rs-label"><i class="fa-solid fa-lock"></i> HTTPS (PWA)</span>
          <label class="rs-toggle" :class="{ on: httpsEnabled, blocked: !!foreignTarget }">
            <input
              type="checkbox"
              :checked="httpsEnabled"
              :disabled="httpsBusy || !httpsAvailable || !!foreignTarget"
              @change="onToggleHttps" />
            {{ httpsEnabled ? 'On' : 'Off' }}
          </label>
        </div>
        <p v-if="foreignTarget" class="rs-note rs-warn">
          Another app already serves <code>/</code> on this tailnet name to {{ foreignTarget }}. Turn that one off, or switch to a public origin.
        </p>
        <p v-else-if="!httpsAvailable" class="rs-note">Tailscale is not available on this Mac.</p>
      </template>

      <div v-if="pairLink" class="rs-row rs-link-row" :title="pairLink">
        <span class="rs-label"><i class="fa-solid fa-link"></i> Pair link</span>
        <span class="rs-link u-select-text">{{ pairLink }}</span>
        <button type="button" class="rs-icon-btn" title="Copy the pairing link" @click="copyPairLink">
          <i class="fa-regular fa-copy"></i>
        </button>
      </div>
      <p v-if="pairLink" class="rs-note">Opening this link pairs the phone without typing the 6-digit code.</p>

      <div class="rs-devices">
        <div class="rs-row">
          <span class="rs-label"><i class="fa-solid fa-mobile-screen"></i> Paired devices</span>
          <span class="rs-count">{{ devices.length }}</span>
        </div>
        <div v-if="!devices.length" class="rs-note">No device is paired yet.</div>
        <div v-for="d in devices" :key="d.id" class="rs-device">
          <span class="rs-device-label" :title="d.label">{{ d.label || d.id }}</span>
          <span class="rs-device-date">{{ pairedOn(d.pairedAt) }}</span>
          <button type="button" class="rs-icon-btn rs-revoke" :title="'Revoke ' + (d.label || d.id)" @click="revoke(d)">
            <i class="fa-solid fa-ban"></i>
          </button>
        </div>
      </div>

      <p v-if="error" class="rs-note rs-warn">{{ error }}</p>
    </div>

    <div class="modal-footer">
      <button class="btn-secondary" @click="$emit('close')">Close</button>
      <button v-if="mode === 'public'" class="btn-save" :disabled="ingressBusy" @click="saveOrigin">
        <i class="fa-solid fa-floppy-disk mr-1"></i> Save
      </button>
    </div>
  </BaseModal>
</template>

<script setup>
import { ref, watch } from 'vue'
import BaseModal from './BaseModal.vue'
import { useRemoteControl } from '../../composables/useRemoteControl'
import { copyText } from '../../utils/clipboard'
import { Toast } from '../../store/projectStore'

const props = defineProps({ show: Boolean })
defineEmits(['close'])

const MODES = [
  { id: 'tailscale', label: 'Tailscale', hint: 'This app runs `tailscale serve` and manages the HTTPS mount itself' },
  { id: 'public', label: 'Public origin', hint: 'You run the edge (Cloudflare tunnel, reverse proxy); this app only stores and shows the address' },
]

const {
  httpsAvailable, httpsEnabled, httpsBusy, toggleHttps, foreignTarget,
  ingressMode, ingressOrigin, suggestedOrigin, ingressBusy, saveIngress, refreshIngress,
  pairLink, devices, refreshDevices, revokeDevice, error,
} = useRemoteControl()

// Mode and origin are edited locally so a half-typed hostname is never persisted; both are committed through saveIngress.
const mode = ref(ingressMode.value)
const originDraft = ref(ingressOrigin.value)

const httpsTitle = 'Serve over HTTPS via Tailscale so the phone can install this as a standalone app (PWA). Needs HTTPS certs enabled once in the Tailscale admin console.'

watch(() => props.show, async (open) => {
  if (!open) return
  await refreshIngress()
  await refreshDevices()
  mode.value = ingressMode.value
  originDraft.value = ingressOrigin.value || suggestedOrigin.value
})

async function selectMode(next) {
  mode.value = next
  if (next === 'public') {
    if (!originDraft.value) originDraft.value = suggestedOrigin.value
    return
  }
  await saveIngress('tailscale', ingressOrigin.value)
}

async function saveOrigin() {
  if (await saveIngress('public', originDraft.value.trim())) {
    Toast.fire({ icon: 'success', title: 'Ingress saved', text: ingressOrigin.value })
  }
}

async function onToggleHttps(e) {
  await toggleHttps()
  e.target.checked = httpsEnabled.value
}

async function copyPairLink() {
  if (await copyText(pairLink.value)) Toast.fire({ icon: 'success', title: 'Pair link copied' })
  else Toast.fire({ icon: 'info', title: pairLink.value })
}

async function revoke(d) {
  await revokeDevice(d.id)
  Toast.fire({ icon: 'success', title: `Revoked ${d.label || d.id}` })
}

function pairedOn(secs) {
  return secs ? new Date(secs * 1000).toLocaleDateString() : ''
}
</script>

<style scoped>
.rs-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 0;
  font-size: 12px;
}

.rs-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  min-width: 110px;
  color: var(--text-muted);
}

.rs-modes {
  display: inline-flex;
  gap: 4px;
}

.rs-mode {
  padding: 3px 10px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-control);
  background: var(--bg-tertiary);
  color: var(--text-muted);
  font-size: 11px;
  font-weight: 700;
  cursor: pointer;
}

.rs-mode.on {
  border-color: var(--accent-cyan);
  color: var(--accent-cyan);
}

.rs-mode:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.rs-input {
  flex: 1;
  min-width: 0;
  padding: 4px 8px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-control);
  background: var(--bg-tertiary);
  color: var(--text-light);
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
}

.rs-input:focus {
  outline: none;
  border-color: var(--accent-cyan);
}

.rs-toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--text-muted);
  font-size: 11px;
  font-weight: 700;
  cursor: pointer;
}

.rs-toggle.on {
  color: var(--accent-green);
}

.rs-toggle.blocked {
  color: var(--accent-amber);
  cursor: not-allowed;
}

.rs-link-row {
  align-items: baseline;
}

.rs-link {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--accent-cyan);
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 11px;
}

.rs-icon-btn {
  flex-shrink: 0;
  padding: 2px 6px;
  border: none;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
}

.rs-icon-btn:hover {
  color: var(--text-light);
}

.rs-revoke:hover {
  color: var(--accent-red);
}

.rs-note {
  margin: 0 0 4px;
  color: var(--text-darker);
  font-size: 11px;
}

.rs-warn {
  color: var(--accent-amber);
}

.rs-devices {
  margin-top: 8px;
  border-top: 1px solid var(--border-card);
}

.rs-count {
  padding: 1px 6px;
  border-radius: 8px;
  background: var(--bg-tertiary);
  color: var(--text-muted);
  font-size: 11px;
}

.rs-device {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 0;
  font-size: 12px;
}

.rs-device-label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-light);
}

.rs-device-date {
  flex-shrink: 0;
  color: var(--text-darker);
  font-size: 11px;
}
</style>
