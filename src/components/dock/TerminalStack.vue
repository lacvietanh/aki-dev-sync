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
  - COLLAPSING THE STACK: unchanged from WP-A. `DockStack.vue`'s default slot only renders while
    expanded (`v-if`/`v-else` on `collapsed`, not `v-show`), so collapsing the terminal stack still
    unmounts every TerminalView, and expanding it re-mounts them all. `DockStack.vue` is out of this
    package's file list (single-owner: WP-A), so changing that v-if to v-show to also survive a
    STACK collapse is left as a follow-up rather than done here. Accepted as-is because it is exactly
    the behaviour WP-A already documented and verified as harmless: `pty_spawn` is idempotent (T-3),
    scrollback rehydrates on remount, and tri-state `alive` starts at `'unknown'` on every fresh
    mount, so there is no red flash — the one thing this DOES cost with N tabs is re-mounting N
    xterm instances (and N scrollback fetches) on every collapse→expand, instead of one.
-->
<template>
  <DockStack
    :collapsed="collapsed"
    collapse-variant="close"
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
    <!-- No #actions slot: CLOSE is the DockStack chevron itself (collapse-variant="close" above),
         and CLEAR / RESTART / KILL / OPEN are gone (see docs/feat/in-app-terminal.md's migration
         table) — reachable via the tab chip's ✕ (kill), ✕+ (restart), or the OPEN popup (external). -->
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
      <template v-for="t in tabs" :key="t.id">
        <TerminalView
          v-if="activatedTabs.has(t.id)"
          v-show="t.id === activeTabId"
          :tab-id="t.id"
          :cwd="t.cwd"
          :active="t.id === activeTabId"
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
import { useTerminalTabs, activatedTabs } from '../../composables/useTerminalTabs';
import { projectIconSrc } from '../../utils/projectIcon';
import { iconTimestamp } from '../../store/projectStore';

// THE ref, not a copy: useTerminalPanel.js owns this stack's collapse state so that
// openProjectTerminal / openGlobalTerminal — the TERMINAL column, the header icon and the OPEN
// popup — can expand this exact stack. Collapsed by default: the terminal starts out of the way
// until something explicitly asks for it.
const collapsed = terminalStackCollapsed;

const { tabs, scope, scopeProject, activeTabId, newTab, closeTab, cycleTab } = useTerminalTabs();

// Icon mechanism (named explicitly): utils/projectIcon.js's projectIconSrc(id, timestamp) +
// projectStore.iconTimestamp, with a @error fallback flag — the exact trio ProjectTable.vue,
// GitModal.vue and ProjectTasksModal.vue already use. Resolves the aki-devsync-icon:// protocol on
// the host and the mirrored data URI on a companion, so the phone's stack header shows the same
// icon for free.
const scopeIconFailed = ref(false);
watch(scope, () => { scopeIconFailed.value = false; }); // reset per scope, not globally

const scopeIconSrc = computed(() => (scopeProject.value ? projectIconSrc(scopeProject.value.id, iconTimestamp.value) : ''));
// 'TERMINAL' spelled out, matching the project table's TERMINAL column — one canonical term for one
// concept. A project scope shows the project's own 4-char abbreviation instead: that is an identity,
// not the word for the feature.
const scopeLabel = computed(() => (scopeProject.value ? scopeProject.value.name.slice(0, 4).toUpperCase() : 'TERMINAL'));
const scopeTitle = computed(() => (scopeProject.value ? `Terminal group: ${scopeProject.value.name}` : 'Global terminal group'));

// ⌘T / ⌘W / ⌘⇧[ / ⌘⇧] — only while focus is inside this stack's terminal area (a hidden xterm
// textarea living inside .pty-terminal triggers the focusin/focusout above). preventDefault() ONLY
// when a shortcut is actually recognised and handled, or e.g. ⌘W would additionally close the
// Tauri window itself. newTab/closeTab/cycleTab are already scope-filtered (useTerminalTabs.js),
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
</style>

