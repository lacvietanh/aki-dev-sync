<!--
  Terminal tab strip — renders the ACTIVE SCOPE's tabs; always rendered — it carries the +
  affordance for the group. Icon-only chips, reusing AgentUsageSlot.vue's `.tab-group` / `.tab`
  visual language (the ~20 lines of CSS below are copied from there, not moved to main.css —
  CLAUDE.md: only WP-A's DockStack geometry pass owns main.css, everything else is scoped).

  Extreme Narrow (CLAUDE.md): an exited tab is communicated by tinting ITS OWN chip's icon red plus
  a tooltip — no banner, no extra element. The close-x only ever replaces the glyph on the ALREADY
  ACTIVE chip's hover (CSS only, no JS hover state) — swapping it on any hovered chip meant a
  mid-chip click on an inactive tab could land on the x and close it instead of activating it.
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
    >
      <i class="fa-solid fa-terminal icon-default"></i>
      <i class="fa-solid fa-xmark icon-close" @click.stop="closeTab(t.id)"></i>
    </button>
    <button class="tab term-tab term-tab-add" title="New terminal tab in this group (⌘T)" @click="newTab">
      <i class="fa-solid fa-plus"></i>
    </button>
  </div>
</template>

<script setup>
import { useTerminalTabs, tabAlive } from '../composables/useTerminalTabs'

const { scopedTabs: tabs, activeTabId, setActiveTab, newTab, closeTab } = useTerminalTabs()

function onChipClick(t) {
  setActiveTab(t.id)
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
  justify-content: center;
  /* Extreme Narrow: 1px 2px is the whole chip's padding. The BORDER (not whitespace) is what
     separates one tab from the next, so it is visible on every chip, not just the active one. */
  gap: 2px;
  background: rgba(15, 20, 30, 0.6);
  border: 1px solid var(--border-card);
  border-radius: 3px;
  padding: 1px 2px;
  box-sizing: border-box;
  cursor: pointer;
  /* Chip fills the header vertically minus 2px breathing top+bottom (4px total) — derived from
     main.css's :root --control-h (the one height every control in a row/header derives from), so
     this can never drift out of sync with the header's own height by hand. min-width mirrors height
     so an icon-only chip is at minimum square, per Extreme Narrow: the tappable/icon area should
     grow, not float tiny in leftover space. */
  height: calc(var(--control-h, 28px) - 4px);
  min-width: calc(var(--control-h, 28px) - 4px);
  font-size: 13px;
  font-weight: 800;
  letter-spacing: 0.4px;
  line-height: 1;
  color: var(--text-darker);
  opacity: 0.6;
  transition: opacity 0.15s ease, background 0.15s ease, color 0.15s ease;
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

/* Hover swaps the terminal glyph for a close-x — CSS-only, no JS hover state — but ONLY on the
   already-active chip. An inactive chip keeps its glyph on hover so a click anywhere on it always
   activates; only the active chip's hover offers a close target. */
.term-tab .icon-close {
  display: none;
}
.term-tab.is-active:hover .icon-default {
  display: none;
}
.term-tab.is-active:hover .icon-close {
  display: inline;
}

.term-tab-add {
  opacity: 0.5;
}
.term-tab-add:hover {
  opacity: 1;
}
</style>
