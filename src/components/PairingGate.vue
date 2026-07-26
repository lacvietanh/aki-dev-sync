<!--
  Companion pairing screen — docs/plan/done/remote-control.md §7.1.

  Renders ONLY on a phone/browser companion that has no accepted token (`needsPairing`), and covers
  the app completely: behind it every panel would be empty anyway, since no state can be mirrored
  until the socket is paired. On the host it renders nothing at all.

  Deliberately full-viewport with large touch targets — the "Extreme Narrow" rule tunes the Mac
  window's density and does not apply to a phone screen that is showing one input.
-->
<template>
  <!-- Shown whenever the companion is NOT ready (socket not open). On the host `ready` is always
       true, so this renders nothing.

       ROBUST-1: the code-entry form is ALWAYS present here, in every not-ready state — no token,
       stale token, connecting, closed, errored. An earlier version hid the input whenever a token
       existed and only revealed it on a 4001 close; a phone whose socket hung at "connecting" or
       failed with a non-4001 error then had no way to re-enter a code and sat on a dead
       "Connecting…" screen forever. Keeping the form always visible makes that unreachable. -->
  <div v-if="!ready" class="pair-gate">
    <div class="pair-card">
      <i class="fa-solid fa-tower-broadcast pair-ic"></i>
      <h1>Aki Dev Sync</h1>

      <p class="pair-sub">
        <template v-if="needsPairing">Enter the 6-digit code shown on the Mac<br />(menu ☰ → Remote Control).</template>
        <template v-else>Reconnecting to the Mac — or enter the code again to re-pair.</template>
      </p>

      <form @submit.prevent="onSubmit">
        <input
          ref="codeInput"
          v-model="code"
          type="text"
          inputmode="numeric"
          pattern="[0-9]*"
          autocomplete="one-time-code"
          maxlength="6"
          placeholder="000000"
          class="pair-input"
          :disabled="busy"
          @input="onInput" />
        <button type="submit" class="pair-btn" :disabled="busy || code.length !== 6">
          {{ busy ? 'Pairing…' : 'Pair this device' }}
        </button>
      </form>

      <p v-if="error" class="pair-err">{{ error }}</p>
      <p class="pair-state">{{ stateLabel }}</p>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue';
import { useCompanionPairing } from '../composables/useCompanionPairing';

const { ready, needsPairing, busy, error, connectionState, submitCode } = useCompanionPairing();

const code = ref('');
const codeInput = ref(null);

// Strip anything that isn't a digit as it is typed: iOS autofill and some keyboards paste the code
// with spaces or a trailing character, which would fail the 6-digit check for no visible reason.
function onInput() {
  code.value = code.value.replace(/\D/g, '').slice(0, 6);
}

async function onSubmit() {
  const ok = await submitCode(code.value);
  if (!ok) code.value = '';
}

const stateLabel = computed(() => {
  switch (connectionState.value) {
    case 'connecting': return 'Connecting to the Mac…';
    case 'open': return 'Connected.';
    case 'error':
    case 'closed': return 'No connection to the Mac — is the app running?';
    default: return '';
  }
});

onMounted(() => {
  // The code input is present in every not-ready state now (ROBUST-1), so focus it whenever it
  // exists. On the host the gate renders nothing, so `codeInput` is null and this no-ops.
  codeInput.value?.focus();
});
</script>

<style scoped>
.pair-gate {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  background: #14161a;
  color: #e6e8ec;
}

.pair-card {
  width: 100%;
  max-width: 320px;
  text-align: center;
}

.pair-ic {
  font-size: 34px;
  color: #4ea1ff;
  margin-bottom: 14px;
}

.pair-card h1 {
  font-size: 19px;
  font-weight: 600;
  margin: 0 0 6px;
}

.pair-sub {
  font-size: 13px;
  line-height: 1.5;
  color: #98a1b0;
  margin: 0 0 22px;
}

.pair-input {
  width: 100%;
  box-sizing: border-box;
  padding: 14px 12px;
  font-size: 30px;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  letter-spacing: 8px;
  text-align: center;
  color: #e6e8ec;
  background: #1c1f26;
  border: 1px solid #2c313b;
  border-radius: 10px;
  outline: none;
}

.pair-input:focus {
  border-color: #4ea1ff;
}

.pair-btn {
  width: 100%;
  margin-top: 12px;
  padding: 14px;
  font-size: 15px;
  font-weight: 600;
  color: #fff;
  background: #2563eb;
  border: none;
  border-radius: 10px;
  cursor: pointer;
}

.pair-btn:disabled {
  background: #2c313b;
  color: #6b7482;
  cursor: default;
}

.pair-err {
  margin: 14px 0 0;
  font-size: 13px;
  color: #f87171;
}

.pair-state {
  margin: 10px 0 0;
  font-size: 12px;
  color: #6b7482;
  min-height: 16px;
}
</style>
