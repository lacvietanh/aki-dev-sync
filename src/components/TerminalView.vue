<!-- In-app terminal: role-agnostic xterm.js mount supporting tabs and remote resize authority. -->
<template>
  <div class="pty-terminal">
    <div ref="mountEl" class="pty-terminal-mount" @click="onMountClick"></div>
    <!-- Prevent button focus to keep xterm textarea focused and prevent soft keyboard dismissal. -->
    <div v-if="ptyApi?.showKeyRow" class="pty-key-row">
      <!-- Independently toggleable key row and font zoom controls. -->
      <template v-if="chromeVisible.keyRow">
        <button
          v-for="k in KEY_ROW"
          :key="k.title"
          class="pty-key"
          :title="k.title"
          :class="{ 'is-armed': !!k.arms && !!ptyApi?.pendingModifiers?.value?.[k.arms] }"
          @mousedown.prevent
          @touchstart.prevent="onKeyTouch(k)"
          @click="onKeyClick(k)"
        >
          <span v-if="k.label" class="pty-key-label">{{ k.label }}</span>
          <i v-else class="fa-solid" :class="k.icon"></i>
        </button>
      </template>
      <!-- Font zoom controls rendered inside key row for touch/companion surfaces. -->
      <span v-if="chromeVisible.keyRow && chromeVisible.textSize" class="pty-key-sep" aria-hidden="true"></span>
      <template v-if="chromeVisible.textSize">
        <button class="pty-key" title="Smaller text" @mousedown.prevent @click="zoomOutTerminalFont">
          <i class="fa-solid fa-magnifying-glass-minus"></i>
        </button>
        <button
          class="pty-key"
          :class="{ 'is-armed': terminalFontScale !== 1 }"
          :title="`Reset text size (now ${Math.round(terminalFontScale * 100)}%)`"
          @mousedown.prevent
          @click="resetTerminalFont"
        >
          <span class="pty-key-label">{{ Math.round(terminalFontScale * 100) }}%</span>
        </button>
        <button class="pty-key" title="Larger text" @mousedown.prevent @click="zoomInTerminalFont">
          <i class="fa-solid fa-magnifying-glass-plus"></i>
        </button>
      </template>
      <!-- Request temporary resize authority over shared PTY. -->
      <span class="pty-key-sep" aria-hidden="true"></span>
      <button class="pty-key" title="Fit terminal to my screen" @mousedown.prevent @click="onRequestFitToMe">
        <i class="fa-solid fa-expand"></i>
      </button>
    </div>
    <!-- Host reclaim overlay shown when companion holds resize authority. -->
    <button
      v-if="ptyApi?.showReclaimPill?.value"
      class="pty-resize-owner-pill"
      title="Reclaim this terminal's size for your own window"
      @mousedown.prevent
      @click="onReclaimResize"
    >
      <i class="fa-solid fa-mobile-screen-button" aria-hidden="true"></i>
      <span>Phone controls size, tap to reclaim</span>
    </button>
    <!-- Compose row: text input field for IME composition before sending to PTY. -->
    <div v-if="chromeVisible.compose" class="pty-compose-row">
      <!-- Textarea allows multi-line input with Shift+Enter while auto-sizing to content. -->
      <textarea
        ref="composeInputEl"
        v-model="composeText"
        rows="1"
        class="pty-compose-input"
        placeholder="message…"
        autocapitalize="off"
        autocomplete="off"
        autocorrect="off"
        spellcheck="false"
        @keydown="onComposeKeydown"
        @compositionend="onComposeCompositionEnd"
      ></textarea>
      <button class="pty-key pty-compose-send" title="Send" @mousedown.prevent @click="onComposeSend">
        <i class="fa-solid fa-paper-plane"></i>
      </button>
    </div>
  </div>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { usePtyTerminal } from '../composables/usePtyTerminal'
import { useTerminalTextDrain, POST_COMPOSITION_MS } from '../composables/useTerminalTextDrain'
import { chromeVisible } from '../composables/useTerminalChrome'
import { renameTerminalTab, reclaimResizeAuthority } from '../store/terminalTabsStore'
import {
  terminalFontScale,
  zoomInTerminalFont,
  zoomOutTerminalFont,
  resetTerminalFont,
} from '../composables/useTerminalFont'

