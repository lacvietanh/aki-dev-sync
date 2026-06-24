<template>
  <div class="projects-table-container">
    <table class="projects-table">
      <thead>
        <tr>
          <th>PROJECT / PATH</th>
          <th class="col-git-status" title="LOCAL GIT">
            <span class="th-with-ring">
              GIT
              <RefreshRing :interval-s="refreshSettings.git_interval_s" :refresh-key="gitRefreshKey" stroke-color="rgba(16, 185, 129, 0.6)" />
            </span>
          </th>
          <th class="col-last-sync">LAST ACTION</th>
          <th class="col-actions">
            <span class="th-with-ring">
              ACTIONS
              <RefreshRing :interval-s="refreshSettings.remote_diff_interval_s" :refresh-key="diffRefreshKey" stroke-color="rgba(255, 140, 0, 0.6)" />
            </span>
          </th>
        </tr>
      </thead>
      <tbody>
        <tr v-if="isReloading && projects.length === 0">
          <td colspan="4" style="padding: 20px;">
            <div style="display: flex; flex-direction: column; gap: 15px; width: 100%;">
              <div v-for="i in 3" :key="i" style="display: flex; gap: 15px; align-items: center;">
                <div class="skeleton-box" style="width: 28px; height: 28px; border-radius: 6px;"></div>
                <div style="flex: 1; display: flex; flex-direction: column; gap: 8px;">
                  <div class="skeleton-box" style="height: 12px; width: 30%;"></div>
                  <div class="skeleton-box" style="height: 10px; width: 60%;"></div>
                </div>
              </div>
            </div>
          </td>
        </tr>
        <tr v-else-if="projects.length === 0">
          <td colspan="4" class="empty-state"><i class="fa-solid fa-folder-open mb-2"></i><br>No projects found. Add one to get started.</td>
        </tr>
        <tr v-for="p in projects" :key="p.id" :class="{ 'row-syncing': projectRuntime[p.id]?.syncing }">
          <td class="col-project-info">
            <div style="display: flex; align-items: center; gap: 12px;">

              <!-- Project Icon -->
              <div style="width: 28px; height: 28px; display: flex; align-items: center; justify-content: center; background: rgba(0,0,0,0.2); border-radius: 6px; overflow: hidden; flex-shrink: 0;">
                  <img v-if="projectIcons[p.id]" :src="projectIcons[p.id]" style="width: 100%; height: 100%; object-fit: cover;" />
                  <i v-else class="fa-solid fa-folder-open text-cyan" style="font-size: 16px;"></i>
              </div>

              <div style="flex: 1; min-width: 0; padding-right: 16px;">
                <div class="project-name" style="display: flex; justify-content: space-between; align-items: center;">
                  <span style="white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">{{ p.name }}</span>
                  <a v-if="p.production_url" href="#" @click.prevent="openUrl(p.production_url)" title="Open Production Site" style="color: var(--accent-cyan); font-size: 11px; text-decoration: none; display: flex; align-items: center; gap: 4px;">
                    <i class="fa-solid fa-globe"></i><i class="fa-solid fa-arrow-up-right-from-square" style="font-size: 9px;"></i>
                  </a>
                </div>
                <div class="project-paths">
                  <span class="path-local" :title="p.local_path"><i class="fa-solid fa-laptop-code text-cyan mr-1"></i> {{ p.local_path }}</span>
                  <span v-if="p.remote_host" class="path-remote" :title="`${p.remote_host}:${p.remote_path}`"><i class="fa-solid fa-cloud text-amber mr-1"></i> {{ p.remote_host }}:{{ p.remote_path }}</span>
                </div>
              </div>
            </div>
          </td>
          <td class="col-git-status">
            <div class="git-cell">
              <button class="btn-action-git" @click="openGitModal(p)" title="Git Actions (Commit & Push to Remote Git)" aria-label="Git Actions">
                <i class="fa-brands fa-git-alt"></i>
              </button>
              <span class="git-badge" :class="'git-' + (projectRuntime[p.id]?.git_status || 'Unknown').replace(' ', '-')">
                {{ projectRuntime[p.id]?.git_status || '...' }}
              </span>
            </div>
          </td>
          <td class="col-last-sync">
            <div v-if="p.last_sync_action" class="last-sync-badge" :class="p.last_sync_action.includes('PULL') ? 'badge-pull' : 'badge-push'">
              {{ p.last_sync_action }} <span class="sync-time">{{ formatTimeAgo(p.last_sync_time) }}</span>
            </div>
            <div v-else class="text-muted">Never</div>
          </td>
          <td class="col-actions">
            <div class="actions-wrapper">
              
              <!-- Open Popup Trigger (OPEN Button) -->
              <div class="open-popup-wrapper" @mouseenter="onOpenEnter(p, $event)" @mouseleave="onOpenLeave()">
                <button class="btn-tech btn-tech-primary btn-action-open" title="Open Popup">
                  OPEN <i class="fa-solid fa-caret-down ml-1"></i>
                </button>

                <!-- Open Popup -->
                <transition name="popup-fade">
                  <div v-if="activeOpenPopup === p.id" class="open-popup" :style="popupPositionStyle" @mouseenter="onPopupEnter()" @mouseleave="onOpenLeave()">
                    <div class="popup-header" :title="p.name">
                      <i class="fa-solid fa-folder-open" style="color: var(--accent-cyan); margin-right: 6px;"></i>{{ p.name }}
                    </div>
                    <div style="display: flex;">
                      <!-- LOCAL -->
                    <div style="flex: 1; min-width: 150px;">
                      <div class="popup-section-label">💻 LOCAL</div>
                      <div class="popup-item" @click="openIdeLocal('finder', p.local_path)">
                        <i class="fa-solid fa-folder-open" style="width:14px; color: #fbbf24;"></i> Finder
                      </div>
                      <div class="popup-item" @click="openIdeLocal('terminal', p.local_path)">
                        <i class="fa-solid fa-terminal" style="width:14px;"></i> Terminal
                      </div>
                      <div class="popup-item" :class="{ 'popup-disabled': ideAvailability && !ideAvailability.vscode }" @click="openIdeLocal('vscode', p.local_path)">
                        <img src="/vscode-icon.png" class="popup-icon" alt="VSCode" /> VSCode
                      </div>
                      <div class="popup-item" :class="{ 'popup-disabled': ideAvailability && !ideAvailability.vscode_insiders }" @click="openIdeLocal('vscode_insiders', p.local_path)">
                        <img src="/vscode-icon.png" class="popup-icon popup-icon-insiders" alt="VSCode Insiders" /> VSCode Insiders
                      </div>
                      <div class="popup-item" :class="{ 'popup-disabled': ideAvailability && !ideAvailability.antigravity }" @click="openIdeLocal('antigravity', p.local_path)">
                        <img src="/antigravity-icon.png" class="popup-icon" alt="Antigravity" /> Antigravity IDE
                      </div>
                    </div>

                    <!-- REMOTE (only if project has remote config) -->
                    <div v-if="p.remote_host && p.remote_path" style="flex: 1; min-width: 180px; border-left: 1px solid rgba(255, 255, 255, 0.07); padding-left: 4px;">
                      <div class="popup-section-label">☁️ REMOTE (SSH)</div>
                      <div class="popup-item" @click="openIdeRemote('terminal', p.remote_host, p.remote_path)">
                        <i class="fa-solid fa-terminal" style="width:14px;"></i> SSH Terminal
                      </div>
                      <div class="popup-item" :class="{ 'popup-disabled': ideAvailability && !ideAvailability.vscode }" @click="openIdeRemote('vscode', p.remote_host, p.remote_path)">
                        <img src="/vscode-icon.png" class="popup-icon" alt="VSCode" /> VSCode (Remote SSH)
                      </div>
                      <div class="popup-item" :class="{ 'popup-disabled': ideAvailability && !ideAvailability.vscode_insiders }" @click="openIdeRemote('vscode_insiders', p.remote_host, p.remote_path)">
                        <img src="/vscode-icon.png" class="popup-icon popup-icon-insiders" alt="VSCode Insiders" /> VSCode Insiders (Remote)
                      </div>
                      <div class="popup-item" :class="{ 'popup-disabled': ideAvailability && !ideAvailability.antigravity }" @click="openIdeRemote('antigravity', p.remote_host, p.remote_path)">
                        <img src="/antigravity-icon.png" class="popup-icon" alt="Antigravity" /> Antigravity (Remote)
                      </div>
                    </div>
                  </div>
                </div>
                </transition>
              </div>

              <button class="btn-tech btn-tech-push-special" @click="openSpecialModal(p)" :disabled="projectRuntime[p.id]?.syncing" title="Select specific files to push">
                SELECT
              </button>

              <div class="dry-group" :class="p.dry_run ? 'is-safe' : 'is-danger'">
                <div class="dry-group-left">
                  <label class="btn-tech-git-inline" :class="{ 'active': p.sync_git }" title="Include .git in Push">
                    <input type="checkbox" v-model="p.sync_git" :disabled="projectRuntime[p.id]?.syncing" @change="saveProjectsList()" />
                    .git
                  </label>
                  <button class="btn-tech btn-tech-push" :class="{ 'btn-sync-clean': projectRuntime[p.id]?.hasPendingPush === false, 'btn-sync-checking': projectRuntime[p.id]?.hasPendingPush === null }" @click="startSync(p, 'push')" :disabled="projectRuntime[p.id]?.syncing" title="Push Local to Remote">
                    <i class="fa-solid fa-arrow-up"></i> PUSH
                  </button>
                </div>

                <div class="dry-toggle-center" title="Toggle Dry Run">
                  <span class="dry-label">DRY</span>
                  <label class="switch switch-sm">
                    <input type="checkbox" v-model="p.dry_run" :disabled="projectRuntime[p.id]?.syncing" @change="saveProjectsList()" />
                    <span class="slider"></span>
                  </label>
                </div>

                <div class="dry-group-right">
                  <button class="btn-tech btn-tech-pull" :class="{ 'btn-sync-clean': projectRuntime[p.id]?.hasPendingPull === false, 'btn-sync-checking': projectRuntime[p.id]?.hasPendingPull === null }" @click="startSync(p, 'pull')" :disabled="projectRuntime[p.id]?.syncing" title="Pull Remote to Local">
                    <i class="fa-solid fa-arrow-down"></i> PULL
                  </button>
                </div>
              </div>

              <button class="btn-tech btn-tech-secondary" :class="{ 'log-active': activeLogProjectId === p.id }" @click="toggleProjectLog(p.id)" title="View Project Log">
                LOG
              </button>

              <button class="btn-tech btn-tech-secondary btn-icon-only" @click="openConfig(p)" :disabled="projectRuntime[p.id]?.syncing" title="Edit Configuration" aria-label="Edit Configuration">
                <i class="fa-solid fa-gear"></i>
              </button>
            </div>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<script setup>
