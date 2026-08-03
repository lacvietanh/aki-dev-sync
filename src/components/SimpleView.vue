<!--
  Phone companion terminal view — renders the PTY byte stream as a plain text line stream. No xterm.js, no cols/rows, no grid geometry. Companion-only by TerminalStack.vue's routing (useTerminalViewType.js); this file itself is role-agnostic and never checks isHost.

  Key row and compose row are copy-adapted from TerminalView.vue (SV-4: Rule of Three — this is the second consumer, not the third, and the two are not drop-in identical). Props match TerminalView.vue exactly so TerminalStack.vue's v-for can use either with no change to loop logic.
-->
<template>
  <div class="sv-terminal">
    <!-- Scrolling text stream — the plain-text alternative to xterm's canvas grid. -->
    <div ref="streamEl" class="sv-stream" @scroll="onScroll">
      <div
        v-for="(line, i) in lines"
        :key="i"
        class="sv-line"
      >{{ line }}</div>
      <!-- In-progress line: the write buffer not yet committed by a newline. -->
      <div v-if="buffer" class="sv-line sv-line--buffer">{{ buffer }}</div>
    </div>

    <!--
      Key row: copy-adapted from TerminalView.vue. SimpleView is always the companion surface so the key row is always shown — there is no showKeyRow gate. sendRaw comes from usePtyStream rather than usePtyTerminal, but the wire path (FRAME_PTY_INPUT) is identical.
      @mousedown.prevent + @touchstart.prevent: same fix as TerminalView.vue — prevents the button tap from moving focus away from the compose textarea and closing the soft keyboard.
    -->
    <div class="pty-key-row">
      <button
        v-for="k in KEY_ROW"
        :key="k.title"
        class="pty-key"
        :title="k.title"
        :class="{ 'is-armed': !!k.arms && !!pendingModifiers[k.arms] }"
        @mousedown.prevent
        @touchstart.prevent="onKeyTouch(k)"
        @click="onKeyClick(k)"
      >
        <span v-if="k.label" class="pty-key-label">{{ k.label }}</span>
        <i v-else class="fa-solid" :class="k.icon"></i>
      </button>
    </div>

    <!--
      Compose row: copy-adapted from TerminalView.vue. Native <textarea> (no xterm, so no IME trap). No .prevent on the textarea itself — it must keep native focus so an IME can compose.
    -->
    <div class="pty-compose-row">
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
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { usePtyStream } from '../composables/usePtyStream'

// Props match TerminalView.vue exactly so TerminalStack.vue's v-for can use either with no change to loop logic (docs/plan/wish-terminal-split-simpleview.md §SimpleView.vue interface).
const props = defineProps({
  cwd:    { type: String,  default: null },
  tabId:  { type: Number,  default: 0 },
  active: { type: Boolean, default: true },
})

const { lines, buffer, alive, sendRaw, start } = usePtyStream(props.tabId)

// ── Auto-scroll ────────────────────────────────────────────────────────────────
// Track whether the user is at (or within a threshold of) the bottom. When they are, auto-scroll on every new line; when they have scrolled up, do not forcibly pull them back — that would interrupt reading history mid-stream.
const streamEl = ref(null)
const userScrolledUp = ref(false)
const SCROLL_THRESHOLD = 40 // px from the bottom to still count as "at the bottom"

function isAtBottom(el) {
  return el.scrollHeight - el.scrollTop - el.clientHeight <= SCROLL_THRESHOLD
}

function onScroll() {
  if (!streamEl.value) return
  userScrolledUp.value = !isAtBottom(streamEl.value)
}

function scrollToBottom() {
  if (!streamEl.value) return
  streamEl.value.scrollTop = streamEl.value.scrollHeight
}

// Scroll when lines or buffer change — but only if the user has not scrolled up.
watch([lines, buffer], () => {
  if (userScrolledUp.value) return
  nextTick(scrollToBottom)
})

