<!-- Terminal tab strip for active scope tabs with title, pin, close, and inline rename. -->
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
      <span
        class="tab-icon-btn icon-pin"
        :class="{ 'is-pinned': t.pinned }"
        :title="t.pinned ? 'Unpin — keep in this group only' : 'Pin — show in every group'"
        :aria-label="t.pinned ? 'Unpin tab' : 'Pin tab'"
        role="button"
        tabindex="0"
        @click.stop="togglePin(t.id)"
        @keydown.enter.stop="togglePin(t.id)"
      >
        <i class="fa-solid fa-thumbtack"></i>
      </span>
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
      <span
        class="tab-icon-btn icon-close"
        title="Close tab (⌘W)"
        aria-label="Close tab"
        role="button"
        tabindex="0"
        @click.stop="closeTab(t.id)"
        @keydown.enter.stop="closeTab(t.id)"
      >
        <i class="fa-solid fa-xmark"></i>
      </span>
    </button>
    <button class="tab term-tab term-tab-add" :class="{ 'is-full': scopeFull }" :title="addTitle" @click="newTab">
      <i class="fa-solid fa-plus"></i>
    </button>
  </div>
</template>

<script setup>
import { computed, nextTick, ref } from 'vue'
import { useTerminalTabs, tabAlive } from '../composables/useTerminalTabs'
import { MAX_TABS_PER_SCOPE, renameTerminalTab, toggleTabPinned } from '../store/terminalTabsStore'

const { scopedTabs: tabs, ownedScopeTabs, activeTabId, setActiveTab, newTab, closeTab } = useTerminalTabs()

// Scope cap uses ownedScopeTabs so foreign pinned tabs do not count against this group.
const scopeFull = computed(() => ownedScopeTabs.value.length >= MAX_TABS_PER_SCOPE)
const addTitle = computed(() =>
  scopeFull.value
    ? `This group is full, ${ownedScopeTabs.value.length} of ${MAX_TABS_PER_SCOPE}. Close a tab to open another.`
    : `New terminal tab in this group, ${ownedScopeTabs.value.length} of ${MAX_TABS_PER_SCOPE} (⌘T)`
)

function onChipClick(t) {
  setActiveTab(t.id)
}

function togglePin(id) {
  toggleTabPinned(id)
}

// Right-click triggers inline title renaming.
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
  /* Extreme Narrow: border separates tabs with minimal padding. */
  gap: 4px;
  background: rgba(15, 20, 30, 0.6);
  border: 1px solid var(--border-card);
  border-radius: 3px;
  padding: 1px 5px;
  box-sizing: border-box;
  cursor: pointer;
  /* Height derived from --control-h with flex sizing across min/max width bounds. */
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

.tab-icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 2px;
  border-radius: 2px;
}

/* Pin toggle reuses active accent color within existing chip gap. */
.term-tab .icon-pin {
  flex: 0 0 auto;
  font-size: 10px;
  opacity: 0.35;
}
.term-tab .icon-pin:hover {
  opacity: 0.8;
}
.term-tab .icon-pin.is-pinned {
  opacity: 1;
  color: var(--accent-blue);
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

/* 'unknown' renders like true (normal); only explicit exit (=== false) tints. */
.term-tab.is-exited { border-color: var(--accent-red); }
.term-tab.is-exited .icon-default {
  color: var(--accent-red);
}

/* Close button has dedicated hit-area to the right of the title. */
.term-tab .icon-close {
  flex: 0 0 auto;
  opacity: 0.5;
  font-size: 11px;
}
.term-tab .icon-close:hover {
  opacity: 1;
  color: var(--accent-red);
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

/* Dimmed on hover when cap is reached instead of hidden. */
.term-tab-add.is-full {
  cursor: not-allowed;
}
.term-tab-add.is-full:hover {
  opacity: 0.5;
}
</style>
