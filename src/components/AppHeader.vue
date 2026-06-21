<template>
  <div class="dashboard-top-header">
    <header class="top-header" data-tauri-drag-region @mousedown.prevent="startDragging">
      <div class="logo-section" data-tauri-drag-region>
        <h1 data-tauri-drag-region><img src="/titlebar-icon.png" class="app-icon mr-1" data-tauri-drag-region /> AkiDevSync</h1>
      </div>
      <div class="header-actions">
        <button class="btn-tech btn-tech-primary" @click="handleCreateNew" :disabled="syncingProjectId !== null || isReloading">
          <i class="fa-solid fa-plus"></i> NEW PROJECT
        </button>
        <button class="btn-tech btn-tech-secondary" @click="openSshConfig" title="Edit SSH Config" :disabled="syncingProjectId !== null || isReloading">
          <i class="fa-solid fa-server"></i> SSH CONFIG
        </button>
        <button class="btn-tech btn-tech-secondary" @click="handleReload" title="Reload Data" :disabled="syncingProjectId !== null || isReloading">
          <i class="fa-solid" :class="isReloading ? 'fa-rotate-right fa-spin' : 'fa-rotate-right'"></i> 
          {{ isReloading ? 'RELOADING...' : 'RELOAD' }}
        </button>
        
        <!-- Custom Traffic Lights -->
        <div class="titlebar-button minimize-btn" @click="minimize" title="Minimize">
          <i class="fa-solid fa-window-minimize"></i>
        </div>
        <div class="titlebar-button close-btn" @click="closeWin" title="Close">
          <i class="fa-solid fa-xmark fa-lg"></i>
        </div>
      </div>
    </header>
  </div>
</template>

<script setup>
import { useAppWindow } from '../composables/useAppWindow';
import { useProjects } from '../composables/useProjects';
import { useSsh } from '../composables/useSsh';

const { startDragging, minimize, closeWin } = useAppWindow();
const { sshHosts, openSshConfig } = useSsh();
const { createNewProject, loadData, syncingProjectId, isReloading } = useProjects();

function handleReload() {
  loadData(sshHosts, true);
}

function handleCreateNew() {
  createNewProject(sshHosts);
}
</script>