// When this tab becomes active, restore scroll to bottom if it was already there.
watch(
  () => props.active,
  (isActive) => {
    if (!isActive) return
    if (!userScrolledUp.value) nextTick(scrollToBottom)
  },
)

// ── Key row ────────────────────────────────────────────────────────────────────
// Data-driven key row, same shape as TerminalView.vue. Sticky Ctrl/Shift latches are owned locally (no usePtyTerminal — this composable does not exist here), because SimpleView has no xterm binding to pipe through.
const KEY_ROW = [
  { title: 'Esc',                                          label: 'Esc',   seq: '\x1b' },
  { title: 'Tab',                                          label: 'Tab',   seq: '\t', shiftSeq: '\x1b[Z' },
  { title: 'Shift (tap to latch, tap again to release)',   label: 'Shift', arms: 'shift' },
  { title: 'Ctrl (tap to latch, tap again to release)',    label: 'Ctrl',  arms: 'ctrl' },
  { title: 'Up',    icon: 'fa-arrow-up',    csi: 'A' },
  { title: 'Down',  icon: 'fa-arrow-down',  csi: 'B' },
  { title: 'Left',  icon: 'fa-arrow-left',  csi: 'D' },
  { title: 'Right', icon: 'fa-arrow-right', csi: 'C' },
  { title: 'Enter', label: 'Enter', seq: '\r' },
]

// Sticky modifier state — latched on first tap, released on second.
const pendingModifiers = ref({ ctrl: false, shift: false })

function toggleModifier(mod) {
  pendingModifiers.value = { ...pendingModifiers.value, [mod]: !pendingModifiers.value[mod] }
}

// Translate a key-row entry into the raw bytes to send, applying any armed modifiers.
// Mirrors the emitKey contract from usePtyTerminal.js without importing that composable.
function emitKeyRaw(k) {
  // Ctrl modifier: convert a printable character to its control byte (A→\x01, C→\x03, …).
  if (k.char !== undefined) {
    const code = k.char.toUpperCase().charCodeAt(0)
    if (code >= 64 && code <= 95) {
      sendRaw(String.fromCharCode(code - 64))
    }
    pendingModifiers.value = { ...pendingModifiers.value, ctrl: false }
    return
  }
  if (k.csi !== undefined) {
    const seq = pendingModifiers.value.shift ? `\x1b[1;2${k.csi}` : `\x1b[${k.csi}`
    sendRaw(seq)
    pendingModifiers.value = { ctrl: false, shift: false }
    return
  }
  if (k.seq !== undefined) {
    const seq = pendingModifiers.value.shift && k.shiftSeq ? k.shiftSeq : k.seq
    sendRaw(seq)
    pendingModifiers.value = { ctrl: false, shift: false }
    return
  }
}

let lastKeyTouchAt = 0

function fireKey(k) {
  if (k.arms) {
    toggleModifier(k.arms)
    return
  }
  emitKeyRaw(k)
}

function onKeyTouch(k) {
  lastKeyTouchAt = Date.now()
  fireKey(k)
}

function onKeyClick(k) {
  if (Date.now() - lastKeyTouchAt < 700) return
  fireKey(k)
}

// ── Compose row ────────────────────────────────────────────────────────────────
// Copy-adapted from TerminalView.vue. Same IME guard, same multi-line join, same send-then-refocus pattern. No xterm dependency here — sendRaw goes to usePtyStream.
const composeInputEl = ref(null)
const composeText = ref('')
const POST_COMPOSITION_MS = 50 // same window as useTerminalTextDrain.js

let composeEndedAt = -Infinity
function onComposeCompositionEnd() {
  composeEndedAt = performance.now()
}

