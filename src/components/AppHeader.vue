<template>
  <div class="dashboard-top-header">
    <header class="top-header" data-tauri-drag-region @mousedown.prevent="startDragging">
      <div class="logo-section" data-tauri-drag-region>
        <h1 data-tauri-drag-region>
          <img src="/titlebar-icon.png" class="app-icon mr-1" data-tauri-drag-region /> Aki Dev Sync
          <span v-if="isDev" class="dev-tag">DEV</span>
          <span class="app-version clickable" @click="showChangelog" title="Click to view Changelog">v{{ appVersion }} ({{ buildDate }} #{{ buildHash }})</span>
        </h1>
      </div>
      <div class="header-actions">
        <button class="btn-tech btn-tech-secondary btn-intro" @click="openIntroModal" title="Introduction">
          <i class="fa-solid fa-book-open"></i> INTRO
          <span class="badge-dot"></span>
        </button>
        <button class="btn-tech btn-tech-primary" @click="handleCreateNew" :disabled="anySyncing || isReloading">
          <i class="fa-solid fa-plus"></i> NEW PROJECT
        </button>
        <button class="btn-tech btn-tech-secondary" @click="openSshConfig" title="Edit SSH Config" :disabled="anySyncing || isReloading">
          <i class="fa-solid fa-server"></i> SSH CONFIG
        </button>
        <div class="btn-group-refresh">
          <button class="btn-tech btn-tech-secondary btn-refresh-main" @click="handleRefresh" title="Refresh all — git, remote diff, usage" :disabled="anySyncing || isReloading">
            <i class="fa-solid" :class="isReloading ? 'fa-rotate-right fa-spin' : 'fa-rotate-right'"></i>
            {{ isReloading ? 'REFRESHING...' : 'REFRESH' }}
          </button>
          <button class="btn-tech btn-tech-secondary btn-refresh-settings" @click="showRefreshSettings = true" title="Background Refresh Settings" :disabled="isReloading">
            <i class="fa-solid fa-sliders"></i>
          </button>
        </div>
        
        <RefreshSettingsModal :show="showRefreshSettings" @close="showRefreshSettings = false" />

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
import { ref } from 'vue';
import { useAppWindow } from '../composables/useAppWindow';
import { useProjects } from '../composables/useProjects';
import { useSsh } from '../composables/useSsh';
import { useIntro } from '../composables/useIntro';
import changelogText from '../../CHANGELOG.md?raw';
import Swal from 'sweetalert2';
import { renderMarkdown, runMermaid } from '../utils/markdown';
import RefreshSettingsModal from './modals/RefreshSettingsModal.vue';

const appVersion = __APP_VERSION__;
const buildDate = __BUILD_DATE__;
const buildHash = __BUILD_HASH__;
const showRefreshSettings = ref(false);
const isDev = import.meta.env.DEV;

function showChangelog() {
  Swal.fire({
    title: 'Aki Dev Sync Changelog',
    html: `<div class="markdown-body" style="text-align: left; font-size: 13px; color: #e2e8f0; background: #1a1a24; padding: 20px; border-radius: 8px; max-height: 500px; overflow-y: auto; line-height: 1.5;">${renderMarkdown(changelogText)}</div>`,
    background: '#131317',
    color: '#F3F4F6',
    width: '800px',
    confirmButtonColor: '#3b82f6',
    confirmButtonText: 'Close',
    showCloseButton: true,
    didOpen: () => {
      runMermaid();
    }
  });
}

const { startDragging, minimize, closeWin } = useAppWindow();
const { sshHosts, openSshConfig } = useSsh();
const { createNewProject, loadData, anySyncing, isReloading } = useProjects();
const { openIntroModal } = useIntro();

function handleRefresh() {
  loadData(sshHosts, true);
}

function handleCreateNew() {
  createNewProject(sshHosts);
}
</script>

<style scoped>
.btn-intro {
  position: relative;
  margin-right: 4px;
}
.badge-dot {
  position: absolute;
  top: -2px;
  right: -2px;
  width: 10px;
  height: 10px;
  background-color: #ef4444;
  border-radius: 50%;
  border: 2px solid #131317;
  animation: pulse-red 2s infinite;
}
@keyframes pulse-red {
  0% { transform: scale(0.95); box-shadow: 0 0 0 0 rgba(239, 68, 68, 0.7); }
  70% { transform: scale(1); box-shadow: 0 0 0 6px rgba(239, 68, 68, 0); }
  100% { transform: scale(0.95); box-shadow: 0 0 0 0 rgba(239, 68, 68, 0); }
}

.btn-group-refresh {
  display: flex;
  align-items: center;
  gap: 0;
}
.btn-refresh-main {
  border-top-right-radius: 0;
  border-bottom-right-radius: 0;
  border-right: none;
}
.btn-refresh-settings {
  border-top-left-radius: 0;
  border-bottom-left-radius: 0;
  padding: 0 10px;
}
.btn-refresh-main:hover:not(:disabled) + .btn-refresh-settings,
.btn-refresh-settings:hover:not(:disabled) {
  border-left-color: rgba(255, 255, 255, 0.4);
}

.app-version {
  font-size: 0.75em;
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

.dev-tag {
  background-color: rgba(239, 68, 68, 0.15);
  color: #f87171;
  border: 1px solid rgba(239, 68, 68, 0.4);
  font-size: 10px;
  font-weight: bold;
  padding: 1px 6px;
  border-radius: 4px;
  margin-left: 8px;
  vertical-align: middle;
  letter-spacing: 0.5px;
  display: inline-block;
}
</style>
