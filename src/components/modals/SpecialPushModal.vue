<template>
  <BaseModal :show="showSpecialModal" @close="closeSpecialModal" container-class="special-modal" header-class="header-special">
    <template #title>
      <i class="fa-solid fa-list-check mr-1"></i> Push Special: {{ specialProject?.name }}
    </template>
    <div class="modal-body scrollable">
      <div v-if="specialLoading" class="loading-state">
        <i class="fa-solid fa-circle-notch fa-spin fa-2x text-cyan mb-2"></i>
        <p>Scanning local Git files...</p>
      </div>
      <div v-else-if="specialFiles.length === 0" class="empty-state">
        <i class="fa-solid fa-folder-open mb-2 text-muted"></i>
        <p>No tracked files found. Make sure this is a valid Git repository.</p>
      </div>
      <div v-else>
        <div class="special-actions mb-2">
          <span class="text-sm">{{ specialSelected.length }} / {{ specialFiles.length }} selected</span>
          <div>
            <button class="btn-secondary btn-sm mr-1" @click="selectAllSpecial(true)">Select All</button>
            <button class="btn-secondary btn-sm" @click="selectAllSpecial(false)">Deselect All</button>
          </div>
        </div>
        <div class="file-list">
          <label v-for="file in specialFiles" :key="file" class="file-item">
            <input type="checkbox" :checked="specialSelected.includes(file)" @change="toggleSpecialSelection(file)" />
            <span class="file-name"><i class="fa-regular fa-file-code mr-1 text-muted"></i> {{ file }}</span>
          </label>
        </div>
      </div>
    </div>
    <div class="modal-footer">
      <button class="btn-secondary" @click="closeSpecialModal">Cancel</button>
      <button class="btn-save btn-push" @click="confirmPushSpecial" :disabled="specialSelected.length === 0">
        <i class="fa-solid fa-arrow-up mr-1"></i> Push Selected Files
      </button>
    </div>
  </BaseModal>
</template>

<script setup>
import BaseModal from './BaseModal.vue'
import { useProjects } from '../../composables/useProjects'

const {
  showSpecialModal, specialProject, specialFiles, specialSelected, specialLoading,
  closeSpecialModal, selectAllSpecial, toggleSpecialSelection, confirmPushSpecial
} = useProjects()
</script>
