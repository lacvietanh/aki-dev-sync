<template>
  <div class="task-cell-wrapper" @mouseenter="onEnter($event)" @mouseleave="onLeave()">
    <!-- Trigger button: tasks icon + open-count badge -->
    <button
      class="btn-task-trigger"
      :class="{ 'has-doing': summary.doing > 0, 'is-active': isOpen }"
      :aria-label="`Tasks for ${project.name}`"
      title="Tasks"
    >
      <i class="fa-solid fa-list-check"></i>
      <span v-if="summary.open > 0" class="task-badge">{{ summary.open }}</span>
    </button>

    <!-- Full task list, revealed on hover -->
    <transition name="popup-fade">
      <div
        v-if="isOpen"
        class="task-popup"
        :style="popupPositionStyle"
        @mouseenter="onPopupEnter()"
        @mouseleave="onLeave()"
        @focusin="hasFocusInside = true"
        @focusout="hasFocusInside = false"
      >
        <div class="task-popup-header">
          <span class="tph-title" :title="project.name">
            <i class="fa-solid fa-list-check"></i>{{ project.name }}
          </span>
          <span v-if="summary.total > 0" class="tph-summary">
            <span v-if="summary.doing > 0" class="tph-stat s-doing">{{ summary.doing }} doing</span>
            <span v-if="summary.todo > 0" class="tph-stat s-todo">{{ summary.todo }} todo</span>
            <span v-if="summary.done > 0" class="tph-stat s-done">{{ summary.done }} done</span>
          </span>
        </div>

        <!-- Add task -->
        <div class="task-add-row">
          <input
            v-model="newTitle"
            @keyup.enter="submitNew()"
            type="text"
            class="task-add-input"
            placeholder="Add a task"
            maxlength="200"
          />
          <button class="task-add-btn" :disabled="!newTitle.trim()" @click="submitNew()" aria-label="Add task" title="Add task">
            <i class="fa-solid fa-plus"></i>
          </button>
        </div>

        <!-- List -->
        <div class="task-list">
          <div v-if="orderedTasks.length === 0" class="task-empty">
            <i class="fa-regular fa-circle-check"></i>
            <span>No tasks yet. Add what you are working on.</span>
          </div>

          <div
            v-for="task in orderedTasks"
            :key="task.id"
            class="task-item"
            :class="['is-' + task.status, { 'has-detail': !!task.detail }]"
          >
            <button
              class="task-tag"
              :class="'tag-' + task.status"
              @click="cycleStatus(project, task)"
              :aria-label="`Status: ${task.status}. Click to change.`"
              :title="`${task.status} (click to change)`"
            >
              {{ task.status }}
            </button>

            <div class="task-body">
              <input
                :value="task.title"
                @change="updateTaskTitle(project, task, $event.target.value)"
                type="text"
                class="task-title-input"
                maxlength="200"
              />
              <input
                :value="task.detail"
                @change="updateTaskDetail(project, task, $event.target.value)"
                type="text"
                class="task-detail-input"
                placeholder="Add detail"
                maxlength="500"
              />
            </div>

            <span class="task-time" :title="'Updated ' + timeAgo(task.updated_at) + ' ago'">{{ timeAgo(task.updated_at) }}</span>

            <button class="task-del" @click="removeTask(project, task)" aria-label="Delete task" title="Delete task">
              <i class="fa-solid fa-xmark"></i>
            </button>
          </div>
        </div>
      </div>
    </transition>
  </div>
</template>

<script setup>
import { ref, computed, onUnmounted } from 'vue'
import {
  sortedTasks, openTaskCount, doingCount,
  addTask, updateTaskTitle, updateTaskDetail, cycleStatus, removeTask,
} from '../composables/useProjectTasks'

const props = defineProps({
  project: { type: Object, required: true },
})

const isOpen = ref(false)
const hasFocusInside = ref(false)
const newTitle = ref('')
const popupPositionStyle = ref({ top: '32px', bottom: 'auto', transformOrigin: 'top left' })
let closeTimer = null

const orderedTasks = computed(() => sortedTasks(props.project))

const summary = computed(() => {
  const total = Array.isArray(props.project.tasks) ? props.project.tasks.length : 0
  const open = openTaskCount(props.project)
  const doing = doingCount(props.project)
  return { total, open, doing, todo: open - doing, done: total - open }
})

