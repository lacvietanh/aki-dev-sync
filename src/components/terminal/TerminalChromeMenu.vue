<!-- Terminal controls 3-dot drop-up (terminal-chrome-settings.md §8). Uses .open-popup/.popup-item from main.css with accessible <label>+<input type="checkbox"> rows (§6). -->
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

// Position on open (visibility:hidden): drop-UP for bottom dock, drop-DOWN for right dock to fit viewport.
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
/* Scope hook for onDocPointerDown; styles come from .open-popup-wrapper in main.css. */
/* Armed toggle tint matches .pty-key.is-armed in TerminalView.vue. */
.btn-terminal-action.is-armed {
  color: var(--bg-primary);
  background: var(--accent-cyan);
  border-color: var(--accent-cyan);
}
</style>