function onComposeKeydown(e) {
  if (e.isComposing || e.keyCode === 229) return
  // A latched Ctrl + a printable character that has a control-byte meaning.
  if (
    pendingModifiers.value.ctrl &&
    e.key.length === 1 &&
    ctrlByteFor(e.key) !== null
  ) {
    e.preventDefault()
    emitKeyRaw({ char: e.key })
    return
  }
  if (e.key !== 'Enter') return
  // Shift+Enter falls through to the textarea's own newline insertion — deliberate.
  if (e.shiftKey) return
  // IME-commit guard: WebKit fires compositionend BEFORE keydown, so isComposing may already be false on the Enter that committed a syllable.
  if (performance.now() - composeEndedAt < POST_COMPOSITION_MS) return
  onComposeSend()
}

// Returns the control byte for a character, or null if none applies.
// Used by the compose keydown guard — mirrors the usePtyTerminal.js helper.
function ctrlByteFor(char) {
  const code = char.toUpperCase().charCodeAt(0)
  if (code >= 64 && code <= 95) return code - 64
  return null
}

function onComposeSend() {
  if (!sendRaw) return
  // Multi-line compose: join with \r, send the whole thing, then add the final \r. A single-line send is bit-identical to before (text + '\r'). Empty text + send is still useful (a bare Enter), so always appends '\r'.
  const textLines = (composeText.value || '').split(/\r\n|\n|\r/)
  sendRaw(textLines.join('\r') + '\r')
  composeText.value = ''
  composeInputEl.value?.focus()
}

// ── Lifecycle ──────────────────────────────────────────────────────────────────
onMounted(async () => {
  await start()
  // Initial scroll after the stream renders its first batch of lines.
  nextTick(scrollToBottom)
})

// Consumed by dock/TerminalStack.vue's header buttons — same shape as TerminalView.vue. `alive` is the tri-state ref itself ('unknown'|true|false) from usePtyStream. Only `alive` is exposed: SimpleView has no xterm to restart/clear/kill/focus/openExternal.
defineExpose({
  alive: computed(() => alive.value ?? 'unknown'),
})
</script>

<style scoped>
/* Mirrors .pty-terminal in TerminalView.vue — same flex column, fills parent. */
.sv-terminal {
  display: flex;
  flex-direction: column;
  flex: 1;
  width: 100%;
  min-width: 0;
  height: 100%;
  min-height: 0;
}

/* Scrolling text stream — the phone's xterm replacement. `font-family: ui-monospace` gives the same monospace coverage as the xterm side without importing the terminal library. The background and text colours match the xterm THEME object in TerminalView.vue so the two surfaces look consistent when a user glances between them. `word-break: break-all` prevents an extremely long unspaced line (e.g. a base64 dump) from overflowing the viewport on a narrow phone. */
.sv-stream {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 4px 8px;
  background: #05070c;
  color: #F3F4F6;
  font-family: ui-monospace, Menlo, monospace;
  font-size: 12px;
  line-height: 1.4;
  word-break: break-all;
  scrollbar-width: thin;
  user-select: text;
}

/* Each committed line occupies its own block so a bare \n from the PTY adds a blank line exactly as it does in a real terminal. `min-height: 1lh` ensures blank lines are visible (an empty <div> collapses to zero in most browsers). */
.sv-line {
  display: block;
  min-height: 1lh;
  white-space: pre-wrap;
}

/* In-progress buffer line: slightly dimmed to distinguish it from committed output. This is the write buffer from usePtyStream — text that has arrived but whose newline has not yet landed. */
.sv-line--buffer {
  opacity: 0.7;
}

/* Key row — same CSS as TerminalView.vue, shared class names. */
.pty-key-row {
  display: flex;
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

@media (hover: hover) {
  .pty-key:hover {
    color: var(--text-light);
    border-color: var(--accent-cyan);
  }
}

.pty-key.is-armed {
  color: var(--bg-primary);
  background: var(--accent-cyan);
  border-color: var(--accent-cyan);
}

/* Compose row — same CSS as TerminalView.vue. */
.pty-compose-row {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 8px;
  border-top: 1px solid var(--border-color);
  background: rgba(255, 255, 255, 0.02);
  flex-shrink: 0;
}

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
