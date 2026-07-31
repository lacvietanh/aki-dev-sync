<!--
  In-app terminal dock stack. WP-A rendered exactly one TerminalView; WP-C extended this to a tab
  strip + v-for over multiple TerminalViews sharing one PTY-per-tab backend (src-tauri/src/pty.rs).
  Terminal v2 adds SCOPES (tab groups) on top — see docs/arch/terminal-stack.md for the full model.

  Mount semantics, two different axes:
  - SWITCHING TABS (this component): every tab that has ever been activated stays mounted
    (`v-if="activatedTabs.has(t.id)"`) and only the active one is shown (`v-show`) — switching back
    to a tab never re-spawns its xterm instance or re-fetches its scrollback. This loop iterates the
    FULL tab list (not the scope-filtered one) ON PURPOSE, so switching between GROUPS never
    unmounts/re-spawns an xterm either — only which chip's tab is visible changes.
  - COLLAPSING THE STACK: also non-destructive since 1.21.1. `body-persist` (below) tells DockStack
    to keep its body mounted and merely `v-show` it, so a collapse no longer disposes N xterms and
    an expand no longer re-spawns and re-hydrates them. What it costs instead is that every mounted
    xterm and its 5000-line scrollback is RETAINED behind a closed panel — which is what MAX_TABS
    bounds. What it buys is that scroll position and a full-screen program's painted screen survive
    the round-trip, rather than being rebuilt from a ring buffer that may already have trimmed the
    escape sequences that drew them.
