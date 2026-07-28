<!--
  In-app terminal — the xterm.js mount (docs/plan/done/1.20.0-terminal-and-remote-sync.md §4, §4.5).

  Role-agnostic by construction (ENV-1, docs/plan/done/remote-control.md §9): this file never imports
  or checks `isHost` — every host/companion branch lives in composables/usePtyTerminal.js and
  services/ptyBridge.js. The SAME markup renders on the Mac window and on a paired phone. It asks
  the composable for CAPABILITIES instead: `ownsPtySize` (does this screen decide the shared PTY's
  cols/rows?) and `showKeyRow` (does this screen need the synthetic Esc/Tab/arrow/Ctrl row?) — never
  "am I the host".

  WP-C (tab strip): one instance per open tab (`tabId` prop), all mounted at once so switching tabs
  never re-spawns a shell or drops render state — `active` (prop) picks which one is shown
  (dock/TerminalStack.vue's v-for uses v-show), and this file re-fits + refocuses itself the moment
  it becomes the active one (see the `props.active` watcher below).

  Extreme Narrow (CLAUDE.md): one mount area + one slim icon key row, no extra banner/label row.
-->
<template>
  <div class="pty-terminal">
    <div ref="mountEl" class="pty-terminal-mount"></div>
    <!--
      `@mousedown.prevent` + `@touchstart.prevent` are the fix for "every tap closes the soft
      keyboard": the default action of both is to move focus to the button, which blurs xterm's
      hidden textarea and dismisses the phone's keyboard — making sticky-Ctrl a three-gesture
      operation (tap Ctrl, re-open the keyboard, tap C). Preventing touchstart also suppresses the
      synthetic click on iOS, which is why the touch path FIRES the key itself and `onKeyClick`
      ignores a click that follows a touch it already served.
    -->
    <div v-if="ptyApi?.showKeyRow" class="pty-key-row">
      <button
        v-for="k in KEY_ROW"
        :key="k.title"
        class="pty-key"
        :title="k.title"
        :class="{
          'is-armed':
            (k.arms === 'ctrl' && ptyApi?.ctrlArmed?.value) ||
            (k.arms === 'shift' && ptyApi?.shiftArmed?.value),
        }"
        @mousedown.prevent
        @touchstart.prevent="onKeyTouch(k)"
        @click="onKeyClick(k)"
      >
        <span v-if="k.label" class="pty-key-label">{{ k.label }}</span>
        <i v-else class="fa-solid" :class="k.icon"></i>
      </button>
      <!--
        Font zoom, browser-only BY CONSTRUCTION rather than by a second condition: these buttons sit
        inside the key row, which already renders only where there is no physical keyboard. On the
        Mac the same three actions are ⌘+ / ⌘- / ⌘0 (dock/TerminalStack.vue's keydown handler), so
        the app window spends no pixels on a control its keyboard already has — which is the whole
        Extreme Narrow trade.
      -->
      <span class="pty-key-sep" aria-hidden="true"></span>
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
    </div>
    <!--
      Compose row: a real text input for phone voice-dictation / IME typing (§4.5 follow-up). Unlike
      the key row's buttons, this input MUST keep native focus so the phone's dictation/Telex IME can
      compose into it — no `.prevent` on it, ever.
    -->
    <div v-if="ptyApi?.showKeyRow" class="pty-compose-row">
      <input
        ref="composeInputEl"
        v-model="composeText"
        type="text"
        class="pty-compose-input"
        placeholder="message…"
        autocapitalize="off"
        autocomplete="off"
        autocorrect="off"
        spellcheck="false"
        @keydown.enter="onComposeSend"
      />
      <button class="pty-key pty-compose-send" title="Send" @mousedown.prevent @click="onComposeSend">
        <i class="fa-solid fa-paper-plane"></i>
      </button>
    </div>
  </div>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { usePtyTerminal } from '../composables/usePtyTerminal'
import {
  terminalFontScale,
  zoomInTerminalFont,
  zoomOutTerminalFont,
  resetTerminalFont,
} from '../composables/useTerminalFont'

// `tabId`/`active` are WP-C's multi-tab additions. `active: default true` keeps a single, undecorated
// <TerminalView /> (any call site that predates tabs) behaving exactly as before — only
// dock/TerminalStack.vue's v-for ever passes `active: false` for a backgrounded tab.
const props = defineProps({
  cwd: { type: String, default: null },
  tabId: { type: Number, default: 0 },
  active: { type: Boolean, default: true },
})

const mountEl = ref(null)
let term = null
let fitAddon = null
let resizeObserver = null
const ptyApi = ref(null)

// Compose row (§4.5 follow-up): a real `<input>` under the key row so a phone's voice dictation or
// its browser IME (e.g. Telex) can compose a full command before it is sent, instead of the app
// receiving keystroke-at-a-time IME edits through xterm's own hidden textarea.
const composeInputEl = ref(null)
const composeText = ref('')

function onComposeSend() {
  if (!ptyApi.value) return
  // Empty text + send is still useful (a bare Enter), so this always sends at least '\r'.
  ptyApi.value.sendRaw((composeText.value || '') + '\r')
  composeText.value = ''
  // Refocus explicitly: the send BUTTON path would otherwise blur the input via its default
  // mousedown action (see `@mousedown.prevent` on it, same fix as the key row), and staying
  // focused is what lets consecutive commands flow without re-tapping the input each time.
  composeInputEl.value?.focus()
}

// §4.5 mobile key row, as data rather than eight near-identical buttons — each one carries three
// event bindings now (see the template comment), and hand-copying those is exactly the duplication
// a v-for exists to prevent. `arms: 'ctrl' | 'shift'` marks a sticky modifier button; everything
// else sends a sequence (`shiftSeq`, when present, is what sticky Shift swaps `seq` for — see
// fireKey). Tab between Esc and Ctrl per the sticky-Shift placement (between Tab and Ctrl).
const KEY_ROW = [
  { title: 'Esc', label: 'Esc', seq: '\x1b' },
  { title: 'Tab', label: 'Tab', seq: '\t', shiftSeq: '\x1b[Z' },
  { title: 'Shift (tap, then tap Tab or an arrow)', label: 'Shift', arms: 'shift' },
  { title: 'Ctrl (tap, then type a letter)', label: 'Ctrl', arms: 'ctrl' },
  { title: 'Up', icon: 'fa-arrow-up', seq: '\x1b[A', shiftSeq: '\x1b[1;2A' },
  { title: 'Down', icon: 'fa-arrow-down', seq: '\x1b[B', shiftSeq: '\x1b[1;2B' },
  { title: 'Left', icon: 'fa-arrow-left', seq: '\x1b[D', shiftSeq: '\x1b[1;2D' },
  { title: 'Right', icon: 'fa-arrow-right', seq: '\x1b[C', shiftSeq: '\x1b[1;2C' },
  { title: 'Enter', label: 'Enter', seq: '\r' },
]

// When the touch path last served a key. A browser that still synthesises a click after a
// prevented touchstart would otherwise send the key twice.
let lastKeyTouchAt = 0

// `\x1b[Z` (backtab) and the CSI modifier-2 arrow sequences are the standard terminal encodings
// for Shift+Tab / Shift+arrow — AI agents (Claude Code) use Shift+Tab constantly for mode cycling,
// which a phone's on-screen keyboard has no physical Shift key to produce.
function fireKey(k) {
  if (!ptyApi.value) return
  if (k.arms === 'ctrl') {
    ptyApi.value.armCtrl()
    return
  }
  if (k.arms === 'shift') {
    ptyApi.value.armShift()
    return
  }
  // Ctrl arms the next REAL keystroke (wireInput's onData in usePtyTerminal.js) and is untouched
  // here — Ctrl+letter still goes through the soft keyboard exactly as today. Shift instead arms
  // the next KEY-ROW button, so it is consumed right here: swap in `shiftSeq` if this key has one,
  // then disarm unconditionally (Enter/Esc/etc. have no `shiftSeq` and are sent unaffected, but a
  // tap on ANY key-row key still consumes/disarms sticky Shift).
  const shiftArmed = ptyApi.value.shiftArmed
  const wasShiftArmed = !!shiftArmed?.value
  const seq = wasShiftArmed && k.shiftSeq ? k.shiftSeq : k.seq
  if (wasShiftArmed) shiftArmed.value = false
  if (seq) ptyApi.value.sendRaw(seq)
}

function onKeyTouch(k) {
  lastKeyTouchAt = Date.now()
  fireKey(k)
}

function onKeyClick(k) {
  if (Date.now() - lastKeyTouchAt < 700) return
  fireKey(k)
}

// Hardcoded to the app's existing dark palette (src/assets/main.css :root tokens) — no theme
// config UI, per the task brief.
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

// Coalesces the burst of ResizeObserver callbacks a single window drag or panel expand produces
// into one fit per frame — an unthrottled fit() re-measures the DOM on every callback and can
// feed its own observer, which is the documented ResizeObserver loop warning.
let fitFrame = 0
function scheduleFit() {
  if (fitFrame) return
  fitFrame = requestAnimationFrame(() => {
    fitFrame = 0
    doFit()
  })
}

// Floor/ceiling on the rendered size, scale included — a 3× zoom of the 12px base is a legitimate 36px, so the bounds move WITH the scale rather than capping it.
const MIN_FONT_SIZE = 4
const MAX_FONT_SIZE = 18
const BASE_FONT_SIZE = 12

function doFit() {
  if (!fitAddon || !term || !mountEl.value) return
  // A container that is not laid out yet (tab just switched, panel collapsed, window minimised)
  // measures 0 and makes FitAddon return its minimum size. Applying that would shrink the xterm
  // into a tiny corner of a big black panel — the exact symptom reported against 1.20.0 — and on
  // the host it would also re-wrap the real shell. Skip; the ResizeObserver fires again with real
  // numbers the moment the layout settles.
  const { width, height } = mountEl.value.getBoundingClientRect()
  if (width < 40 || height < 24) return

  // T-4: THE HOST ALONE DECIDES cols/rows. A companion never fits its own container against the shared grid — see docs/feat/in-app-terminal.md's "Terminal font size" for why the old measure-and-scale-to-fit behaviour (`scaleFontToFit`, removed) was wrong, not just redundant: it made the companion's font size a function of the host's grid, so the same `terminalFontScale` value rendered at different sizes on different screens.
  // `ownsPtySize` is asked instead of `isHost` because this component must stay role-agnostic (ENV-1); the role lives in the composable.
  //
  // Both surfaces now do the exact same thing: BASE_FONT_SIZE × this device's own local scale, clamped. Native, 100%-independent size per device (per-device state: `useTerminalFont.js`) — never derived from, or fought over with, the other screen's viewport.
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

// A v-show-hidden tab (WP-C: all tabs stay mounted, only the active one is shown) measures 0 and
// doFit bails below the 40x24 floor, so becoming active must explicitly re-fit rather than rely on
// the ResizeObserver, which never fires for a container that was never resized — only shown.
watch(
  () => props.active,
  (isActive) => {
    if (!isActive) return
    scheduleFit()
    term?.focus()
  }
)

onMounted(async () => {
  term = new Terminal({
    theme: THEME,
    fontFamily: '"SFMono-Regular", Consolas, "Liberation Mono", Menlo, Courier, monospace',
    fontSize: BASE_FONT_SIZE,
    lineHeight: 1.4,
    cursorBlink: true,
    scrollback: 5000,
    allowProposedApi: true,
    // T-5: no local echo — SSOT is the PTY. xterm's own convertEol/etc. defaults are fine as-is;
    // "no local echo" here means we never write typed input back into the terminal ourselves
    // (see usePtyTerminal.js's wireInput — it only ever sends, never writes).
  })
  fitAddon = new FitAddon()
  term.loadAddon(fitAddon)
  term.open(mountEl.value)

  ptyApi.value = usePtyTerminal(term, props.tabId)

  // Hydrate from the backend's last-known state FIRST (spawns the shared PTY if needed, applies
  // its current size), THEN fit to this screen's real container — on the host that fit is
  // authoritative and immediately overwrites the hydrated size via hostResize's pty_resize call +
  // echo (T-4); on a companion hostResize is a no-op, so the hydrated size stands until the next
  // host-originated echo. Fitting BEFORE hydrating would have the hydrate step immediately
  // clobber a freshly-correct host fit with a stale backend value.
  await ptyApi.value.start(props.cwd)
  // One frame later: at this point in onMounted the flex chain above this component
  // (.dashboard-bottom → .dock-stack → .terminal-mount-wrap) has not necessarily resolved a
  // height yet, and the web font xterm measures its cell size against may still be loading.
  // Fitting synchronously here is what produced a permanently 80x24-ish terminal parked in the
  // corner. scheduleFit() also self-corrects via the observer if this frame is still too early.
  scheduleFit()
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(scheduleFit)

  resizeObserver = new ResizeObserver(scheduleFit)
  resizeObserver.observe(mountEl.value)

  if (props.active) term.focus()
})

onBeforeUnmount(() => {
  if (fitFrame) cancelAnimationFrame(fitFrame)
  if (resizeObserver) resizeObserver.disconnect()
  if (term) term.dispose()
})

// Consumed by dock/TerminalStack.vue's header buttons — the management row lives in the panel
// header that already exists rather than in a new row of its own (Extreme Narrow, CLAUDE.md).
// `alive` is now the TRI-STATE ref itself ('unknown'|true|false, see usePtyTerminal.js), not
// coerced to boolean — TerminalTabStrip.vue and TerminalStack.vue need to tell "no news yet" apart
// from "confirmed dead" so an 'unknown' tab is never painted red.
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
/* `flex: 1` + `min-width: 0` are load-bearing, not tidiness: the parent (.terminal-mount-wrap in
   AppConsole.vue) is a ROW flex container, so without a grow factor this element sizes to its
   content — and its content at mount time is an empty div. FitAddon then measured ~0 width and
   fell back to its minimum, which is why the terminal rendered as a small box in the corner of an
   otherwise empty black panel. `min-width: 0` keeps a long unwrapped line from pushing it wider
   than the panel afterwards. */
.pty-terminal {
  display: flex;
  flex-direction: column;
  flex: 1;
  width: 100%;
  min-width: 0;
  height: 100%;
  min-height: 0;
}

/* `overflow-x: auto`, not `hidden`: on a companion the grid is fixed by the host (T-4) and zoom
   makes the FONT bigger, so a zoomed-in phone genuinely renders wider than its viewport — clipping
   that would leave the right-hand columns unreachable, a dead end rather than a zoom. Vertical
   stays `hidden` because xterm's own `.xterm-viewport` owns that axis. */
.pty-terminal-mount {
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow-x: auto;
  overflow-y: hidden;
  background: #05070c;
  padding: 4px 8px;
}

/* xterm sizes its own canvas/DOM rows against this element; it must actually fill the mount. */
.pty-terminal-mount :deep(.xterm) {
  height: 100%;
}

.pty-terminal-mount :deep(.xterm-viewport) {
  overflow-y: auto;
  scrollbar-width: thin;
}

/* Extreme Narrow (CLAUDE.md): one slim icon row, no labels beyond what fits a small button,
   no separators/banners. Mirrors .terminal-actions sizing in AppConsole.vue. */
.pty-key-row {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  padding: 4px 8px;
  border-top: 1px solid var(--border-color);
  background: rgba(255, 255, 255, 0.02);
  flex-shrink: 0;
}

.pty-key {
  flex: 0 0 auto;
  padding: 4px 8px;
  font-size: 10px;
  line-height: 1;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  color: var(--text-muted);
  cursor: pointer;
}

.pty-key:hover {
  color: var(--text-light);
  border-color: var(--accent-cyan);
}

.pty-key.is-armed {
  color: var(--bg-primary);
  background: var(--accent-cyan);
  border-color: var(--accent-cyan);
}

.pty-key-label {
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", Menlo, Courier, monospace;
}

/* Hairline between the key group and the zoom group — a 1px rule inside a row that already exists,
   not a separator element of its own (Extreme Narrow, CLAUDE.md). */
.pty-key-sep {
  align-self: stretch;
  width: 1px;
  margin: 0 2px;
  background: var(--border-color);
}

/* Compose row: one slim row directly under the key row — still Extreme Narrow (CLAUDE.md), just one
   more element in the same companion-only block that already exists only on phones. */
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
  font-size: 13px;
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", Menlo, Courier, monospace;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  color: var(--text-light);
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
