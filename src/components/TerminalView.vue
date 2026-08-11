<!--
  In-app terminal — the xterm.js mount (docs/plan/done/1.20.0-terminal-and-remote-sync.md §4, §4.5).

  Role-agnostic by construction (ENV-1, docs/plan/done/remote-control.md §9): this file never imports
  or checks `isHost` — every host/companion branch lives in composables/usePtyTerminal.js and
  services/ptyBridge.js. The SAME markup renders on Mac and phone; it asks the composable for
  CAPABILITIES (`ownsPtySize`, `showReclaimPill` host-only, `showKeyRow`), never "am I the host".
  Resize-authority design: docs/plan/wish-terminal-manual-resize-authority.md.

  WP-C (tab strip): one instance per open tab (`tabId` prop), all mounted at once so switching tabs
  never re-spawns a shell or drops render state — `active` (prop) picks which one is shown
  (dock/TerminalStack.vue's v-for uses v-show), and this file re-fits + refocuses itself the moment
  it becomes the active one (see the `props.active` watcher below).

  Extreme Narrow (CLAUDE.md): one mount area + one slim icon key row, no extra banner/label row.
-->
<template>
  <div class="pty-terminal">
    <div ref="mountEl" class="pty-terminal-mount" @click="onMountClick"></div>
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
        :class="{ 'is-armed': !!k.arms && !!ptyApi?.pendingModifiers?.value?.[k.arms] }"
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
      <!-- "Fit to my screen": the one explicit tap that claims temporary resize authority over the
           shared PTY. docs/plan/wish-terminal-manual-resize-authority.md. -->
      <span class="pty-key-sep" aria-hidden="true"></span>
      <button class="pty-key" title="Fit terminal to my screen" @mousedown.prevent @click="onRequestFitToMe">
        <i class="fa-solid fa-expand"></i>
      </button>
    </div>
    <!-- Host-only reclaim pill: shown only while a companion holds resize authority. An overlay,
         not a row (Extreme Narrow). docs/plan/wish-terminal-manual-resize-authority.md. -->
    <button
      v-if="ptyApi?.showReclaimPill?.value"
      class="pty-resize-owner-pill"
      title="Reclaim this terminal's size for your own window"
      @mousedown.prevent
      @click="onReclaimResize"
    >
      Sized for a connected phone — tap to reclaim
    </button>
    <!--
      Compose row: a real text field that composes a whole line before anything reaches the PTY.
      Gated on the SAME capability as the key row (`showKeyRow`), which is what re-scopes it to a
      companion. Its Mac justification was Vietnamese IME input; direct typing now works there
      (useTerminalTextDrain.js), so that justification is spent and Extreme Narrow (CLAUDE.md)
      removes the control the Mac's own keyboard already provides. What survives is the surface with
      no physical keyboard, plus true composing IMEs — whose WKWebView support is independently poor
      and which are scoped out of the direct-typing fix.
      Unlike the key row's buttons, this field MUST keep native focus so an IME can compose into it —
      no `.prevent` on it, ever.
    -->
    <div v-if="ptyApi?.showKeyRow" class="pty-compose-row">
      <!--
        A `<textarea>`, not an `<input>`, and that is the WHOLE of what makes Shift+Enter possible:
        a single-line input has no representation for a newline at all — `\n` cannot exist in its
        value — so no keydown handler could ever have inserted one. `rows="1"` + `field-sizing`
        keeps it one line tall until it actually needs two (see <style>).
      -->
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
import { renameTerminalTab, reclaimResizeAuthority } from '../store/terminalTabsStore'
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
let textDrain = null
let fitAddon = null
let resizeObserver = null
// `shallowRef`, NOT `ref`, and that is load-bearing rather than an optimisation. A deep `ref` runs
// its assigned object through `reactive()`, and `reactive()` UNWRAPS refs held in properties — so
// `ptyApi.value.pendingModifiers` came back as the plain `{ ctrl, shift }` object and every
// `.pendingModifiers.value` in this file silently read `undefined`. That is exactly why the Ctrl /
// Shift key-row buttons never lit up while the bytes they sent were correct all along (the latch
// itself lives inside the composable's closure, which the unwrapping cannot reach): the template's
// `is-armed` test was reading `undefined?.[k.arms]` on every render. `shallowRef` leaves the object
// exactly as the composable returned it, so the refs stay refs and `.value` means what it says.
//
// This is why the return shape must stay disciplined: `showKeyRow` / `ownsPtySize` are plain
// booleans (`v-if="ptyApi?.showKeyRow"` reads them directly — a ref there would be truthy ALWAYS and
// would put the phone-only key row on the Mac), while `pendingModifiers` / `alive` are refs read
// through `.value`. Adding a ref to the composable's return means every consumer here needs
// `.value`; adding a plain value means none may use it.
const ptyApi = shallowRef(null)

// Compose row: a real text field under the key row so a full command is composed before anything
// reaches the PTY, instead of the terminal receiving keystroke-at-a-time edits.
//
// COMPANION-ONLY AGAIN (see the template's `v-if`). It rendered on the Mac too from 1.22.0, on the
// rationale that OpenKey-class engines could not type directly into xterm. That is now fixed at the
// source (useTerminalTextDrain.js), so the Mac's justification is spent and the row went back
// behind the same capability the key row uses. What it is for now is the surface with no physical
// keyboard: dictation, and a soft keyboard's own IME owning the text with synchronous feedback,
// which a no-local-echo terminal cannot give a preedit.
const composeInputEl = ref(null)
const composeText = ref('')
// WebKit fires `compositionend` BEFORE `keydown`, so `isComposing` is already false on the Enter
// that committed a syllable. Same timestamp window as the drain, for the same reason.
let composeEndedAt = -Infinity
function onComposeCompositionEnd() {
  composeEndedAt = performance.now()
}

/** ONE keydown handler for the compose box, because two (`@keydown` plus `@keydown.enter`) would
 *  collide on the same `onKeydown` prop and silently drop one of them.
 *
 *  Enter must not send MID-COMPOSITION. A composing IME fires `keydown` with `isComposing` true
 *  (legacy engines report `keyCode` 229) for the Enter that COMMITS the syllable — acting on it
 *  would fire the half-finished line and swallow the commit, which is the same class of bug this
 *  whole row exists to remove.
 *
 *  A LATCHED CTRL APPLIES HERE TOO (plan §4.3). `onComposeSend` refocuses this box after every
 *  send, so on a phone it holds focus by DEFAULT — a latch that stopped working exactly where the
 *  user types is not a working latch, and Ctrl+C is the escape hatch from a runaway process. This
 *  is a latch statement about the next key, the way OS-level sticky keys behave, not a property of
 *  one widget. It early-returns when nothing is latched, so the common case is unchanged and no new
 *  DOM element exists to show it (Extreme Narrow, CLAUDE.md).
 *
 *  Deliberately narrow, each clause for a stated reason:
 *   - Ctrl only. A latched SHIFT is ignored and PRESERVED — capitalisation is the soft keyboard's
 *     own job and intercepting it would swallow ordinary typing.
 *   - Only characters Ctrl actually means something for (`ctrlByteFor`), so a latched Ctrl plus a
 *     space or a digit types normally instead of vanishing into the shell.
 *   - Never while composing — an armed latch must never eat an IME keystroke. */
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
  // Shift+Enter is simply NOT HANDLED: it falls through to the textarea's own newline insertion.
  // That is deliberate — intercepting it and inserting the newline by hand is the one form a real
  // IME fights, and the native path is already correct.
  if (e.shiftKey) return
  // WebKit fires compositionend BEFORE keydown, so this Enter may be the one that COMMITTED a
  // syllable, with `isComposing` already false. Acting on it would send a half-finished line and
  // swallow the commit — the exact class of bug this row exists to remove.
  if (performance.now() - composeEndedAt < POST_COMPOSITION_MS) return
  onComposeSend()
}