import { ref, watch, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useProjects } from '../composables/useProjects';
import { useLogs } from '../composables/useLogs';
import { gitRefreshKey, diffRefreshKey } from '../composables/useBackgroundRefresh';
import { refreshSettings } from '../store/refreshStore';
import RefreshRing from './RefreshRing.vue';

const { projects, projectRuntime, isReloading, startSync, saveProjectsList, openSpecialModal, openConfig, openGitModal } = useProjects();
const { activeLogProjectId, toggleProjectLog } = useLogs();

const projectIcons = ref({});
const activeOpenPopup = ref(null);
const ideAvailability = ref(null);
const popupPositionStyle = ref({ top: '34px', bottom: 'auto', transformOrigin: 'top left' });
let popupTimer = null;

watch(() => projects.value.map(p => p.id), async (newIds) => {
  for (const id of newIds) {
    if (projectIcons.value[id] === undefined) {
      projectIcons.value[id] = null;
      const project = projects.value.find(p => p.id === id);
      if (!project) continue;
      try {
        const base64 = await invoke("get_project_icon_base64", { localPath: project.local_path });
        if (base64) projectIcons.value[id] = base64;
      } catch (e) {
        console.error("Failed to load icon", e);
      }
    }
  }
}, { immediate: true });

async function onOpenEnter(project, event) {
  clearTimeout(popupTimer);
  activeOpenPopup.value = project.id;
  
  if (event) {
    const rect = event.currentTarget.getBoundingClientRect();
    const windowHeight = window.innerHeight;
    if (windowHeight - rect.bottom < 350) {
      popupPositionStyle.value = { top: 'auto', bottom: '34px', transformOrigin: 'bottom left' };
    } else {
      popupPositionStyle.value = { top: '34px', bottom: 'auto', transformOrigin: 'top left' };
    }
  }

  if (ideAvailability.value === null) {
    try {
      ideAvailability.value = await invoke('check_ide_availability');
    } catch {
      ideAvailability.value = { vscode: false, vscode_insiders: false, antigravity: false };
    }
  }
}

