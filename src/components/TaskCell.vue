<template>
  <div class="task-cell-wrapper">
    <!-- Trigger button: tasks icon + open-count badge -->
    <button
      class="btn-task-trigger"
      :class="{ 'has-doing': summary.doing > 0 }"
      :aria-label="`Tasks for ${project.name}`"
      title="Tasks"
      @click="openTasksModal(project)"
    >
      <i class="fa-solid fa-list-check"></i>
      <!-- Two overlay badges: pinned-open (amber, top) and normal-open (white, bottom) -->
      <span v-if="summary.doing > 0" class="task-badge task-badge-pin">{{ summary.doing }}</span>
      <span v-if="summary.todo > 0" class="task-badge task-badge-normal">{{ summary.todo }}</span>
    </button>
  </div>
</template>

<script setup>
import { computed } from 'vue'
import { openTasksModal } from '../composables/useProjectTasks'
import { openTaskCount, doingCount } from '../composables/useProjectTasks'

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
.task-cell-wrapper {
  display: inline-flex;
}

/* Trigger — matches the row's btn-tech-secondary language */
.btn-task-trigger {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 28px;
  border-radius: 4px;
  border: 1px solid rgba(255, 255, 255, 0.2);
  background: rgba(15, 20, 30, 0.6);
  color: var(--text-muted);
  font-size: 13px;
  cursor: pointer;
  transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
}

.btn-task-trigger:hover {
  color: var(--accent-cyan);
  border-color: rgba(0, 210, 255, 0.5);
  background: rgba(0, 210, 255, 0.1);
  box-shadow: 0 0 10px rgba(0, 210, 255, 0.25);
}

.btn-task-trigger.has-doing {
  color: var(--accent-amber);
  border-color: rgba(255, 140, 0, 0.5);
}

.btn-task-trigger.has-doing:hover {
  background: rgba(255, 140, 0, 0.12);
  box-shadow: 0 0 10px rgba(255, 140, 0, 0.25);
}

.task-badge {
  position: absolute;
  right: -6px;
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  border-radius: 999px;
  font-size: 9px;
  font-weight: 800;
  line-height: 16px;
  text-align: center;
  box-shadow: 0 0 0 2px var(--bg-primary);
}

/* Pinned open tasks — amber, top-right (matches the pin colour) */
.task-badge-pin {
  top: -6px;
  background: var(--accent-amber);
  color: #1a1000;
}

/* Normal open tasks — white, bottom-right */
.task-badge-normal {
  bottom: -6px;
  background: #fff;
  color: #04121a;
}
</style>
