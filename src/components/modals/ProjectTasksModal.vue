<template>
  <BaseModal :show="showTasksModal" @close="closeTasksModal" container-class="tasks-modal" header-class="header-tasks">
    <template #title>
      <div class="tasks-modal-header-title" v-if="tasksProject">
        <img
          :src="`aki-devsync-icon://${tasksProject.id}?t=${iconTimestamp}`"
          class="project-header-icon"
          alt=""
          @error="handleIconError"
          v-show="showIcon"
        />
        <i class="fa-solid fa-list-check mr-1" v-show="!showIcon"></i>
        <span>Tasks: {{ tasksProject.name }}</span>
      </div>
    </template>
    
    <div class="modal-body scrollable" v-if="tasksProject">
      <div class="tasks-summary-bar mb-3" v-if="summary.total > 0">
        <div class="tasks-summary">
          <span v-if="summary.doing > 0" class="stat s-doing">{{ summary.doing }} pinned</span>
          <span v-if="summary.todo > 0" class="stat s-todo">{{ summary.todo }} active</span>
          <span v-if="summary.done > 0" class="stat s-done">{{ summary.done }} done</span>
        </div>
        <label class="toggle-hide-done">
          <input type="checkbox" v-model="hideCompleted" />
          <span>Hide Completed</span>
        </label>
      </div>

      <!-- Project Notes -->
      <div class="project-notes-section mb-3">
        <div class="notes-header">
          <span class="notes-title"><i class="fa-regular fa-note-sticky mr-1"></i> Project Notes</span>
        </div>
        <textarea
          v-model="tasksProject.notes"
          @change="handleNotesChange"
          class="project-notes-textarea"
          placeholder="Write general project notes, credentials, or context here..."
          maxlength="1500"
          rows="2"
        ></textarea>
      </div>

      <!-- Add task -->
      <div class="task-add-row mb-3">
        <input
          v-model="newTitle"
          @keyup.enter="submitNew"
          type="text"
          class="task-add-input"
          placeholder="Add a new task..."
          maxlength="200"
        />
        <button class="btn-tech btn-tech-primary task-add-btn" :disabled="!newTitle.trim()" @click="submitNew" aria-label="Add task" title="Add task">
          <i class="fa-solid fa-plus"></i> Add
        </button>
      </div>

      <!-- List wrapper to cleanly separate empty state from transition group -->
      <div class="task-list-wrapper">
        <div v-if="orderedTasks.length === 0" class="task-empty-state">
          <i class="fa-regular fa-circle-check fa-2x mb-2 text-muted"></i>
          <p>No tasks yet. Add what you are working on.</p>
        </div>

        <transition-group name="task-list" tag="div" class="task-list" v-else>
          <div
            v-for="task in orderedTasks"
            :key="task.id"
            class="task-item-row"
            :class="[{ 'is-done': task.done, 'has-detail': !!task.detail }]"
            :data-task-id="task.id"
          >
            <!-- Left side controls: Pin, Wish -->
            <div class="task-states-left">
              <!-- Pin Toggle -->
              <button
                class="task-state-icon-btn pin-btn"
                :class="{ 'is-active': task.pin }"
                @click="toggleTaskProp(task, 'pin')"
                :disabled="task.done"
                title="Pin to top"
              >
                <i class="fa-solid fa-thumbtack"></i>
              </button>

              <!-- Wish Toggle -->
              <button
                class="task-state-icon-btn wish-btn"
                :class="{ 'is-active': task.wish }"
                @click="toggleTaskProp(task, 'wish')"
                :disabled="task.done"
                title="Mark as wish (do it later)"
              >
                <i class="fa-regular fa-clock"></i>
              </button>
            </div>

            <div class="task-info">
              <!-- Enter key marks task as Done -->
              <input
                v-model="task.title"
                @change="saveProjectsList"
                @keyup.enter="toggleTaskProp(task, 'done')"
                type="text"
                class="task-title-input"
                maxlength="200"
                :disabled="task.done"
              />
              <textarea
                v-model="task.detail"
                @change="handleDetailChange(task)"
                class="task-detail-textarea"
                placeholder="Add detail description..."
                maxlength="500"
                :disabled="task.done"
                rows="1"
              ></textarea>
            </div>

            <span class="task-time" :title="'Updated ' + timeAgo(task.updated_at) + ' ago'">{{ timeAgo(task.updated_at) }}</span>

            <!-- Mark Done Checklist Checkbox -->
            <button
              class="task-check-btn"
              :class="{ 'is-completed': task.done }"
              @click="toggleTaskProp(task, 'done')"
              aria-label="Toggle Done"
              :title="task.done ? 'Mark Active' : 'Mark Done'"
            >
              <i class="fa-solid fa-circle-check" v-if="task.done"></i>
              <i class="fa-regular fa-circle" v-else></i>
            </button>

            <button class="task-copy-btn" @click="copyTaskText(task)" aria-label="Copy task text" title="Copy title & detail">
              <i class="fa-solid fa-circle-check text-green" v-if="copiedTaskId === task.id"></i>
              <i class="fa-regular fa-copy" v-else></i>
            </button>

            <button class="task-del-btn" @click="removeTask(tasksProject, task)" aria-label="Delete task" title="Delete task">
              <i class="fa-solid fa-trash-can"></i>
            </button>
          </div>
        </transition-group>
      </div>
    </div>

    <div class="modal-footer">
      <button class="btn-secondary" @click="closeTasksModal">Close</button>
    </div>
  </BaseModal>