function onOpenLeave() {
  popupTimer = setTimeout(() => { activeOpenPopup.value = null; }, 150);
}

function onPopupEnter() {
  clearTimeout(popupTimer);
}

onUnmounted(() => clearTimeout(popupTimer));

const IDE_LOCAL_ARGS = {
  finder:          p => [p],
  terminal:        p => ['-a', 'Terminal', p],
  vscode:          p => ['-a', 'Visual Studio Code', p],
  vscode_insiders: p => ['-a', 'Visual Studio Code - Insiders', p],
  antigravity:     p => ['-a', 'Antigravity IDE', p],
}

async function openIdeLocal(ideName, path) {
  const args = IDE_LOCAL_ARGS[ideName]?.(path)
  if (args) try { await invoke('macos_open', { args }); } catch (e) { console.error(e); }
}

async function openIdeRemote(ideName, host, path) {
  try {
    let resolvedPath = path;
    if (path.startsWith('~/') || path === '~' || path.includes('$HOME')) {
      try {
        resolvedPath = await invoke('resolve_remote_path', { host, path });
      } catch (e) {
        console.error('Failed to resolve remote path', e);
      }
    }
    const remotePath = resolvedPath.startsWith('/') ? resolvedPath : `/${resolvedPath}`;
    if (ideName === 'vscode') {
      await invoke('macos_open', { args: [`vscode://vscode-remote/ssh-remote+${host}${remotePath}`] })
    } else if (ideName === 'vscode_insiders') {
      await invoke('macos_open', { args: [`vscode-insiders://vscode-remote/ssh-remote+${host}${remotePath}`] })
    } else {
      await invoke('open_remote_subprocess', { ideName, host, path: remotePath })
    }
  } catch (e) { console.error(e); }
}

