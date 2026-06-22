<template>
  <div class="dashboard-top-header">
    <header class="top-header" data-tauri-drag-region @mousedown.prevent="startDragging">
      <div class="logo-section" data-tauri-drag-region>
        <h1 data-tauri-drag-region>
          <img src="/titlebar-icon.png" class="app-icon mr-1" data-tauri-drag-region /> Aki Remote Dev Sync
          <span class="app-version clickable" @click="showChangelog" title="Nhấn để xem Changelog">v{{ appVersion }} ({{ buildDate }})</span>
        </h1>
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
import { ref, onMounted } from 'vue';
import { getVersion } from '@tauri-apps/api/app';
import { useAppWindow } from '../composables/useAppWindow';
import { useProjects } from '../composables/useProjects';
import { useSsh } from '../composables/useSsh';
import changelogText from '../../CHANGELOG.md?raw';
import Swal from 'sweetalert2';
import { renderMarkdown, runMermaid } from '../utils/markdown';

const appVersion = ref('');
const buildDate = __BUILD_DATE__;

function showChangelog() {
  Swal.fire({
    title: 'Aki Remote Dev Sync Changelog',
    html: `<div class="markdown-body" style="text-align: left; font-size: 15px; color: #e2e8f0; background: #1a1a24; padding: 20px; border-radius: 8px; max-height: 500px; overflow-y: auto; line-height: 1.6;">${renderMarkdown(changelogText)}</div>`,
    background: '#131317',
    color: '#F3F4F6',
    width: '800px',
    confirmButtonColor: '#3b82f6',
    confirmButtonText: 'Đóng',
    showCloseButton: true,
    didOpen: () => {
      runMermaid();
    }
  });
}

onMounted(async () => {
  try {
    appVersion.value = await getVersion();
  } catch (e) {
    appVersion.value = 'dev';
  }
});

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

<style scoped>
.app-version {
  font-size: 0.5em;
  color: #64748b;
  margin-left: 8px;
  vertical-align: middle;
  font-weight: normal;
  letter-spacing: 1px;
}
.app-version.clickable {
  cursor: pointer;
  transition: color 0.2s;
}
.app-version.clickable:hover {
  color: #3b82f6;
}
</style>
