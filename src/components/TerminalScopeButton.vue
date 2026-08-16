<template>
  <button
    class="btn-cell-trigger"
    :class="{ 'is-live': tabCount > 0 }"
    :aria-label="ariaLabel"
    :title="cellTitle"
    @click="onClick"
  >
    <i class="fa-solid fa-terminal"></i>
    <TerminalCountBadges :tabs="tabCount" :external="externalCount" :exited="hasExited" />
  </button>
</template>

<script setup>
import { computed } from 'vue'
import { terminalTabs, MAX_TABS_PER_SCOPE, GLOBAL_SCOPE } from '../store/terminalTabsStore'
import { externalTermCounts, externalTermGlobalCount } from '../store/projectStore'
import { useTerminalTabs, tabAlive } from '../composables/useTerminalTabs'
import TerminalCountBadges from './terminal/TerminalCountBadges.vue'

// One button, two scopes (docs/plan/done/terminal-ownership-model.md §7/S5): a project object for the per-row TERMINAL cell, or the GLOBAL_SCOPE token for the column header. Neither caller computes its own count any more — both read the same scope-keyed source here, so they cannot disagree.
const props = defineProps({
  scope: { type: [Object, String], required: true },
})

const { openProjectTerminal, openGlobalTerminal } = useTerminalTabs()

const isGlobal = computed(() => props.scope === GLOBAL_SCOPE)
// Tabs store a project's tabs under its id and the global group's under `null` (useTerminalTabs.js's own scopeOf), so this mirrors that convention rather than inventing a second one.
const scopeId = computed(() => (isGlobal.value ? null : props.scope.id))

const tabCount = computed(() => terminalTabs.value.filter((t) => (t.projectId ?? null) === scopeId.value).length)
// `.value` is load-bearing: `tabAlive` is a ref and refs only auto-unwrap in TEMPLATES, never in `<script setup>` JS.
const hasExited = computed(() => terminalTabs.value.some((t) => (t.projectId ?? null) === scopeId.value && tabAlive.value[t.id] === false))
// Live, not accumulated: the host re-scans the process table every 5s (composables/useExternalTerminals.js).
const externalCount = computed(() => (isGlobal.value ? externalTermGlobalCount.value : externalTermCounts.value[scopeId.value] || 0))

const ariaLabel = computed(() => (isGlobal.value ? 'Global terminal' : `Terminal for ${props.scope.name}`))

function onClick() {
  if (isGlobal.value) openGlobalTerminal()
  else openProjectTerminal(props.scope)
}

// Composes honestly, one line per fact that is non-zero (Extreme Narrow: no new element per fact).
const cellTitle = computed(() => {
  const noun = isGlobal.value ? 'Global terminal' : 'In-app terminal'
  const lines = [
    tabCount.value === 0
      ? noun
      : tabCount.value >= MAX_TABS_PER_SCOPE
        ? `${noun}, ${tabCount.value} of ${MAX_TABS_PER_SCOPE} tabs in this group. Close one to open another.`
        : `${noun}, ${tabCount.value} of ${MAX_TABS_PER_SCOPE} tabs in this group`,
  ]
  if (externalCount.value > 0) {
    lines.push(
      isGlobal.value
        ? `${externalCount.value} external Terminal window(s) not standing in any listed project`
        : `${externalCount.value} external Terminal window(s) standing in this folder now`
    )
  }
  if (hasExited.value) lines.push('A shell in this group has exited')
  return lines.join('\n')
})
</script>

<!-- No scoped styles: geometry and states come from main.css's .btn-cell-trigger pattern, shared with TaskCell.vue. An open group reads as live via `.is-live` — no extra element. -->
