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
        <span>Tasks: {{ tasksProject.name }}</span>
      </div>
    </template>

    <div class="modal-body scrollable" v-if="tasksProject">
      <NotesField
        :model-value="collection.notes.value"
        @update:model-value="collection.setNotes"
        label="Project Notes"
        placeholder="Write general project notes, credentials, or context here..."
        :maxlength="1500"
        :rows="2"
        class="mb-3"
      />

      <TaskListPanel
        :tasks="collection.orderedTasks.value"
        :summary="collection.summary.value"
        :hide-completed="collection.hideCompleted.value"
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
import { ref, watch } from 'vue'
import BaseModal from './BaseModal.vue'
import { iconTimestamp } from '../../store/projectStore'
import { projectIconSrc } from '../../utils/projectIcon'
import {
  showTasksModal, tasksProject, closeTasksModal,
  useProjectTaskCollection,
} from '../../composables/useProjectTasks'
import NotesField from '../tasks/NotesField.vue'
import TaskListPanel from '../tasks/TaskListPanel.vue'

const showIcon = ref(true)

const collection = useProjectTaskCollection(tasksProject)

function handleIconError() {
  showIcon.value = false
}

// Reset icon state when project changes
watch(tasksProject, () => {
  showIcon.value = true
})
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