async function openUrl(url) {
  try { await invoke('macos_open', { args: [url] }); } catch (e) { console.error(e); }
}

function formatTimeAgo(timestamp) {
  if (!timestamp) return "Never";
  const seconds = Math.floor(Date.now() / 1000) - timestamp;
  if (seconds < 60) return "Just now";
  if (seconds < 3600) return `${Math.floor(seconds/60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds/3600)}h ago`;
  return `${Math.floor(seconds/86400)}d ago`;
}
</script>

<style scoped>
.th-with-ring {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding-right: 8px;
}

.git-cell {
  display: flex;
  align-items: center;
  gap: 8px;
}

/* Open Popup */
.open-popup-wrapper {
  position: relative;
  display: inline-flex;
}

.btn-action-open {
  padding: 0 10px;
}

.open-popup {
  position: absolute;
  top: 30px;
  left: 0;
  z-index: 99;
  background: rgba(22, 30, 44, 0.97);
  border: 1px solid rgba(0, 210, 255, 0.2);
  border-radius: 8px;
  padding: 8px 0 6px 0;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.6);
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

.popup-header {
  font-size: 11px;
  font-weight: 700;
  color: #e5e7eb;
  padding: 0 12px 8px 12px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  margin-bottom: 6px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 320px;
}

.popup-section-label {
  font-size: 9px;
  text-transform: uppercase;
  letter-spacing: 0.1em;
  color: rgba(255, 255, 255, 0.35);
  padding: 4px 12px 2px;
  user-select: none;
}

.popup-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 12px;
  cursor: pointer;
  font-size: 12px;
  color: rgba(255, 255, 255, 0.8);
  transition: background 0.12s;
  user-select: none;
}

.popup-item:hover {
  background: rgba(0, 210, 255, 0.08);
  color: #fff;
}

.popup-item.popup-disabled {
  filter: grayscale(1) opacity(0.35);
  cursor: not-allowed;
  pointer-events: none;
}

.popup-icon {
  width: 14px;
  height: 14px;
  object-fit: contain;
  flex-shrink: 0;
}

.popup-icon-insiders {
  filter: hue-rotate(-50deg) saturate(2) brightness(1.2);
}

</style>
