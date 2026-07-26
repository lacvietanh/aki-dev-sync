<!--
  In-app terminal — the xterm.js mount (docs/plan/1.20.0-terminal-and-remote-sync.md §4, §4.5).

  Role-agnostic by construction (ENV-1, docs/plan/done/remote-control.md §9): this file never imports
  or checks `isHost` — every host/companion branch lives in composables/usePtyTerminal.js and
  services/ptyBridge.js. The SAME markup renders on the Mac window and on a paired phone.

  Extreme Narrow (CLAUDE.md): one mount area + one slim icon key row, no extra banner/label row.
-->
<template>
  <div class="pty-terminal">
    <div ref="mountEl" class="pty-terminal-mount"></div>
    <div class="pty-key-row">
      <button class="pty-key" title="Esc" @click="ptyApi?.sendRaw('\x1b')"><span class="pty-key-label">Esc</span></button>
      <button class="pty-key" title="Tab" @click="ptyApi?.sendRaw('\t')"><span class="pty-key-label">Tab</span></button>
      <button class="pty-key" title="Ctrl (tap, then type a letter)" :class="{ 'is-armed': ptyApi?.ctrlArmed?.value }" @click="ptyApi?.armCtrl()">
        <span class="pty-key-label">Ctrl</span>
      </button>
      <button class="pty-key" title="Up" @click="ptyApi?.sendRaw('\x1b[A')"><i class="fa-solid fa-arrow-up"></i></button>
      <button class="pty-key" title="Down" @click="ptyApi?.sendRaw('\x1b[B')"><i class="fa-solid fa-arrow-down"></i></button>
      <button class="pty-key" title="Left" @click="ptyApi?.sendRaw('\x1b[D')"><i class="fa-solid fa-arrow-left"></i></button>
      <button class="pty-key" title="Right" @click="ptyApi?.sendRaw('\x1b[C')"><i class="fa-solid fa-arrow-right"></i></button>
      <button class="pty-key" title="Enter" @click="ptyApi?.sendRaw('\r')"><span class="pty-key-label">Enter</span></button>
    </div>
  </div>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { usePtyTerminal } from '../composables/usePtyTerminal'
import { pendingCd } from '../composables/useTerminalPanel'

// T-8: no caller passes this yet (the DEV/BUILD-redirect follow-up in plan §7 will) — the prop
// exists so that follow-up is additive, not a signature change.
const props = defineProps({ cwd: { type: String, default: null } })

const mountEl = ref(null)
let term = null
let fitAddon = null
let resizeObserver = null
const ptyApi = ref(null)

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

function doFit() {
  if (!fitAddon || !term || !mountEl.value) return
  // A container that is not laid out yet (tab just switched, panel collapsed, window minimised)
  // measures 0 and makes FitAddon return its minimum size. Applying that would shrink the xterm
  // into a tiny corner of a big black panel — the exact symptom reported against 1.20.0 — and on
  // the host it would also re-wrap the real shell. Skip; the ResizeObserver fires again with real
  // numbers the moment the layout settles.
  const { width, height } = mountEl.value.getBoundingClientRect()
  if (width < 40 || height < 24) return
  try {
    fitAddon.fit()
  } catch {
    return // fit() throws if the renderer is not ready yet; the next observer tick retries.
  }
  ptyApi.value?.hostResize(term.cols, term.rows)
}

// Consume the queued "open project in the in-app terminal" gesture (useTerminalPanel.js). Watched
// rather than read once, since the tab can already be open when the user clicks a second project.
watch(pendingCd, (path) => {
  if (!path || !ptyApi.value) return
  pendingCd.value = null
  ptyApi.value.cd(path)
})

onMounted(async () => {
  term = new Terminal({
    theme: THEME,
    fontFamily: '"SFMono-Regular", Consolas, "Liberation Mono", Menlo, Courier, monospace',
    fontSize: 12,
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

  ptyApi.value = usePtyTerminal(term)

  // Hydrate from the backend's last-known state FIRST (spawns the shared PTY if needed, applies
  // its current size), THEN fit to this screen's real container — on the host that fit is
  // authoritative and immediately overwrites the hydrated size via hostResize's pty_resize call +
  // echo (T-4); on a companion hostResize is a no-op, so the hydrated size stands until the next
  // host-originated echo. Fitting BEFORE hydrating would have the hydrate step immediately
  // clobber a freshly-correct host fit with a stale backend value.
  await ptyApi.value.start(props.cwd)
  // One frame later: at this point in onMounted the flex chain above this component
  // (.dashboard-bottom → .terminal-panel → .terminal-mount-wrap) has not necessarily resolved a
  // height yet, and the web font xterm measures its cell size against may still be loading.
  // Fitting synchronously here is what produced a permanently 80x24-ish terminal parked in the
  // corner. scheduleFit() also self-corrects via the observer if this frame is still too early.
  scheduleFit()
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(scheduleFit)

  resizeObserver = new ResizeObserver(scheduleFit)
  resizeObserver.observe(mountEl.value)

  // Consume a `cd` queued BEFORE this component existed (the common case: the popup click is what
  // switched the tab and mounted us).
  if (pendingCd.value) {
    const path = pendingCd.value
    pendingCd.value = null
    ptyApi.value.cd(path)
  }
  term.focus()
})

onBeforeUnmount(() => {
  if (fitFrame) cancelAnimationFrame(fitFrame)
  if (resizeObserver) resizeObserver.disconnect()
  if (term) term.dispose()
})

// Consumed by AppConsole.vue's header buttons — the management row lives in the panel header that
// already exists rather than in a new row of its own (Extreme Narrow, CLAUDE.md).
defineExpose({
  alive: computed(() => !!ptyApi.value?.alive?.value),
  restart: () => ptyApi.value?.restart(),
  clear: () => ptyApi.value?.clear(),
  kill: () => ptyApi.value?.kill(),
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
</style>