</template>

<script setup>
import { ref, computed, watch, nextTick } from 'vue'
import BaseModal from './BaseModal.vue'
import { iconTimestamp } from '../../store/projectStore'
import { saveProjectsList } from '../../composables/useProjectConfig'
import {
  showTasksModal, tasksProject, closeTasksModal,
  sortedTasks, openTaskCount, doingCount,
  addTask, toggleTaskProp, removeTask
} from '../../composables/useProjectTasks'

const newTitle = ref('')
const showIcon = ref(true)
const hideCompleted = ref(false)
const copiedTaskId = ref(null)

function handleIconError() {
  showIcon.value = false
}

function handleNotesChange() {
  if (tasksProject.value) {
    tasksProject.value.notes = (tasksProject.value.notes || '').trim()
    saveProjectsList()
  }
}

function handleDetailChange(task) {
  if (task) {
    task.detail = (task.detail || '').trim()
    saveProjectsList()
  }
}

async function copyTaskText(task) {
  const text = task.detail ? `${task.title}\n${task.detail}` : task.title
  try {
    await navigator.clipboard.writeText(text)
    copiedTaskId.value = task.id
    setTimeout(() => {
      if (copiedTaskId.value === task.id) {
        copiedTaskId.value = null
      }
    }, 1500)
  } catch (err) {
    console.error('Failed to copy text:', err)
  }
}

// Reset icon state when project changes
watch(tasksProject, () => {
  showIcon.value = true
})


const orderedTasks = computed(() => {
  if (!tasksProject.value) return []
  let list = sortedTasks(tasksProject.value)
  if (hideCompleted.value) {
    list = list.filter(t => !t.done)
  }
  return list
})


const summary = computed(() => {
  if (!tasksProject.value) return { total: 0, open: 0, doing: 0, todo: 0, done: 0 }
  const total = Array.isArray(tasksProject.value.tasks) ? tasksProject.value.tasks.length : 0
  const open = openTaskCount(tasksProject.value)
  const doing = doingCount(tasksProject.value)
  return { total, open, doing, todo: open - doing, done: total - open }
})

async function submitNew() {
  if (!tasksProject.value) return
  const task = addTask(tasksProject.value, newTitle.value)
  if (task) {
    newTitle.value = ''
    await nextTick()
    const el = document.querySelector(`[data-task-id="${task.id}"] .task-detail-textarea`)
    if (el) {
      el.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
      el.focus()
    }
  }
}

function timeAgo(ts) {
  if (!ts) return ''
  const s = Math.floor(Date.now() / 1000) - Math.floor(ts / 1000)
  if (s < 60) return 'now'
  if (s < 3600) return `${Math.floor(s / 60)}m`
  if (s < 86400) return `${Math.floor(s / 3600)}h`
  return `${Math.floor(s / 86400)}d`
}
</script>

<style scoped>
.tasks-modal-header-title {
  display: flex;
  align-items: center;
  gap: 8px;
}

.project-header-icon {
  width: 20px;
  height: 20px;
  border-radius: 4px;
  object-fit: contain;
}

.tasks-summary-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  border-bottom: 1px solid var(--border-color);
  padding-bottom: 10px;
}

.tasks-summary {
  display: flex;
  gap: 12px;
  font-size: 11px;
}

.toggle-hide-done {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--text-muted);
  cursor: pointer;
  user-select: none;
}

.toggle-hide-done input {
  cursor: pointer;
}

.stat {
  padding: 2px 6px;
  border-radius: 4px;
  font-weight: 700;
}
.stat.s-doing {
  background: rgba(255, 140, 0, 0.15);
  color: var(--accent-amber);
  border: 1px solid rgba(255, 140, 0, 0.3);
}
.stat.s-todo {
  background: rgba(0, 210, 255, 0.15);
  color: var(--accent-cyan);
  border: 1px solid rgba(0, 210, 255, 0.3);
}
.stat.s-done {
  background: rgba(255, 255, 255, 0.05);
  color: var(--text-muted);
  border: 1px solid rgba(255, 255, 255, 0.1);
}

.task-add-row {
  display: flex;
  gap: 8px;
}

.task-add-input {
  flex: 1;
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  color: var(--text-light);
  padding: 6px 10px;
  font-size: 13px;
  outline: none;
}

.task-add-input:focus {
  border-color: var(--accent-cyan);
  background: rgba(255, 255, 255, 0.06);
}