// Multi-tab props: tabId identifies the session; active toggles visibility without unmounting.
const props = defineProps({
  cwd: { type: String, default: null },
  tabId: { type: Number, default: 0 },
  active: { type: Boolean, default: true },
  dockAnimating: { type: Boolean, default: false },
})

const mountEl = ref(null)
let term = null
let textDrain = null
let fitAddon = null
let resizeObserver = null
// shallowRef prevents unwrapping nested refs returned by usePtyTerminal composable.
const ptyApi = shallowRef(null)

// Compose field elements for pre-composing commands before PTY submission.
const composeInputEl = ref(null)
const composeText = ref('')
// Track composition end timestamp to prevent Enter from firing on committed syllables in WebKit.
let composeEndedAt = -Infinity
function onComposeCompositionEnd() {
  composeEndedAt = performance.now()
}

// Handles Enter submission, Shift+Enter pass-through, and latched Ctrl modifier routing.
function onComposeKeydown(e) {
  if (e.isComposing || e.keyCode === 229) return
  const api = ptyApi.value
  if (
    api?.pendingModifiers.value.ctrl &&
    e.key.length === 1 &&
    api.ctrlByteFor(e.key) !== null
  ) {
    e.preventDefault()
    api.emitKey({ char: e.key })
    return
  }
  if (e.key !== 'Enter') return
  // Shift+Enter falls through to native textarea newline insertion.
  if (e.shiftKey) return
  // Ignore Enter if it immediately follows composition end.
  if (performance.now() - composeEndedAt < POST_COMPOSITION_MS) return
  onComposeSend()
}

function onComposeSend() {
  if (!ptyApi.value) return
  // Translates buffer newlines into carriage returns for shell execution.
  const lines = (composeText.value || '').split(/\r\n|\n|\r/)
  ptyApi.value.sendRaw(lines.join('\r') + '\r')
  composeText.value = ''
  // Refocus input to allow continuous typing without re-tapping.
  composeInputEl.value?.focus()
}

// Mobile key row definitions mapped to modifier toggles, CSI escapes, or raw sequences.
const KEY_ROW = [
  { title: 'Esc', label: 'Esc', seq: '\x1b' },
  { title: 'Tab', label: 'Tab', seq: '\t', shiftSeq: '\x1b[Z' },
  { title: 'Shift (tap to latch, tap again to release)', label: 'Shift', arms: 'shift' },
  { title: 'Ctrl (tap to latch, tap again to release)', label: 'Ctrl', arms: 'ctrl' },
  { title: 'Up', icon: 'fa-arrow-up', csi: 'A' },
  { title: 'Down', icon: 'fa-arrow-down', csi: 'B' },
  { title: 'Left', icon: 'fa-arrow-left', csi: 'D' },
  { title: 'Right', icon: 'fa-arrow-right', csi: 'C' },
  { title: 'Enter', label: 'Enter', seq: '\r' },
]

// Debounce timestamp preventing duplicate clicks after touch events.
let lastKeyTouchAt = 0

// Toggles modifier latch or emits key sequence via usePtyTerminal.
function fireKey(k) {
  if (!ptyApi.value) return
  if (k.arms) {
    ptyApi.value.toggleModifier(k.arms)
    return
  }
  ptyApi.value.emitKey(k)
}

function onKeyTouch(k) {
  lastKeyTouchAt = Date.now()
  fireKey(k)
}

function onKeyClick(k) {
  if (Date.now() - lastKeyTouchAt < 700) return
  fireKey(k)
}

// Measures local screen size and requests host to resize shared PTY.
function onRequestFitToMe() {
  if (!fitAddon || !term || !ptyApi.value) return
  try {
    fitAddon.fit()
  } catch {
    return // Same as doFit(): fit() throws if the renderer is not ready yet.
  }
  ptyApi.value.requestResize(term.cols, term.rows)
}

// Reclaims resize authority for the host window and refits terminal.
function onReclaimResize() {
  reclaimResizeAuthority(props.tabId)
  scheduleFit()
}

// Focuses xterm textarea when clicking terminal container padding.
function onMountClick() {
  term?.focus()
}

