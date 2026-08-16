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

// Scopes (terminal-ownership-model.md §7/S5): project object (row cell) or GLOBAL_SCOPE token (header). SSoT count source.
const props = defineProps({
  scope: { type: [Object, String], required: true },
})

const { openProjectTerminal, openGlobalTerminal } = useTerminalTabs()

const isGlobal = computed(() => props.scope === GLOBAL_SCOPE)
// Matches useTerminalTabs scopeOf convention: project id for project tabs, null for global group.
const scopeId = computed(() => (isGlobal.value ? null : props.scope.id))

const tabCount = computed(() => terminalTabs.value.filter((t) => (t.projectId ?? null) === scopeId.value).length)
// .value is load-bearing: tabAlive is a ref, which does not auto-unwrap in <script setup> JS.
const hasExited = computed(() => terminalTabs.value.some((t) => (t.projectId ?? null) === scopeId.value && tabAlive.value[t.id] === false))
// Live count: host re-scans process table every 5s (useExternalTerminals.js).
const externalCount = computed(() => (isGlobal.value ? externalTermGlobalCount.value : externalTermCounts.value[scopeId.value] || 0))

const ariaLabel = computed(() => (isGlobal.value ? 'Global terminal' : `Terminal for ${props.scope.name}`))

function onClick() {
  if (isGlobal.value) openGlobalTerminal()
  else openProjectTerminal(props.scope)
}

// Composes one line per non-zero fact without creating extra DOM elements.
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

<!-- Geometry and states from .btn-cell-trigger in main.css (shared with TaskCell.vue). Active group uses .is-live. -->