function onEnter(event) {
  clearTimeout(closeTimer)
  isOpen.value = true
  if (event) {
    const rect = event.currentTarget.getBoundingClientRect()
    if (window.innerHeight - rect.bottom < 360) {
      popupPositionStyle.value = { top: 'auto', bottom: '32px', transformOrigin: 'bottom left' }
    } else {
      popupPositionStyle.value = { top: '32px', bottom: 'auto', transformOrigin: 'top left' }
    }
  }
}

function onLeave() {
  closeTimer = setTimeout(() => {
    // Keep the popup open while the user is editing inside it.
    if (!hasFocusInside.value) isOpen.value = false
  }, 180)
}

function onPopupEnter() {
  clearTimeout(closeTimer)
}

function submitNew() {
  if (addTask(props.project, newTitle.value)) newTitle.value = ''
}

function timeAgo(ts) {
  if (!ts) return ''
  const s = Math.floor(Date.now() / 1000) - Math.floor(ts / 1000)
  if (s < 60) return 'now'
  if (s < 3600) return `${Math.floor(s / 60)}m`
  if (s < 86400) return `${Math.floor(s / 3600)}h`
  return `${Math.floor(s / 86400)}d`
}

onUnmounted(() => clearTimeout(closeTimer))
</script>

<style scoped>
.task-cell-wrapper {
  position: relative;
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

.btn-task-trigger:hover,
.btn-task-trigger.is-active {
  color: var(--accent-cyan);
  border-color: rgba(0, 210, 255, 0.5);
  background: rgba(0, 210, 255, 0.1);
  box-shadow: 0 0 10px rgba(0, 210, 255, 0.25);
}

.btn-task-trigger.has-doing {
  color: var(--accent-amber);
  border-color: rgba(255, 140, 0, 0.5);
}

.btn-task-trigger.has-doing:hover,
.btn-task-trigger.has-doing.is-active {
  background: rgba(255, 140, 0, 0.12);
  box-shadow: 0 0 10px rgba(255, 140, 0, 0.25);
}

.task-badge {
  position: absolute;
  top: -6px;
  right: -6px;
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  border-radius: 999px;
  background: var(--accent-cyan);
  color: #04121a;
  font-size: 9px;
  font-weight: 800;
  line-height: 16px;
  text-align: center;
  box-shadow: 0 0 0 2px var(--bg-primary);
}

.btn-task-trigger.has-doing .task-badge {
  background: var(--accent-amber);
  color: #1a1000;
}

/* Popup */
.task-popup {
  position: absolute;
  left: 0;
  z-index: 99;
  width: 330px;
  background: rgba(16, 22, 33, 0.98);
  border: 1px solid var(--border-card);
  border-radius: 8px;
  padding: 10px;
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.65);
  backdrop-filter: blur(8px);
  will-change: transform, opacity;
}

.popup-fade-enter-active,
.popup-fade-leave-active {
  transition: opacity 0.15s ease, transform 0.15s cubic-bezier(0.2, 0.8, 0.2, 1);
}

.popup-fade-enter-from,
.popup-fade-leave-to {
  opacity: 0;
  transform: scale(0.96);
}

/* Header */
.task-popup-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding-bottom: 8px;
  margin-bottom: 8px;
  border-bottom: 1px solid var(--border-color);
}