// Terminal theme palette matching app dark theme tokens.
const THEME = {
  background: '#05070c',
  foreground: '#F3F4F6',
  cursor: '#00d2ff',
  cursorAccent: '#05070c',
  selectionBackground: 'rgba(0, 210, 255, 0.25)',
  black: '#0A0F16',
  red: '#ef4444',
  green: '#10b981',
  yellow: '#ff8c00',
  blue: '#0088ff',
  magenta: '#a855f7',
  cyan: '#00d2ff',
  white: '#F3F4F6',
  brightBlack: '#6B7280',
  brightRed: '#ef4444',
  brightGreen: '#10b981',
  brightYellow: '#ff8c00',
  brightBlue: '#0088ff',
  brightMagenta: '#a855f7',
  brightCyan: '#00d2ff',
  brightWhite: '#F3F4F6',
}

// Coalesces ResizeObserver callbacks into one fit per animation frame.
let fitFrame = 0
function scheduleFit() {
  if (props.dockAnimating || fitFrame) return
  fitFrame = requestAnimationFrame(() => {
    fitFrame = 0
    doFit()
  })
}

// Re-fit terminal once when dock layout animation completes.
watch(() => props.dockAnimating, (animating) => {
  if (!animating) scheduleFit()
})

// System monospace font stack for xterm cell metrics and glyph coverage.
const FONT_FAMILY = 'ui-monospace, Menlo, monospace'

// Bounds for terminal font rendering.
const MIN_FONT_SIZE = 4
const MAX_FONT_SIZE = 18
const BASE_FONT_SIZE = 12

function doFit() {
  if (!fitAddon || !term || !mountEl.value) return
  // Skip fit if container is unrendered or too small (e.g. collapsed panel or hidden tab).
  const { width, height } = mountEl.value.getBoundingClientRect()
  if (width < 40 || height < 24) return

  // Apply local font scale independently per device.
  const wanted = clampFont(BASE_FONT_SIZE * terminalFontScale.value)
  if (term.options.fontSize !== wanted) term.options.fontSize = wanted

  if (!ptyApi.value?.ownsPtySize) return
  try {
    fitAddon.fit()
  } catch {
    return // fit() throws if the renderer is not ready yet; the next observer tick retries.
  }
  ptyApi.value.hostResize(term.cols, term.rows)
}

function clampFont(size) {
  const scale = terminalFontScale.value
  return Math.min(MAX_FONT_SIZE * Math.max(1, scale), Math.max(MIN_FONT_SIZE * Math.min(1, scale), size))
}

// Re-apply on every zoom. doFit reads `terminalFontScale` on both surfaces, so one scheduleFit() is the whole implementation.
watch(terminalFontScale, () => scheduleFit())

// Re-fit and focus terminal when tab becomes active.
watch(
  () => props.active,
  (isActive) => {
    if (!isActive) return
    scheduleFit()
    term?.focus()
  }
)

onMounted(async () => {
  // Initialize xterm instance with keyboard capture enabled for text drain.
  term = new Terminal({
    theme: THEME,
    fontFamily: FONT_FAMILY,
    fontSize: BASE_FONT_SIZE,
    lineHeight: 1.4,
    cursorBlink: true,
    scrollback: 5000,
    allowProposedApi: true,
    macOptionClickForcesSelection: true, // without this, Option-drag can't select while a TUI has mouse-mode on
  })
  fitAddon = new FitAddon()
  term.loadAddon(fitAddon)
  term.open(mountEl.value)

  // After open(): `term.textarea` / `term.element` do not exist before it.
  textDrain = useTerminalTextDrain(term)

  // Auto-sync shell OSC 0/2 window title changes to tab store.
  term.onTitleChange((title) => {
    if (title) renameTerminalTab(props.tabId, title, { auto: true })
  })

  ptyApi.value = usePtyTerminal(term, props.tabId)

  // Start PTY session from backend state before measuring layout.
  await ptyApi.value.start(props.cwd)
  // Schedule initial fit after DOM layout and web fonts resolve.
  scheduleFit()
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(scheduleFit)

  resizeObserver = new ResizeObserver(scheduleFit)
  resizeObserver.observe(mountEl.value)

  if (props.active) term.focus()
})