function onComposeSend() {
  if (!ptyApi.value) return
  // A buffer newline and a wire newline are different things and must be translated HERE, at the
  // send boundary — never passed through. A raw 0x0a mid-buffer reaches a readline-style shell as
  // accept-line, so a two-line compose would submit line 1 and strand line 2 at the prompt as a
  // stray command, which a shell then EXECUTES.
  //
  // What lines are joined WITH is the one open decision in this feature and it is NOT settled here:
  // the specified form for "embedded newlines as literal text" is bracketed paste
  // (\x1b[200~ … \x1b[201~), but sending that to a program with the mode OFF lands the escape bytes
  // as literal garbage at the prompt — a worse failure than the bug. It must therefore be
  // CONDITIONED on the receiving program's bracketed-paste state, and whether xterm 5.x exposes a
  // readable accessor for that is unverified (see docs/plan/done/terminal-input-surface.md §3.4 and §6
  // row 4 for the settling command). Until it is settled this sends the fallback form the plan
  // names for the mode-off branch — the lines as separate submitted commands, i.e. what the user
  // typed rather than corrupt bytes. An unconditional bracketed-paste wrap is explicitly rejected.
  //
  // A SINGLE-LINE SEND IS BIT-IDENTICAL TO BEFORE: no newline to split on, so this is `text + '\r'`.
  // Empty text + send is still useful (a bare Enter), so it always sends at least '\r'.
  const lines = (composeText.value || '').split(/\r\n|\n|\r/)
  ptyApi.value.sendRaw(lines.join('\r') + '\r')
  composeText.value = ''
  // Refocus explicitly: the send BUTTON path would otherwise blur the input via its default
  // mousedown action (see `@mousedown.prevent` on it, same fix as the key row), and staying
  // focused is what lets consecutive commands flow without re-tapping the input each time.
  composeInputEl.value?.focus()
}

