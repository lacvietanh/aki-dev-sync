<!-- Terminal controls 3-dot drop-up (terminal-chrome-settings.md §8). Uses .open-popup/.popup-item from main.css with accessible <label>+<input type="checkbox"> rows (§6). -->
<template>
  <div class="open-popup-wrapper">
    <button
      class="btn-tech btn-tech-secondary btn-terminal-action"
      :class="{ 'is-armed': chromeMenuArmed }"
      title="Terminal controls"
      popovertarget="terminal-chrome-menu"
      style="anchor-name: --chrome-menu-anchor"
    >
      <i class="fa-solid fa-ellipsis-vertical"></i>
    </button>
    <div
      class="open-popup"
      :class="{ 'is-dropdown': rightDockActive }"
      style="position-anchor: --chrome-menu-anchor"
      popover
      id="terminal-chrome-menu"
      aria-label="Terminal controls"
      @beforetoggle="onOpen"
    >
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
import {
  chromeMenuArmed,
  chromeMenuRows,
  markChromeMenuSeen,
  setChromePreference,
  showAllChrome,
  showAllVisible,
} from '../../composables/useTerminalChrome'
import { rightDockActive } from '../../composables/useRightDockLayout'

// Positioning is CSS Anchor Positioning (main.css .open-popup / .is-dropdown), not JS - this only handles the side effect of opening.
function onOpen(e) {
  if (e.newState !== 'open') return
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
