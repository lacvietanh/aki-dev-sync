<template>
  <div class="task-cell-wrapper">
    <!-- Trigger button: tasks icon + open-count badge -->
    <!-- Muted state with tooltip when project notes are not writable (UI Extreme Narrow). -->
    <button
      class="btn-cell-trigger"
      :class="{ 'is-attention': writable && summary.doing > 0, 'is-muted': !writable }"
      :aria-label="`Tasks for ${project.name}`"
      :title="writable ? 'Tasks' : `Tasks unavailable — ${notesEntry.error || notesEntry.status}`"
      @click="openTasksModal(project)"
    >
      <i class="fa-solid fa-list-check"></i>
      <TaskCountBadges v-if="writable" :pinned="summary.doing" :open="summary.todo" />
    </button>
  </div>
</template>

<script setup>
import { computed } from 'vue'
import { openTasksModal, openTaskCount, doingCount, totalTaskCount } from '../composables/useProjectTasks'
import { isProjectNotesWritable } from '../composables/useProjectNotes'
import { getProjectNotesEntry } from '../store/projectNotesStore'
import TaskCountBadges from './tasks/TaskCountBadges.vue'

const props = defineProps({
  project: { type: Object, required: true },
})

// `projectNotes` is a mirrored ref, so reading it inside a computed is what makes this cell update live on the Mac AND on a paired phone with no extra wiring.
const notesEntry = computed(() => getProjectNotesEntry(props.project.id))
const writable = computed(() => isProjectNotesWritable(props.project.id))

const summary = computed(() => {
  const total = totalTaskCount(props.project)
  const open = openTaskCount(props.project)
  const doing = doingCount(props.project)
  return { total, open, doing, todo: open - doing, done: total - open }
})
</script>

<style scoped>
/* Wrapper for shared .btn-cell-trigger button pattern (main.css). */
.task-cell-wrapper {
  display: inline-flex;
}
</style>
