<template>
  <div class="dashboard-top-header">
    <header class="top-header" data-tauri-drag-region @mousedown.prevent="startDragging">
      <div class="logo-section" data-tauri-drag-region>
        <h1 data-tauri-drag-region>
          <span class="app-icon-menu" @mousedown.stop title="Links">
            <img src="/titlebar-icon.png" class="app-icon icon-glow" />
            <span class="icon-chevron"><i class="fa-solid fa-chevron-down"></i></span>
            <div class="icon-dropdown">
              <a href="#" @click.prevent="openLink(REPO_URL)" class="icon-dropdown-item">
                <i class="fa-brands fa-github"></i> GitHub Repository
              </a>
              <a href="#" @click.prevent="openLink(RELEASE_URL)" class="icon-dropdown-item">
                <i class="fa-solid fa-download"></i> Latest Release
              </a>
            </div>
          </span>
          Aki Dev Sync
        </h1>
        <span v-if="isDev" class="dev-tag">DEV</span>
      </div>
      <span class="app-version clickable" @click="showChangelogModal = true" title="Click to view Changelog">
        <span class="version-num">v{{ appVersion }}</span>
        <span class="build-time">{{ buildDate }} {{ buildTime }}</span>
      </span>
      <div class="header-actions">
        <button class="btn-tech btn-tech-secondary btn-intro" @click="openIntroModal" title="Introduction">
          <i class="fa-solid fa-book-open"></i> <span class="btn-text">INTRO</span>
          <span class="badge-dot"></span>
        </button>
        <button class="btn-tech btn-tech-primary" @click="handleCreateNew" :disabled="anySyncing || isReloading">
          <i class="fa-solid fa-plus"></i> <span class="btn-text">PROJECT</span>
        </button>
        <button class="btn-tech btn-tech-secondary" @click="openSshConfig" title="Edit SSH Config" :disabled="anySyncing || isReloading">
          <i class="fa-solid fa-edit"></i> <span class="">SSH</span>
        </button>
        <div class="btn-group-refresh">
          <button class="btn-tech btn-tech-secondary btn-refresh-main" @click="handleRefresh" title="Refresh all — git, remote diff, usage" :disabled="anySyncing || isReloading">
            <i class="fa-solid" :class="isReloading ? 'fa-rotate-right fa-spin' : 'fa-rotate-right'"></i>
            <span class="btn-text">{{ isReloading ? 'REFRESHING...' : 'REFRESH' }}</span>
          </button>
          <button class="btn-tech btn-tech-secondary btn-refresh-settings" @click="showRefreshSettings = true" title="Background Refresh Settings" :disabled="isReloading">
            <i class="fa-solid fa-sliders"></i>
          </button>
        </div>

        <RefreshSettingsModal :show="showRefreshSettings" @close="showRefreshSettings = false" />
        <ChangelogModal :show="showChangelogModal" @close="showChangelogModal = false" />

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
import { invoke } from '@tauri-apps/api/core';
import { useAppWindow } from '../composables/useAppWindow';
import { useProjects } from '../composables/useProjects';
import { useSsh } from '../composables/useSsh';
import { useIntro } from '../composables/useIntro';
import RefreshSettingsModal from './modals/RefreshSettingsModal.vue';
import ChangelogModal from './modals/ChangelogModal.vue';

const REPO_URL = 'https://github.com/lacvietanh/aki-dev-sync';
const RELEASE_URL = 'https://github.com/lacvietanh/aki-dev-sync/releases/latest';

const appVersion = __APP_VERSION__;
const buildDate = __BUILD_DATE__;
const buildTime = __BUILD_TIME__;
const showRefreshSettings = ref(false);
const showChangelogModal = ref(false);
const isDev = import.meta.env.DEV;

const { startDragging, minimize, closeWin } = useAppWindow();
const { sshHosts, openSshConfig } = useSsh();
const { createNewProject, loadData, anySyncing, isReloading } = useProjects();
const { openIntroModal } = useIntro();

function openLink(url) {
  invoke('macos_open', { args: [url] }).catch(console.error);
}

function handleRefresh() {
  loadData(sshHosts, true);
}

function handleCreateNew() {
  createNewProject(sshHosts);
}
</script>

<style scoped>
.app-icon-menu {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 1px;
  cursor: pointer;
  padding: 2px 3px;
  border-radius: 5px;
  transition: background 0.15s;
  vertical-align: middle;
  margin-right: 4px;
}

.app-icon-menu:hover {
  background: rgba(255, 255, 255, 0.08);
}

.icon-chevron {
  font-size: 10px;
  color: #94a3b8;
  line-height: 1;
  margin-top: 1px;
  transition: color 0.15s, transform 0.2s;
}

.app-icon-menu:hover .icon-chevron {
  color: #cbd5e1;
  transform: rotate(180deg);
}

.icon-dropdown {
  display: none;
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  z-index: 1000;
  background: #1a1d23;
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 7px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(255, 255, 255, 0.04);
  min-width: 180px;
  padding: 4px;
  white-space: nowrap;
  /* reset h1 inherited styles */
  text-shadow: none;
  text-transform: none;
  letter-spacing: 0;
  font-weight: 400;
  font-size: 13px;
}

.icon-dropdown::before {
  content: '';
  position: absolute;
  top: -6px;
  left: 0;
  right: 0;
  height: 6px;
}

.app-icon-menu:hover .icon-dropdown {
  display: block;
}

.icon-dropdown-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 10px;
  font-size: 12px;
  color: #94a3b8;
  text-decoration: none;
  border-radius: 5px;
  transition: background 0.12s, color 0.12s;
}

.icon-dropdown-item:hover {
  background: rgba(255, 255, 255, 0.07);
  color: #e2e8f0;
}

.icon-dropdown-item i {
  width: 14px;
  text-align: center;
  color: #64748b;
}

.icon-dropdown-item:hover i {
  color: #94a3b8;
}

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
  0% {
    transform: scale(0.95);
    box-shadow: 0 0 0 0 rgba(239, 68, 68, 0.7);
  }

  70% {
    transform: scale(1);
    box-shadow: 0 0 0 6px rgba(239, 68, 68, 0);
  }

  100% {
    transform: scale(0.95);
    box-shadow: 0 0 0 0 rgba(239, 68, 68, 0);
  }
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

.btn-refresh-main:hover:not(:disabled)+.btn-refresh-settings,
.btn-refresh-settings:hover:not(:disabled) {
  border-left-color: rgba(255, 255, 255, 0.4);
}

.app-version {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 10px;
  color: #64748b;
  margin-left: 8px;
  margin-right: auto;
  vertical-align: middle;
  font-weight: normal;
  letter-spacing: 1px;
  white-space: nowrap;
  flex: 1;
  min-width: 0;
}

.version-num {
  flex-shrink: 0;
}

.build-time {
  flex-shrink: 1;
  overflow: hidden;
  text-overflow: ellipsis;
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

@media (max-width: 850px) {
  .header-actions .btn-tech {
    padding: 0 10px !important;
  }

  .header-actions .btn-tech .btn-text {
    display: none !important;
  }

  .header-actions .btn-tech i {
    display: inline-block !important;
  }
}
</style>
