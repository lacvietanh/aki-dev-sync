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

// Bounds for the companion's font scaling. The floor is deliberately tiny — on a phone held in
// portrait, an 80-column shared terminal genuinely needs ~5px text, and rendering it small is the
// decided trade (see doFit).
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

  // T-4: THE HOST ALONE DECIDES cols/rows. A companion that fitted its own container re-wrapped
  // the shared shell's output to phone width, which destroys exactly what someone opens the phone
  // to read — progress bars, tables, `git status` alignment — and no amount of zooming brings that
  // back, whereas small text can at least be zoomed. So the companion keeps the host's grid and
  // scales the FONT until that grid fills its viewport. `ownsPtySize` is asked instead of `isHost`
  // because this component must stay role-agnostic (ENV-1); the role lives in the composable.
  if (!ptyApi.value?.ownsPtySize) {
    scaleFontToFit()
    return
  }
  try {
    fitAddon.fit()
  } catch {
    return // fit() throws if the renderer is not ready yet; the next observer tick retries.
  }
  ptyApi.value.hostResize(term.cols, term.rows)
}

/** Companion-only: pick the font size at which the host's cols × rows grid just fits this screen.
 *
 *  Measured, not calculated: cell width is a font-metric no formula predicts reliably across
 *  fonts and DPRs, so the current render is used as the ruler — `.xterm-screen` is exactly
 *  cols × rows cells wide/high, so the ratio between the space available and the space it
 *  currently occupies is the ratio to apply to the font size. Converges in one or two frames. */
function scaleFontToFit() {
  const screen = mountEl.value.querySelector('.xterm-screen')
  if (!screen) return
  const w = screen.offsetWidth
  const h = screen.offsetHeight
  if (!w || !h) return

  const cs = getComputedStyle(mountEl.value)
  // Content box, minus a gutter for the viewport scrollbar — overshooting width is what would
  // force xterm's own soft wrap back in, which is the damage this whole branch exists to avoid.
  const availWidth = mountEl.value.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight) - 10
  const availHeight = mountEl.value.clientHeight - parseFloat(cs.paddingTop) - parseFloat(cs.paddingBottom)
  if (availWidth <= 0 || availHeight <= 0) return

  const ratio = Math.min(availWidth / w, availHeight / h)
  if (!Number.isFinite(ratio) || ratio <= 0) return

  const current = term.options.fontSize || BASE_FONT_SIZE
  const next = Math.min(MAX_FONT_SIZE, Math.max(MIN_FONT_SIZE, current * ratio))
  // Sub-pixel churn would re-render the whole terminal on every observer tick for no visible gain.
  if (Math.abs(next - current) < 0.2) return
  term.options.fontSize = next
}

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
  // A companion's grid changes without its container changing (the host echoes a new cols/rows, or
  // a scrollback hydrate applies one), and a ResizeObserver on the container cannot see that — so
  // the font has to be re-scaled off the grid change itself. On the host this is a no-op loop:
  // fit() emits the event, the follow-up fit finds the same size and emits nothing further.
  term.onResize(scheduleFit)

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

.pty-terminal-mount {
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
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
