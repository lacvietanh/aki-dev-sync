<template>
  <BaseModal :show="showTasksModal" @close="closeTasksModal" container-class="tasks-modal" header-class="header-tasks">
    <template #title>
      <div class="tasks-modal-header-title" v-if="tasksProject">
        <img
          :src="projectIconSrc(tasksProject.id, iconTimestamp)"
          class="project-header-icon"
          alt=""
          @error="handleIconError"
          v-show="showIcon"
        />
        <i class="fa-solid fa-list-check mr-1" v-show="!showIcon"></i>
        <!-- Read-only reason lives as a SUFFIX on the title that is already here, plus its tooltip —
             no banner, no extra row (Extreme Narrow, CLAUDE.md). -->
        <span :title="notesEntry.error || undefined">
          Tasks: {{ tasksProject.name }}<template v-if="!writable"> — {{ notesEntry.status }}</template>
        </span>
      </div>
    </template>

    <div class="modal-body scrollable" v-if="tasksProject">
      <NotesField
        :model-value="collection.notes.value"
        @update:model-value="collection.setNotes"
        label="Project Notes"
        :placeholder="writable
          ? 'Write general project notes, credentials, or context here...'
          : 'Read-only — this project\'s .akidevsync/notes.json could not be read'"
        :maxlength="1500"
        :rows="2"
        :readonly="!writable"
        class="mb-3"
      />

      <TaskListPanel
        :tasks="collection.orderedTasks.value"
        :summary="collection.summary.value"
        :hide-completed="collection.hideCompleted.value"
        :disabled="!writable"
        @add="collection.addTask"
        @toggle="collection.toggleProp"
        @remove="collection.removeTask"
        @update:title="collection.updateTitle"
        @update:detail="collection.updateDetail"
        @update:hide-completed="(v) => (collection.hideCompleted.value = v)"
      />
    </div>

    <div class="modal-footer">
      <button class="btn-secondary" @click="closeTasksModal">Close</button>
    </div>
  </BaseModal>
</template>

<script setup>
import { computed, ref, watch } from 'vue'
import BaseModal from './BaseModal.vue'
import { iconTimestamp } from '../../store/projectStore'
import { projectIconSrc } from '../../utils/projectIcon'
import {
  showTasksModal, tasksProject, closeTasksModal,
  useProjectTaskCollection,
} from '../../composables/useProjectTasks'
import { isProjectNotesWritable } from '../../composables/useProjectNotes'
import { getProjectNotesEntry } from '../../store/projectNotesStore'
import { requestProjectNotesRefresh } from '../../store/remoteActions'
import NotesField from '../tasks/NotesField.vue'
import TaskListPanel from '../tasks/TaskListPanel.vue'

const showIcon = ref(true)

const collection = useProjectTaskCollection(tasksProject)

const notesEntry = computed(() => getProjectNotesEntry(tasksProject.value?.id))
const writable = computed(() => isProjectNotesWritable(tasksProject.value?.id))

function handleIconError() {
  showIcon.value = false
}

// Reset icon state when project changes
watch(tasksProject, () => {
  showIcon.value = true
})

// Re-read the file every time this modal opens for a project. Two things it catches that a
// boot-time hydrate cannot: a volume that has since been mounted (the entry flips from
// `unavailable` back to writable without a reload), and a `git pull` that changed notes.json under
// us. Routed through the action so a COMPANION's open also triggers the read — on the Mac, where
// the filesystem is (read_project_notes is not in the companion allowlist).
watch(
  [showTasksModal, tasksProject],
  ([open, project]) => {
    if (open && project?.id) requestProjectNotesRefresh(project.id)
  },
  { immediate: true }
)
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
</style>