-->
<template>
  <DockStack
    :collapsed="collapsed"
    collapse-variant="close"
    body-persist
    @update:collapsed="collapsed = $event"
  >
    <template #title>
      <span class="term-scope-id" :title="scopeTitle">
        <img
          v-if="scopeIconSrc && !scopeIconFailed"
          :src="scopeIconSrc"
          class="term-scope-icon"
          alt=""
          @error="scopeIconFailed = true"
        />
        <i v-else class="fa-solid" :class="scopeProject ? 'fa-folder-open' : 'fa-terminal'"></i>
        <span class="term-scope-name">{{ scopeLabel }}</span>
      </span>
      <TerminalTabStrip />
    </template>
    <!-- #actions holds only ICON buttons that act on the PANEL, never on a shell: CLEAR / RESTART /
         KILL / OPEN stay gone (see docs/feat/in-app-terminal.md's migration table) — reachable via
         the tab chip's ✕ (kill), ✕+ (restart), or the OPEN popup (external). CLOSE is still the
         DockStack chevron itself (collapse-variant="close" above). -->
    <template #actions>
      <button
        v-if="externalTerminalsSupported"
        class="btn-tech btn-tech-secondary btn-terminal-action btn-external-term"
        title="External Terminal.app sessions — what is running in each"
        @click="openExternalTermModal"
      >
        <i class="fa-solid fa-terminal external-term-icon"></i>
        <span v-if="externalTermTotalCount > 0" class="external-term-badge">{{ externalTermTotalCount }}</span>
      </button>
      <!-- Hidden when the whole dock is collapsed to its headers: CSS owns the height in that state
           (useDockLayout.js's dockAllCollapsed), so the button would be a no-op. -->
      <button
        v-if="!dockAllCollapsed"
        class="btn-tech btn-tech-secondary btn-terminal-action"
        :title="dockMaximized ? 'Restore panel height' : 'Maximize panel'"
        @click="toggleDockMaximized"
      >
        <i class="fa-solid" :class="dockMaximized ? 'fa-down-left-and-up-right-to-center' : 'fa-up-right-and-down-left-from-center'"></i>
      </button>
    </template>
    <!-- No #peek slot: the terminal stack collapses to header-only (unlike the log stack, there
         is no "latest line" concept worth surfacing while collapsed). -->
    <div
      class="terminal-mount-wrap"
      @focusin="hasTerminalFocus = true"
      @focusout="hasTerminalFocus = false"
    >
      <!-- `template v-for` + `v-if` on the child (not both on the same node): Vue 3 gives v-if
           priority over v-for when they share a node, which would leave `t` out of scope — see
           https://vuejs.org/guide/essentials/list.html#v-for-with-v-if. -->
      <!-- `&& !collapsed` IS the re-fit mechanism, and the reason TerminalView needs no change:
           it already watches `active` and calls scheduleFit()+focus() on the false→true edge,
           written for exactly this case (a container that was only shown, never resized, so the
           ResizeObserver never fires). Collapsing drops `active` on every view; expanding raises
           it on the active one, which re-fits it. -->
      <template v-for="t in tabs" :key="t.id">
        <TerminalView
          v-if="activatedTabs.has(t.id)"
          v-show="t.id === activeTabId"
          :tab-id="t.id"
          :cwd="t.cwd"
          :active="t.id === activeTabId && !collapsed"
        />
      </template>
    </div>
  </DockStack>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import DockStack from '../DockStack.vue';
import TerminalView from '../TerminalView.vue';
import TerminalTabStrip from '../TerminalTabStrip.vue';
import { terminalStackCollapsed } from '../../composables/useTerminalPanel';
import { dockAllCollapsed, dockMaximized, toggleDockMaximized } from '../../composables/useDockLayout';
import {
  externalTerminalsSupported,
  openExternalTermModal,
} from '../../composables/useExternalTerminals';
import { externalTermCounts, externalTermGlobalCount } from '../../store/projectStore';
import {
  zoomInTerminalFont,
  zoomOutTerminalFont,
  resetTerminalFont,
} from '../../composables/useTerminalFont';
import { useTerminalTabs, activatedTabs } from '../../composables/useTerminalTabs';
import { projectIconSrc } from '../../utils/projectIcon';
import { iconTimestamp } from '../../store/projectStore';

// THE ref, not a copy: useTerminalPanel.js owns this stack's collapse state so that
// openProjectTerminal / openGlobalTerminal — the TERMINAL column, the header icon and the OPEN
// popup — can expand this exact stack. Collapsed by default: the terminal starts out of the way
// until something explicitly asks for it.
const collapsed = terminalStackCollapsed;

const { tabs, scope, scopeProject, activeTabId, newTab, closeTab, cycleTab } = useTerminalTabs();

// Every tab, everywhere, closed (⌘W on the last one or any other close path) -> nothing left to
// show, so the panel folds itself away instead of sitting open over an empty mount area.
watch(() => tabs.value.length, (n) => {
  if (n === 0) collapsed.value = true;
});

// Icon mechanism (named explicitly): utils/projectIcon.js's projectIconSrc(id, timestamp) +
// projectStore.iconTimestamp, with a @error fallback flag — the exact trio ProjectTable.vue,
// GitModal.vue and ProjectTasksModal.vue already use. Resolves the aki-devsync-icon:// protocol on
// the host and the mirrored data URI on a companion, so the phone's stack header shows the same
// icon for free.
const scopeIconFailed = ref(false);
watch(scope, () => { scopeIconFailed.value = false; }); // reset per scope, not globally

const scopeIconSrc = computed(() => (scopeProject.value ? projectIconSrc(scopeProject.value.id, iconTimestamp.value) : ''));

// ALL external Terminal.app sessions right now, not just the ones standing in a project directory:
// every project's own count (externalTermCounts) plus the global complement (sessions matching no
// project path) that the same scan already produces (useExternalTerminals.js).
const externalTermTotalCount = computed(() =>
  Object.values(externalTermCounts.value).reduce((sum, n) => sum + n, 0) + externalTermGlobalCount.value
);
// 'TERMINAL' spelled out, matching the project table's TERMINAL column — one canonical term for one
// concept. A project scope shows the project's own 4-char abbreviation instead: that is an identity,
// not the word for the feature.
const scopeLabel = computed(() => (scopeProject.value ? scopeProject.value.name.slice(0, 4).toUpperCase() : 'TERMINAL'));
const scopeTitle = computed(() => (scopeProject.value ? `Terminal group: ${scopeProject.value.name}` : 'Global terminal group'));

// ⌘T / ⌘W / ⌘⇧[ / ⌘⇧] / ⌘+ / ⌘- / ⌘0 — only while focus is inside this stack's terminal area (a
// hidden xterm textarea, or the compose input, living inside .pty-terminal triggers the
// focusin/focusout above). preventDefault() ONLY when a shortcut is actually recognised and
// handled, or e.g. ⌘W would additionally close the Tauri window itself. newTab/closeTab/cycleTab are already scope-filtered (useTerminalTabs.js),
// so these bindings need no changes for scopes: ⌘T/⌘W/⌘⇧[/⌘⇧] all act within the current group only.
const hasTerminalFocus = ref(false);

function onKeydown(e) {
  // macOS-only app: ⌘ only. Ctrl+T/W belong to the shell running inside the terminal, not to this
  // dock's tab management.
  if (!hasTerminalFocus.value || !e.metaKey) return;
  if (e.key === 't' && !e.shiftKey) {
    e.preventDefault();
    newTab();
  } else if (e.key === 'w' && !e.shiftKey) {
    e.preventDefault();
    closeTab(activeTabId.value);
    // `e.code`, not `e.key`: with Shift held, `e.key` reports the shifted character ('{'/'}'), not
    // the bracket, so a `e.key === '['` test never matches ⌘⇧[. The physical key code is shift-
    // invariant.
  } else if (e.shiftKey && e.code === 'BracketLeft') {
    e.preventDefault();
    cycleTab(-1);
  } else if (e.shiftKey && e.code === 'BracketRight') {
    e.preventDefault();
    cycleTab(1);
  }
  // ⌘+ / ⌘- / ⌘0 — VS Code's terminal zoom. `e.code` again, and for the same reason as the brackets
  // above: ⌘+ is physically ⌘⇧= on a US layout, so `e.key` reports '+' with Shift and '=' without.
  // The physical key is the stable test, and it also picks up the numeric keypad. No `!e.shiftKey`
  // guard here, unlike ⌘T/⌘W: Shift is part of how ⌘+ is typed at all. These sit after the bracket
  // branches, which they cannot shadow — BracketLeft/Right are different codes entirely.
  else if (e.code === 'Equal' || e.code === 'NumpadAdd') {
    e.preventDefault();
    zoomInTerminalFont();
  } else if (e.code === 'Minus' || e.code === 'NumpadSubtract') {
    e.preventDefault();
    zoomOutTerminalFont();
  } else if (e.code === 'Digit0' || e.code === 'Numpad0') {
    e.preventDefault();
    resetTerminalFont();
  }
}

onMounted(() => window.addEventListener('keydown', onKeydown, true));
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown, true));
</script>