.tph-title {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  font-size: 11px;
  font-weight: 700;
  color: var(--text-light);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.tph-title i {
  color: var(--accent-cyan);
  flex-shrink: 0;
}

.tph-summary {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.tph-stat {
  font-size: 9px;
  font-weight: 800;
  letter-spacing: 0.03em;
  text-transform: uppercase;
}

.tph-stat.s-doing { color: var(--accent-amber); }
.tph-stat.s-todo { color: var(--accent-cyan); }
.tph-stat.s-done { color: var(--text-darker); }

/* Add row */
.task-add-row {
  display: flex;
  gap: 6px;
  margin-bottom: 8px;
}

.task-add-input {
  flex: 1;
  min-width: 0;
  height: 28px;
  padding: 0 10px;
  border-radius: 4px;
  border: 1px solid var(--border-color);
  background: rgba(0, 0, 0, 0.25);
  color: var(--text-light);
  font-size: 12px;
  transition: border-color 0.15s;
}

.task-add-input::placeholder { color: var(--text-darker); }

.task-add-input:focus {
  outline: none;
  border-color: var(--accent-cyan);
}

.task-add-btn {
  width: 28px;
  height: 28px;
  flex-shrink: 0;
  border-radius: 4px;
  border: 1px solid rgba(0, 210, 255, 0.4);
  background: rgba(0, 210, 255, 0.12);
  color: var(--accent-cyan);
  cursor: pointer;
  transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
}

.task-add-btn:hover:not(:disabled) {
  background: rgba(0, 210, 255, 0.2);
  box-shadow: 0 0 8px rgba(0, 210, 255, 0.3);
}

.task-add-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

/* List */
.task-list {
  max-height: 300px;
  overflow-y: auto;
  margin: 0 -2px;
  padding: 0 2px;
}

.task-list::-webkit-scrollbar { width: 6px; }
.task-list::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.12);
  border-radius: 3px;
}

.task-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 20px 4px;
  text-align: center;
  font-size: 12px;
  color: var(--text-darker);
}

.task-empty i { font-size: 18px; opacity: 0.6; }

/* Item — left rail color-codes status for fast scanning */
.task-item {
  display: flex;
  align-items: flex-start;
  gap: 7px;
  padding: 7px 7px 7px 8px;
  border-radius: 5px;
  border-left: 2px solid transparent;
  transition: background 0.12s;
}

.task-item:hover { background: rgba(255, 255, 255, 0.03); }

.task-item.is-doing { border-left-color: var(--accent-amber); }
.task-item.is-todo { border-left-color: rgba(0, 210, 255, 0.45); }
.task-item.is-done { border-left-color: transparent; opacity: 0.55; }

.task-tag {
  flex-shrink: 0;
  margin-top: 1px;
  width: 48px;
  padding: 3px 0;
  border-radius: 4px;
  border: 1px solid transparent;
  font-size: 9px;
  font-weight: 800;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  text-align: center;
  cursor: pointer;
  transition: filter 0.15s;
}

.task-tag:hover { filter: brightness(1.25); }

.tag-doing {
  color: var(--accent-amber);
  background: rgba(255, 140, 0, 0.12);
  border-color: rgba(255, 140, 0, 0.45);
}

.tag-todo {
  color: var(--accent-cyan);
  background: rgba(0, 210, 255, 0.12);
  border-color: rgba(0, 210, 255, 0.4);
}

.tag-done {
  color: var(--text-darker);
  background: rgba(255, 255, 255, 0.05);
  border-color: rgba(255, 255, 255, 0.12);
}

.task-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.task-title-input,
.task-detail-input {
  width: 100%;
  border: none;
  background: transparent;
  color: var(--text-light);
  font-size: 12px;
  padding: 2px 4px;
  border-radius: 4px;
  transition: background 0.12s;
}

.task-title-input:focus,
.task-detail-input:focus {
  outline: none;
  background: rgba(255, 255, 255, 0.06);
}

.task-detail-input {
  font-size: 11px;
  color: var(--text-muted);
}

.task-detail-input::placeholder { color: var(--text-darker); }

/* Detail stays hidden until needed — declutters the row */
.task-detail-input { display: none; }
.task-item:hover .task-detail-input,
.task-item:focus-within .task-detail-input,
.task-item.has-detail .task-detail-input { display: block; }

.is-done .task-title-input {
  text-decoration: line-through;
  color: var(--text-muted);
}

.task-time {
  flex-shrink: 0;
  margin-top: 4px;
  font-size: 9px;
  font-weight: 600;
  color: var(--text-darker);
  min-width: 20px;
  text-align: right;
}

.task-del {
  flex-shrink: 0;
  margin-top: 2px;
  width: 18px;
  height: 18px;
  border: none;
  background: transparent;
  color: var(--text-darker);
  cursor: pointer;
  border-radius: 4px;
  opacity: 0;
  transition: opacity 0.12s, color 0.12s, background 0.12s;
}

.task-item:hover .task-del { opacity: 1; }

.task-del:hover {
  color: var(--accent-red);
  background: rgba(239, 68, 68, 0.1);
}
</style>
