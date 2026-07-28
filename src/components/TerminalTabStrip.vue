<!--
  Terminal tab strip — renders the ACTIVE SCOPE's tabs; always rendered — it carries the +
  affordance for the group. Chips reuse AgentUsageSlot.vue's `.tab-group` / `.tab` visual language
  as a base (not moved to main.css — CLAUDE.md: only WP-A's DockStack geometry pass owns main.css,
  everything else is scoped) but are no longer icon-only: each now shows a truncated title and its
  own close-x, sized via flex/min/max-width so the strip fills available width evenly.

  Extreme Narrow (CLAUDE.md): an exited tab is communicated by tinting ITS OWN chip's icon red plus
  a tooltip — no banner, no extra element. Each chip now carries a title label (truncated) and its
  own dedicated close-x region on the right — wide enough that the x has a hit-area separate from
  the title/activate area, so a mid-chip click can no longer land on it by accident (the old
  icon-only chip was too narrow for that separation, which is why the x used to only appear on the
  active chip's hover).

  Titles auto-follow the shell's own OSC title escapes (TerminalView.vue's `onTitleChange`), same
  as an external Terminal.app window's titlebar — and right-clicking a chip turns its title into an
  editable field for a manual rename, which then sticks (terminalTabsStore.js's `titleLocked`).
-->
<template>
  <div class="tab-group term-tab-group">
    <button
      v-for="t in tabs"
      :key="t.id"
      class="tab term-tab"
      :class="{ 'is-active': t.id === activeTabId, 'is-exited': tabAlive[t.id] === false }"
      :title="tabAlive[t.id] === false ? `${t.title} — exited` : t.title"
      @click="onChipClick(t)"
      @contextmenu.prevent="startRename(t)"
    >
      <i class="fa-solid fa-terminal icon-default"></i>
      <input
        v-if="renamingId === t.id"
        ref="renameInputEl"
        class="tab-title tab-title-input"
        :value="t.title"
        @click.stop
        @mousedown.stop
        @blur="commitRename(t, $event.target.value)"
        @keydown.enter="$event.target.blur()"
        @keydown.esc="renamingId = null"
      />
      <span v-else class="tab-title">{{ t.title }}</span>
      <i class="fa-solid fa-xmark icon-close" @click.stop="closeTab(t.id)"></i>
    </button>
    <button class="tab term-tab term-tab-add" :class="{ 'is-full': scopeFull }" :title="addTitle" @click="newTab">
      <i class="fa-solid fa-plus"></i>
    </button>
  </div>
</template>

<script setup>
import { computed, nextTick, ref } from 'vue'
import { useTerminalTabs, tabAlive } from '../composables/useTerminalTabs'
import { MAX_TABS_PER_SCOPE, renameTerminalTab } from '../store/terminalTabsStore'

const { scopedTabs: tabs, activeTabId, setActiveTab, newTab, closeTab } = useTerminalTabs()

// The per-group cap, carried by the tooltip the chip already had — no new element, no new row.
// Never the global ceiling: that one is a machine guard and stays invisible until it fires.
const scopeFull = computed(() => tabs.value.length >= MAX_TABS_PER_SCOPE)
const addTitle = computed(() =>
  scopeFull.value
    ? `This group is full, ${tabs.value.length} of ${MAX_TABS_PER_SCOPE}. Close a tab to open another.`
    : `New terminal tab in this group, ${tabs.value.length} of ${MAX_TABS_PER_SCOPE} (⌘T)`
)

function onChipClick(t) {
  setActiveTab(t.id)
}

// Rename, via right-click rather than a full context menu: the chip only ever has ONE thing a menu
// would offer (close already has its own always-visible x), so a menu with a single row would just
// be an extra click to reach the same result (design.A2 — no abstraction without evidence).
const renamingId = ref(null)
const renameInputEl = ref(null)

function startRename(t) {
  renamingId.value = t.id
  nextTick(() => {
    const el = Array.isArray(renameInputEl.value) ? renameInputEl.value[0] : renameInputEl.value
    el?.focus()
    el?.select()
  })
}

function commitRename(t, value) {
  renamingId.value = null
  renameTerminalTab(t.id, value)
}
</script>

<style scoped>
.tab-group {
  display: flex;
  align-items: center;
  gap: 2px;
  min-width: 0;
}

.tab {
  display: flex;
  align-items: center;
  justify-content: flex-start;
  /* Extreme Narrow: 1px 2px is the whole chip's padding. The BORDER (not whitespace) is what
     separates one tab from the next, so it is visible on every chip, not just the active one. */
  gap: 4px;
  background: rgba(15, 20, 30, 0.6);
  border: 1px solid var(--border-card);
  border-radius: 3px;
  padding: 1px 5px;
  box-sizing: border-box;
  cursor: pointer;
  /* Chip fills the header vertically minus 2px breathing top+bottom (4px total) — derived from
     main.css's :root --control-h (the one height every control in a row/header derives from), so
     this can never drift out of sync with the header's own height by hand. Width is now ~3x the
     old icon-only square (min 84px, was ~24px) so a title fragment and a close-x both fit with
     their own hit-areas; flex-grow lets tabs share leftover strip width evenly up to max-width
     rather than all pinning to the min. */
  height: calc(var(--control-h, 28px) - 4px);
  flex: 1 1 84px;
  min-width: 84px;
  max-width: 160px;
  font-size: 13px;
  font-weight: 800;
  letter-spacing: 0.4px;
  line-height: 1;
  color: var(--text-darker);
  opacity: 0.6;
  transition: opacity 0.15s ease, background 0.15s ease, color 0.15s ease;
}

.tab-title {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 600;
  font-size: 11px;
  text-align: left;
}
.tab-title-input {
  background: rgba(0, 0, 0, 0.4);
  border: 1px solid rgba(96, 165, 250, 0.55);
  border-radius: 2px;
  color: inherit;
  font: inherit;
  padding: 0 2px;
  outline: none;
}

.tab:hover {
  opacity: 0.9;
}
.tab.is-active {
  opacity: 1;
  background: rgba(96, 165, 250, 0.16);
  color: #e5e7eb;
  border-color: rgba(96, 165, 250, 0.55);
}

/* 'unknown' renders exactly like `true` (normal) — only a STATED death (=== false) tints. */
.term-tab.is-exited { border-color: var(--accent-red); }
.term-tab.is-exited .icon-default {
  color: var(--accent-red);
}

/* Close-x is now always visible, not hover-only — the chip is wide enough (min 84px) that it has
   its own hit-area to the right of the title, distinct from the activate area, so it no longer
   needs to hide to avoid stray clicks the way the old icon-only chip did. */
.term-tab .icon-close {
  flex: 0 0 auto;
  opacity: 0.5;
}
.term-tab .icon-close:hover {
  opacity: 1;
}

.term-tab-add {
  flex: 0 0 auto;
  min-width: calc(var(--control-h, 28px) - 4px);
  max-width: none;
  justify-content: center;
  opacity: 0.5;
}
.term-tab-add:hover {
  opacity: 1;
}

/* At the group's cap the + stays where it is and keeps its resting mute on hover — DIMMED, NEVER
   HIDDEN: a + that vanishes reads as a bug, a + that will not brighten reads as a limit. The click
   still fires and still Toasts the reason, so the state is discoverable without a tooltip too. */
.term-tab-add.is-full {
  cursor: not-allowed;
}
.term-tab-add.is-full:hover {
  opacity: 0.5;
}
</style>