<style scoped>
.terminal-mount-wrap {
  flex: 1;
  min-height: 0;
  display: flex;
}

.term-scope-id {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  margin-right: 6px;
  padding-right: 6px;
  border-right: 1px solid var(--border-card);
  color: var(--text-muted);
  font-size: 10px;
  font-weight: 800;
  letter-spacing: 0.5px;
  flex-shrink: 0;
}
.term-scope-icon { width: 14px; height: 14px; border-radius: 3px; object-fit: cover; }
.term-scope-name { line-height: 1; }

.btn-external-term {
  position: relative;
}

/* Same fa-terminal glyph as the in-app tab/header identity, boxed in a rounded outline so this one
   specific button reads as "a terminal in its own window" (external) rather than the bare glyph
   in-app terminal uses everywhere else. */
.external-term-icon {
  border: 1.5px solid currentColor;
  border-radius: 5px;
  padding: 2px 3px;
}

.external-term-badge {
  position: absolute;
  top: -6px;
  right: -6px;
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  border-radius: 999px;
  font-size: 9px;
  font-weight: 800;
  line-height: 16px;
  text-align: center;
  background: #94a3b8;
  color: #0b1220;
  box-shadow: 0 0 0 2px var(--bg-primary);
  pointer-events: none;
}
</style>

