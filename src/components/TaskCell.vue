<template>
  <div class="task-cell-wrapper">
    <!-- Trigger button: tasks icon + open-count badge -->
    <!-- Not writable (unmounted volume, corrupt file): the button MUTES and its tooltip carries the
         reason, and the badges render nothing at all rather than `0` — a zero is a claim ("this
         project has no tasks") and we do not know that; we know we could not read the file. Reusing
         the class binding and the title the button already had, per Extreme Narrow: no banner, no
         extra row, no new element. -->
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
/* Geometry and states come from main.css's .btn-cell-trigger pattern (shared with TerminalScopeButton.vue,
   which used to hold a byte-identical copy of it). Nothing left to say locally. */
.task-cell-wrapper {
  display: inline-flex;
}
</style>