// §4.5 mobile key row, as data rather than eight near-identical buttons — each one carries three
// event bindings now (see the template comment), and hand-copying those is exactly the duplication
// a v-for exists to prevent.
//
// Each entry is one of the three shapes `emitKey` (usePtyTerminal.js) understands, and the row is
// pure DATA: nothing here decides how a modifier is applied, which is the whole point of the single
// funnel. `arms: 'ctrl' | 'shift'` marks a sticky-modifier TOGGLE; `csi` is a CSI final byte whose
// modified forms are derived rather than written out four more times; `seq`/`shiftSeq` is a literal
// with an explicitly different shifted form (Tab -> backtab).
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

// When the touch path last served a key. A browser that still synthesises a click after a
// prevented touchstart would otherwise send the key twice.
let lastKeyTouchAt = 0

/** A key-row button either TOGGLES a latch or emits a key — and it never encodes anything itself.
 *  Both the byte encoding and the latch's read/clear live in `emitKey` (usePtyTerminal.js), which
 *  is what stops the two-latch split this row used to be half of: there is no longer a code path
 *  where one modifier is read here and the other somewhere else. */
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

/** Companion "Fit to my screen": measure this screen locally (same `fitAddon.fit()` the host uses
 *  in doFit), then ask the host to apply that size. docs/plan/wish-terminal-manual-resize-authority.md. */
function onRequestFitToMe() {
  if (!fitAddon || !term || !ptyApi.value) return
  try {
    fitAddon.fit()
  } catch {
    return // Same as doFit(): fit() throws if the renderer is not ready yet.
  }
  ptyApi.value.requestResize(term.cols, term.rows)
}

/** Mac reclaim tap: hand authority back, then live-remeasure via scheduleFit — never a cached size. */
function onReclaimResize() {
  reclaimResizeAuthority(props.tabId)
  scheduleFit()
}

/** Clicking the terminal mount area focuses xterm's own hidden textarea so the user can type —
 *  xterm handles this itself for clicks that land on its rows, but not for the padding around
 *  them, which is what this covers. */
function onMountClick() {
  term?.focus()
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

// THE terminal's monospace family — declared exactly once, here. It used to be written out verbatim
// in three places (this option, the key-row button labels, the compose field), which is a
// single-source-of-truth breach regardless of whether the value itself is right.
//
// It is `ui-monospace` (the macOS system monospace token) rather than nothing, because there is no
// "no font" state for a terminal: xterm must measure a cell against SOME family, and its own
// built-in default is `courier-new, courier, monospace` — worse than this on both cell metrics and
// Vietnamese diacritic coverage. Deleting the option would not give the plain system font, it would
// give Courier New. The two DOM elements that used to repeat the stack simply inherit the app's own
// UI font now; neither has a column-alignment requirement.
const FONT_FAMILY = 'ui-monospace, Menlo, monospace'

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
  // xterm owns KEYS, the app owns TEXT (docs/research/terminal-vietnamese-ime-root-cause-4.md §7).
  // xterm's own hidden textarea is the capture surface — `disableStdin` stays at its default
  // `false` and the app-owned overlay textarea that used to sit on top of the mount is gone, along
  // with the ~40 lines of re-implemented terminal protocol it needed. All that is claimed now is
  // the text path; see useTerminalTextDrain.js for why that split is exclusive by construction.
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

  // The tab strip's chip already gets a title "for free" the same way an OS terminal window does:
  // xterm parses the shell's OSC 0/2 title escapes (every shell emits these on `cd`/running a
  // command/etc.) and fires onTitleChange. `auto: true` so a user's own rename (tab-strip context
  // menu) is never clobbered by the next prompt redraw.
  term.onTitleChange((title) => {
    if (title) renameTerminalTab(props.tabId, title, { auto: true })
  })

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
  if (textDrain) textDrain.dispose()
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
  position: relative; /* anchors the absolute .pty-resize-owner-pill overlay */
}

