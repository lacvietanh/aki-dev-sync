<template>
  <div class="task-cell-wrapper">
    <!-- Trigger button: tasks icon + open-count badge -->
    <button
      class="btn-cell-trigger"
      :class="{ 'is-attention': summary.doing > 0 }"
      :aria-label="`Tasks for ${project.name}`"
      title="Tasks"
      @click="openTasksModal(project)"
    >
      <i class="fa-solid fa-list-check"></i>
      <TaskCountBadges :pinned="summary.doing" :open="summary.todo" />
    </button>
  </div>
</template>

<script setup>
import { computed } from 'vue'
import { openTasksModal } from '../composables/useProjectTasks'
import { openTaskCount, doingCount } from '../composables/useProjectTasks'
import TaskCountBadges from './tasks/TaskCountBadges.vue'

const props = defineProps({
  project: { type: Object, required: true },
})

const summary = computed(() => {
  const total = Array.isArray(props.project.tasks) ? props.project.tasks.length : 0
  const open = openTaskCount(props.project)
  const doing = doingCount(props.project)
  return { total, open, doing, todo: open - doing, done: total - open }
})
</script>

<style scoped>
/* Geometry and states come from main.css's .btn-cell-trigger pattern (shared with TerminalCell.vue,
   which used to hold a byte-identical copy of it). Nothing left to say locally. */
.task-cell-wrapper {
  display: inline-flex;
}
</style>
