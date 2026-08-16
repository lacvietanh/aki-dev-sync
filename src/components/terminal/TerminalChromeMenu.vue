<!-- Terminal controls — 3-dot drop-up (docs/plan/done/terminal-chrome-settings.md §8). Reuses ProjectTable.vue's .open-popup/.popup-item drop-up model (promoted to main.css — this is its second call site, Rule of Three evidence for the extraction). Rows are real <label>+<input type="checkbox"> (S6): a checkbox-shaped menu built from .popup-item divs is not keyboard-reachable. -->
<template>
  <div class="open-popup-wrapper chrome-menu-wrapper" :class="{ 'is-open': open }" @keydown.esc.stop="onEscape">
    <button
      ref="triggerEl"
      class="btn-tech btn-tech-secondary btn-terminal-action"
      :class="{ 'is-armed': chromeMenuArmed }"
      title="Terminal controls"
      aria-haspopup="true"
      :aria-expanded="open"
      @click="toggleOpen"
    >
      <i class="fa-solid fa-ellipsis-vertical"></i>
    </button>
    <div class="open-popup" :style="menuStyle" aria-label="Terminal controls">
      <!-- `title` sits on this wrapper, not on `.popup-item`: `.popup-disabled` sets `pointer-events: none` on the locked tab-strip row, which would swallow hover and make its explanatory title unreadable if the title lived on that same element (§8.3). -->
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
import { onBeforeUnmount, ref, watch } from 'vue'
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
const open = ref(false)
const menuStyle = ref({})

// Measured on open, before the CSS transition makes the element visible — same trick
// ProjectTable.vue's positionPopup uses (the element is visibility: hidden, not display: none).
// Direction is context-dependent: bottom dock → drop-UP (button is near the bottom of the screen);
// right dock → drop-DOWN (button is near the top, so the drop-up formula would put the menu
// above the viewport).
function positionMenu() {
  const rect = triggerEl.value?.getBoundingClientRect()
  if (!rect) return
  const margin = 8
  if (rightDockActive.value) {
    menuStyle.value = {
      position: 'fixed',
      top: `${rect.bottom + 4}px`,
      right: `${Math.max(margin, window.innerWidth - rect.right)}px`,
      transformOrigin: 'top right',
    }
  } else {
    menuStyle.value = {
      position: 'fixed',
      bottom: `${Math.max(margin, window.innerHeight - rect.top + 4)}px`,
      right: `${Math.max(margin, window.innerWidth - rect.right)}px`,
      transformOrigin: 'bottom right',
    }
  }
}

function closeMenu() {
  open.value = false
}

function toggleOpen() {
  if (open.value) {
    closeMenu()
    return
  }
  positionMenu()
  open.value = true
  markChromeMenuSeen()
}

function onEscape() {
  closeMenu()
  triggerEl.value?.focus()
}

function onDocPointerDown(e) {
  if (!e.target.closest?.('.chrome-menu-wrapper')) closeMenu()
}

watch(open, (isOpen) => {
  if (isOpen) document.addEventListener('pointerdown', onDocPointerDown, true)
  else document.removeEventListener('pointerdown', onDocPointerDown, true)
})

onBeforeUnmount(() => document.removeEventListener('pointerdown', onDocPointerDown, true))
</script>

<style scoped>
/* .chrome-menu-wrapper carries no rules of its own — .open-popup-wrapper (main.css) already gives position/display, and this class exists only as the onDocPointerDown scope hook. */

/* Same tint the codebase already uses for an armed toggle (.pty-key.is-armed, TerminalView.vue) — reused here rather than a second definition, per the SSoT this file's other classes already obey. */
.btn-terminal-action.is-armed {
  color: var(--bg-primary);
  background: var(--accent-cyan);
  border-color: var(--accent-cyan);
}
</style>