.task-add-btn {
  padding: 6px 12px;
  font-size: 13px;
}

.task-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-height: 400px;
  overflow-y: auto;
  position: relative;
}

.task-empty-state {
  text-align: center;
  padding: 20px 0;
  color: var(--text-muted);
  font-size: 12px;
}

/* Vue Transition Group Animation */
.task-list-move {
  transition: transform 0.3s cubic-bezier(0.25, 0.8, 0.25, 1);
}
.task-list-enter-active {
  transition: all 0.25s ease;
}
.task-list-leave-active {
  transition: none !important;
  display: none !important;
}
.task-list-enter-from {
  opacity: 0;
  transform: translateY(12px);
}
.task-list-leave-to {
  opacity: 0;
}

.task-item-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px;
  background: rgba(255, 255, 255, 0.02);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  transition: background 0.2s, opacity 0.2s, filter 0.2s, transform 0.3s cubic-bezier(0.25, 0.8, 0.25, 1);
}

.task-item-row:hover {
  background: rgba(255, 255, 255, 0.04);
}

/* Dim the completed rows */
.task-item-row.is-done {
  opacity: 0.45;
  filter: grayscale(0.6);
  background: rgba(0, 0, 0, 0.15);
}

.task-states-left {
  display: flex;
  gap: 4px;
  align-items: center;
  flex: none;
}

.task-state-icon-btn {
  background: transparent;
  border: none;
  width: 20px;
  height: 20px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  font-size: 10px;
  color: var(--text-darker);
  opacity: 0.35;
  transition: all 0.12s;
  border-radius: 4px;
  padding: 0;
  flex: none;
}

.task-state-icon-btn:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.08);
  opacity: 0.8;
  color: var(--text-light);
}

.task-state-icon-btn.pin-btn.is-active {
  color: var(--accent-amber);
  opacity: 1;
}

.task-state-icon-btn.wish-btn.is-active {
  color: #60a5fa;
  opacity: 1;
}

.task-state-icon-btn:disabled {
  cursor: not-allowed;
  opacity: 0.1 !important;
  color: var(--text-darker) !important;
}

.task-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.task-title-input {
  background: transparent;
  border: none;
  color: var(--text-light);
  font-size: 13px;
  font-weight: 600;
  outline: none;
  padding: 0;
  border-bottom: 1px solid transparent;
}

.task-title-input:focus:not(:disabled) {
  border-bottom-color: rgba(255, 255, 255, 0.15);
}

/* Strike-through when completed */
.is-done .task-title-input {
  text-decoration: line-through;
  color: var(--text-darker);
}

.task-detail-textarea {
  background: transparent;
  border: none;
  color: var(--text-muted);
  font-size: 11px;
  outline: none;
  padding: 0;
  border-bottom: 1px solid transparent;
  resize: none;
  overflow-y: hidden;
  font-family: inherit;
  width: 100%;
  line-height: 1.4;
  field-sizing: content;
}

.task-detail-textarea:focus:not(:disabled) {
  border-bottom-color: rgba(255, 255, 255, 0.1);
  color: var(--text-light);
}

.is-done .task-detail-textarea {
  color: var(--text-darker);
}

.task-time {
  font-size: 10px;
  color: var(--text-darker);
  white-space: nowrap;
}

.task-check-btn {
  background: transparent;
  border: none;
  color: var(--text-darker);
  font-size: 14px;
  cursor: pointer;
  padding: 2px;
  border-radius: 4px;
  transition: color 0.12s, transform 0.12s;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
}

.task-check-btn:hover {
  color: var(--accent-cyan);
  transform: scale(1.1);
}

.task-check-btn.is-completed {
  color: var(--accent-green);
}

.task-check-btn.is-completed:hover {
  color: var(--accent-red);
}

.task-copy-btn {
  background: transparent;
  border: none;
  color: var(--text-darker);
  font-size: 12px;
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  transition: color 0.12s, background 0.12s;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.task-copy-btn:hover {
  color: var(--accent-cyan);
  background: rgba(0, 210, 255, 0.1);
}

.task-del-btn {
  background: transparent;
  border: none;
  color: var(--text-darker);
  font-size: 12px;
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  transition: color 0.12s, background 0.12s;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.task-del-btn:hover {
  color: var(--accent-red);
  background: rgba(239, 68, 68, 0.1);
}

.project-notes-section {
  background: rgba(255, 255, 255, 0.015);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 8px 12px;
}

.notes-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 6px;
}

.notes-title {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.project-notes-textarea {
  background: transparent;
  border: none;
  color: var(--text-light);
  font-size: 12px;
  outline: none;
  padding: 0;
  resize: none;
  overflow-y: hidden;
  font-family: inherit;
  width: 100%;
  line-height: 1.5;
  border-bottom: 1px solid transparent;
  field-sizing: content;
}

.project-notes-textarea:focus {
  border-bottom-color: rgba(255, 255, 255, 0.1);
}
</style>