onBeforeUnmount(() => {
  if (fitFrame) cancelAnimationFrame(fitFrame)
  if (resizeObserver) resizeObserver.disconnect()
  if (textDrain) textDrain.dispose()
  if (term) term.dispose()
})

// Expose terminal methods and tri-state alive status to parent container.
defineExpose({
  alive: computed(() => ptyApi.value?.alive?.value ?? 'unknown'),
  restart: () => ptyApi.value?.restart(),
  clear: () => ptyApi.value?.clear(),
  kill: () => ptyApi.value?.kill(),
  close: () => ptyApi.value?.close(),
  openExternal: () => ptyApi.value?.openExternal(),
  focus: () => term?.focus(),
})
</script>

<style scoped>
/* Flex container filling dock panel with zero minimum width to prevent overflow. */
.pty-terminal {
  display: flex;
  flex-direction: column;
  flex: 1;
  width: 100%;
  min-width: 0;
  height: 100%;
  min-height: 0;
  position: relative; /* anchors the absolute .pty-resize-owner-pill overlay */
}

/* Floating overlay pill indicating remote resize authority. */
.pty-resize-owner-pill {
  position: absolute;
  top: 8px;
  right: 8px;
  z-index: 3;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 5px 10px;
  font-size: 11px;
  font-weight: 600;
  line-height: 1.2;
  background: var(--accent-cyan);
  border: none;
  border-radius: 6px;
  color: var(--bg-primary);
  cursor: pointer;
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.55);
}

.pty-resize-owner-pill:hover {
  filter: brightness(1.08);
}

/* Allows horizontal scroll when zoomed font exceeds companion viewport width. */
.pty-terminal-mount {
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow-x: auto;
  overflow-y: hidden;
  background: #05070c;
  padding: 4px 8px;
}

/* Ensures xterm fills terminal mount container. */
.pty-terminal-mount :deep(.xterm) {
  height: 100%;
}

/* Preserves xterm helper textarea focusability and IME candidate window positioning. */
.pty-terminal-mount :deep(.xterm-viewport) {
  overflow-y: auto;
  scrollbar-width: thin;
}

/* Compact key bar for mobile and companion views. */
.pty-key-row {
  display: flex;
  /* Prevents key row wrapping on narrow mobile screens. */
  flex-wrap: nowrap;
  overflow-x: auto;
  gap: 2px;
  padding: 2px 4px;
  border-top: 1px solid var(--border-color);
  background: rgba(255, 255, 255, 0.02);
  flex-shrink: 0;
}

.pty-key {
  flex: 0 0 auto;
  padding: 3px 5px;
  font-size: 10px;
  line-height: 1;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  color: var(--text-muted);
  cursor: pointer;
}

/* Restricts hover styles to pointer devices to avoid sticky hover states on touch screens. */
@media (hover: hover) {
  .pty-key:hover {
    color: var(--text-light);
    border-color: var(--accent-cyan);
  }
}

/* Active state styling when modifier key is latched. */
.pty-key.is-armed {
  color: var(--bg-primary);
  background: var(--accent-cyan);
  border-color: var(--accent-cyan);
}

/* Hairline separator between key groups. */
.pty-key-sep {
  align-self: stretch;
  width: 1px;
  margin: 0 1px;
  background: var(--border-color);
}

/* Input row for IME composition under the key bar. */
.pty-compose-row {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 8px;
  border-top: 1px solid var(--border-color);
  background: rgba(255, 255, 255, 0.02);
  flex-shrink: 0;
}

/* Auto-sizing compose textarea supporting multi-line input up to max height. */
.pty-compose-input {
  flex: 1;
  min-width: 0;
  padding: 4px 8px;
  font-family: inherit;
  font-size: 12px;
  line-height: 1.5;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  color: var(--text-light);
  resize: none;
  overflow-y: auto;
  max-height: 5.5em;
  field-sizing: content;
}

.pty-compose-input::placeholder {
  color: var(--text-muted);
}

.pty-compose-input:focus {
  outline: none;
  border-color: var(--accent-cyan);
}

.pty-compose-send {
  font-size: 12px;
}
</style>
