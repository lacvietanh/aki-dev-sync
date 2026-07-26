<template>
  <button
    class="btn-cell-trigger"
    :class="{ 'is-live': tabCount > 0 }"
    :aria-label="`Terminal for ${project.name}`"
    :title="cellTitle"
    @click="openProjectTerminal(project)"
  >
    <i class="fa-solid fa-terminal"></i>
    <TerminalCountBadges :tabs="tabCount" :external="externalCount" :exited="hasExited" />
  </button>
</template>

<script setup>
import { computed } from 'vue'
import { terminalTabs } from '../store/terminalTabsStore'
import { externalTermCounts } from '../store/projectStore'
import { useTerminalTabs, tabAlive } from '../composables/useTerminalTabs'
import TerminalCountBadges from './terminal/TerminalCountBadges.vue'

const props = defineProps({
  project: { type: Object, required: true },
})

const { openProjectTerminal } = useTerminalTabs()

const tabCount = computed(() => terminalTabs.value.filter((t) => t.projectId === props.project.id).length)
// `.value` is load-bearing: `tabAlive` is a ref and refs only auto-unwrap in TEMPLATES, never in
// `<script setup>` JS. Indexing the ref itself always yielded undefined, so this badge never lit up.
const hasExited = computed(() => terminalTabs.value.some((t) => t.projectId === props.project.id && tabAlive.value[t.id] === false))
// Live, not accumulated: the host re-scans the process table every 5s
// (composables/useExternalTerminals.js), so this falls back to 0 when the last window is closed.
const externalCount = computed(() => externalTermCounts.value[props.project.id] || 0)

// Composes honestly, one line per fact that is non-zero.
const cellTitle = computed(() => {
  const lines = [tabCount.value > 0 ? `In-app terminal — ${tabCount.value} tab(s) in this group` : 'In-app terminal']
  if (externalCount.value > 0) lines.push(`${externalCount.value} external Terminal window(s) standing in this folder now`)
  if (hasExited.value) lines.push('A shell in this group has exited')
  return lines.join('\n')
})
</script>

<!-- No scoped styles: geometry and states come from main.css's .btn-cell-trigger pattern, shared
     with TaskCell.vue. An open group reads as live via `.is-live` — no extra element. -->