/* Host-only reclaim pill: absolute overlay, takes no layout space from the mount below. */
.pty-resize-owner-pill {
  position: absolute;
  top: 6px;
  right: 6px;
  z-index: 1;
  padding: 3px 8px;
  font-size: 10px;
  line-height: 1.3;
  background: var(--bg-tertiary);
  border: 1px solid var(--accent-cyan);
  border-radius: 999px;
  color: var(--text-light);
  cursor: pointer;
  opacity: 0.9;
}

.pty-resize-owner-pill:hover {
  opacity: 1;
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

/* NOTHING HIDES xterm's own `.xterm-helper-textarea` any more, and that is deliberate rather than
   an omission. It is the capture surface again (useTerminalTextDrain.js), so it must be focusable;
   `display: none` also defeats xterm's `_syncTextArea()`, which is what positions the OS's IME
   candidate window at the cursor instead of in the corner of the screen. */

.pty-terminal-mount :deep(.xterm-viewport) {
  overflow-y: auto;
  scrollbar-width: thin;
}

/* Extreme Narrow (CLAUDE.md): one slim icon row, no labels beyond what fits a small button,
   no separators/banners. Mirrors .terminal-actions sizing in AppConsole.vue. */
.pty-key-row {
  display: flex;
  /* No wrap: a phone's own narrow width is exactly where losing width to gap/padding forces this
     row onto two lines ("nhảy hàng") — flat and tight is what keeps it on one. */
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

/* `@media (hover: hover)` is load-bearing, not tidiness: on iOS Safari a `:hover` state STICKS to
   the last-tapped element until something else is tapped, so an UN-armed key row button could sit
   showing a cyan border — a false positive on the exact accent the armed state uses to say "this
   modifier is latched". The key row is a touch surface (it renders only where there is no physical
   keyboard), so the hover affordance simply does not belong there. */
@media (hover: hover) {
  .pty-key:hover {
    color: var(--text-light);
    border-color: var(--accent-cyan);
  }
}

/* Armed = solid accent fill (an inversion, the strongest single-property state signal available)
   rather than a new badge/label row — Extreme Narrow, CLAUDE.md. Bound to ONE latch now, so it can
   no longer be truthful about one modifier while lying about the other. */
.pty-key.is-armed {
  color: var(--bg-primary);
  background: var(--accent-cyan);
  border-color: var(--accent-cyan);
}

/* Hairline between the key group and the zoom group — a 1px rule inside a row that already exists,
   not a separator element of its own (Extreme Narrow, CLAUDE.md). */
.pty-key-sep {
  align-self: stretch;
  width: 1px;
  margin: 0 1px;
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

/* Typography matches the project task-note field (tasks/NotesField.vue) — `font-family: inherit`,
   12px, line-height 1.5 — which is the input the owner named as the one that renders correctly.
   The monospace override this used to carry was the third verbatim copy of the terminal's font
   stack and bought nothing: a compose field is prose being typed, not a column-aligned grid.
   Kept, unlike NotesField: the box's own background/border/radius. NotesField sits inside a card
   that already reads as an editable region; this sits in a flat strip next to a send button and
   needs its own affordance.
   `field-sizing: content` grows it only when Shift+Enter actually adds a line — one row tall the
   rest of the time (Extreme Narrow, CLAUDE.md), capped so a long paste cannot eat the terminal.
   The growth cannot re-wrap the PTY: this row renders only where `showKeyRow` is true, i.e. only
   where `ownsPtySize` is false, so `doFit` returns before `fitAddon.fit()` on every surface that
   has this element at all. */
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
