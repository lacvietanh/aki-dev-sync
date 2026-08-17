<!-- Terminal controls 3-dot drop-up (terminal-chrome-settings.md §8). Uses .open-popup/.popup-item from main.css with accessible <label>+<input type="checkbox"> rows (§6). -->
<template>
  <div class="open-popup-wrapper">
    <button
      ref="triggerEl"
      class="btn-tech btn-tech-secondary btn-terminal-action"
      :class="{ 'is-armed': chromeMenuArmed }"
      title="Terminal controls"
      popovertarget="terminal-chrome-menu"
    >
      <i class="fa-solid fa-ellipsis-vertical"></i>
    </button>
    <div class="open-popup" popover id="terminal-chrome-menu" aria-label="Terminal controls" @beforetoggle="onBeforeToggle">
      <!-- title sits on wrapper, not .popup-item: .popup-disabled sets pointer-events:none, swallowing hover tooltips (§8.3). -->
      <div v-for="row in chromeMenuRows" :key="row.key" :title="row.title">
        <label class="popup-item" :class="{ 'popup-disabled': row.locked }">
          <input
            type="checkbox"
            :checked="row.checked"
            :disabled="row.locked"
            @change="setChromePreference(row.key, $event.target.checked)"
          />
          <span>{{ row.label }}</span>
        </label>
      </div>
      <button
        v-if="showAllVisible"
        type="button"
        class="popup-item popup-run-row"
        @click="showAllChrome"
      >
        Show all
      </button>
    </div>
  </div>
</template>

<script setup>
import { ref } from 'vue'
import {
  chromeMenuArmed,
  chromeMenuRows,
  markChromeMenuSeen,
  setChromePreference,
  showAllChrome,
  showAllVisible,
} from '../../composables/useTerminalChrome'
import { rightDockActive } from '../../composables/useRightDockLayout'

const triggerEl = ref(null)

// Drop-UP for the bottom dock, drop-DOWN for the right dock, where there is no room above. Right-aligned to the trigger; the popover sits in the top layer, so these are viewport coordinates.
function onBeforeToggle(e) {
  if (e.newState !== 'open') return
  const rect = triggerEl.value?.getBoundingClientRect()
  if (!rect) return
  const margin = 8
  const style = e.target.style
  style.right = `${Math.max(margin, window.innerWidth - rect.right)}px`
  if (rightDockActive.value) {
    style.top = `${rect.bottom + 4}px`
    style.bottom = 'auto'
  } else {
    style.bottom = `${Math.max(margin, window.innerHeight - rect.top + 4)}px`
    style.top = 'auto'
  }
  markChromeMenuSeen()
}
</script>

<style scoped>
/* Armed toggle tint matches .pty-key.is-armed in TerminalView.vue. */
.btn-terminal-action.is-armed {
  color: var(--bg-primary);
  background: var(--accent-cyan);
  border-color: var(--accent-cyan);
}
</style>
