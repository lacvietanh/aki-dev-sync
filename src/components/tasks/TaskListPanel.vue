<template>
  <div class="task-list-panel">
    <div class="tasks-summary-bar mb-3" v-if="summary.total > 0">
      <div class="tasks-summary">
        <span v-if="summary.doing > 0" class="stat s-doing">{{ summary.doing }} pinned</span>
        <span v-if="summary.todo > 0" class="stat s-todo">{{ summary.todo }} active</span>
        <span v-if="summary.done > 0" class="stat s-done">{{ summary.done }} done</span>
      </div>
      <label class="toggle-hide-done">
        <input
          type="checkbox"
          :checked="hideCompleted"
          @change="$emit('update:hideCompleted', $event.target.checked)"
        />
        <span>Hide Completed</span>
      </label>
    </div>

    <div class="task-add-row mb-3">
      <input
        v-model="newTitle"
        @keyup.enter="submitNew"
        type="text"
        class="task-add-input"
        :placeholder="disabled ? 'Read-only — the notes file could not be read' : 'Add a new task...'"
        maxlength="200"
        :disabled="disabled"
      />
      <button class="btn-tech btn-tech-primary task-add-btn" :disabled="disabled || !newTitle.trim()" @click="submitNew" aria-label="Add task" title="Add task">
        <i class="fa-solid fa-plus"></i> Add
      </button>
    </div>

    <!-- List wrapper to cleanly separate empty state from transition group -->
    <div class="task-list-wrapper">
      <div v-if="tasks.length === 0" class="task-empty-state">
        <i class="fa-regular fa-circle-check fa-2x mb-2 text-muted"></i>
        <p>No tasks yet. Add what you are working on.</p>
      </div>

      <transition-group name="task-list" tag="div" class="task-list" v-else>
        <div
          v-for="task in tasks"
          :key="task.id"
          class="task-item-row"
          :class="[{ 'is-done': task.done, 'has-detail': !!task.detail }]"
        >
          <div class="task-states-left">
            <button
              class="task-state-icon-btn pin-btn"
              :class="{ 'is-active': task.pin }"
              @click="$emit('toggle', task, 'pin')"
              :disabled="disabled || task.done"
              title="Pin to top"
            >
              <i class="fa-solid fa-thumbtack"></i>
            </button>

            <button
              class="task-state-icon-btn wish-btn"
              :class="{ 'is-active': task.wish }"
              @click="$emit('toggle', task, 'wish')"
              :disabled="disabled || task.done"
              title="Mark as wish (do it later)"
            >
              <i class="fa-regular fa-clock"></i>
            </button>
          </div>

          <div class="task-info">
            <input
              v-model="task.title"
              @change="$emit('update:title', task, task.title)"
              @keyup.enter="$emit('toggle', task, 'done')"
              type="text"
              class="task-title-input"
              maxlength="200"
              :disabled="disabled || task.done"
            />
            <textarea
              :ref="(el) => setDetailEl(task.id, el)"
              v-model="task.detail"
              @change="$emit('update:detail', task, task.detail)"
              class="task-detail-textarea"
              placeholder="Add detail description..."
              maxlength="500"
              :disabled="disabled || task.done"
              rows="1"
            ></textarea>
          </div>

          <span class="task-time" :title="'Updated ' + timeAgo(task.updated_at) + ' ago'">{{ timeAgo(task.updated_at) }}</span>

          <button
            class="task-check-btn"
            :class="{ 'is-completed': task.done }"
            @click="$emit('toggle', task, 'done')"
            :disabled="disabled"
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

          <button class="task-del-btn" :disabled="disabled" @click="$emit('remove', task)" aria-label="Delete task" title="Delete task">
            <i class="fa-solid fa-trash-can"></i>
          </button>
        </div>
      </transition-group>
    </div>
  </div>
</template>

<script setup>
import { ref, nextTick, watch } from 'vue'
import { Toast } from '../../store/projectStore'
import { copyText } from '../../utils/clipboard'

const props = defineProps({
  tasks: { type: Array, default: () => [] },
  summary: { type: Object, default: () => ({ total: 0, open: 0, doing: 0, todo: 0, done: 0 }) },
  hideCompleted: { type: Boolean, default: false },
  // Read-only when notes file is unreadable; copy action stays enabled so user can still export text.
  disabled: { type: Boolean, default: false },
})

const emit = defineEmits(['add', 'toggle', 'remove', 'update:title', 'update:detail', 'update:hideCompleted'])

const newTitle = ref('')
const copiedTaskId = ref(null)

function submitNew() {
  // Backstop for the disabled input: `@keyup.enter` still fires on some IME/automation paths.
  if (props.disabled) return
  const text = newTitle.value.trim()
  if (!text) return
  emit('add', text)
  newTitle.value = ''
}

// utils/clipboard.js, not `navigator.clipboard` directly: the companion is a non-secure context where that API does not exist, so this button was silently dead on the phone.
async function copyTaskText(task) {
  const text = task.detail ? `${task.title}\n${task.detail}` : task.title
  if (!(await copyText(text))) {
    Toast.fire({ icon: 'error', title: 'Could not copy - select the task text and copy it by hand' })
    return
  }
  copiedTaskId.value = task.id
  setTimeout(() => {
    if (copiedTaskId.value === task.id) {
      copiedTaskId.value = null
    }
  }, 1500)
}

function timeAgo(ts) {
  if (!ts) return ''
  const s = Math.floor(Date.now() / 1000) - Math.floor(ts / 1000)
  if (s < 60) return 'now'
  if (s < 3600) return `${Math.floor(s / 60)}m`
  if (s < 86400) return `${Math.floor(s / 3600)}h`
  return `${Math.floor(s / 86400)}d`
}

// Scoped template refs prevent document query collisions across multiple TaskListPanel instances.
const detailEls = new Map()
function setDetailEl(taskId, el) {
  if (el) detailEls.set(taskId, el)
  else detailEls.delete(taskId)
}

// Focus and scroll to the newest task's detail field when a task is added.
watch(
  () => props.tasks.length,
  (newLen, oldLen) => {
    if (newLen <= (oldLen || 0)) return
    nextTick(() => {
      const newest = props.tasks.reduce(
        (a, b) => (!a || (b.created_at || 0) > (a.created_at || 0) ? b : a),
        null
      )
      if (!newest) return
      const el = detailEls.get(newest.id)
      if (el) {
        el.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
        el.focus()
      }
    })
  }
)
</script>

<style scoped>
.tasks-summary-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
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
  min-width: 0;
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
  transition: opacity 0.25s ease, transform 0.25s ease;
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
  transition: background 0.12s, opacity 0.12s, color 0.12s;
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

/* min-width:0 overrides flex item default to prevent ~200px input from overflowing at narrow widths */
.task-info {
  flex: 1;
  min-width: 0;
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

/* Narrow mode (700px): tighter gaps and padding preserve ~60px for title without hiding state controls. */
@media (max-width: 700px) {
  .task-item-row {
    gap: 5px;
    padding: 6px;
  }
}
</style>
