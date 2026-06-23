<template>
  <div class="projects-table-container">
    <table class="projects-table">
      <thead>
        <tr>
          <th>PROJECT / PATH</th>
          <th class="col-git-status">LOCAL GIT</th>
          <th class="col-last-sync">LAST ACTION</th>
          <th class="col-actions">ACTIONS</th>
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

              <!-- Project Icon → Hub Trigger -->
              <div class="project-hub-wrapper" @mouseenter="onIconEnter(p)" @mouseleave="onIconLeave()">
                <div style="width: 28px; height: 28px; display: flex; align-items: center; justify-content: center; background: rgba(0,0,0,0.2); border-radius: 6px; overflow: hidden;">
                  <img v-if="projectIcons[p.id]" :src="projectIcons[p.id]" style="width: 100%; height: 100%; object-fit: cover;" />
                  <i v-else class="fa-solid fa-folder-open text-cyan" style="font-size: 16px;"></i>
                </div>

                <!-- Hub Popup -->
                <div v-if="activeHub === p.id" class="project-hub" @mouseenter="onHubEnter()" @mouseleave="onIconLeave()">
                  <!-- LOCAL -->
                  <div class="hub-section-label">💻 LOCAL</div>
                  <div class="hub-item" @click="openIdeLocal('finder', p.local_path)">
                    <i class="fa-solid fa-folder-open" style="width:14px; color: #fbbf24;"></i> Finder
                  </div>
                  <div class="hub-item" @click="openIdeLocal('terminal', p.local_path)">
                    <i class="fa-solid fa-terminal" style="width:14px;"></i> Terminal
                  </div>
                  <div class="hub-item" :class="{ 'hub-disabled': ideAvailability && !ideAvailability.vscode }" @click="openIdeLocal('vscode', p.local_path)">
                    <img src="/vscode-icon.png" class="hub-icon" alt="VSCode" />
                    VSCode
                  </div>
                  <div class="hub-item" :class="{ 'hub-disabled': ideAvailability && !ideAvailability.vscode_insiders }" @click="openIdeLocal('vscode_insiders', p.local_path)">
                    <img src="/vscode-icon.png" class="hub-icon hub-icon-insiders" alt="VSCode Insiders" />
                    VSCode Insiders
                  </div>
                  <div class="hub-item" :class="{ 'hub-disabled': ideAvailability && !ideAvailability.antigravity }" @click="openIdeLocal('antigravity', p.local_path)">
                    <img src="/antigravity-icon.png" class="hub-icon" alt="Antigravity" />
                    Antigravity IDE
                  </div>

                  <!-- REMOTE (only if project has remote config) -->
                  <template v-if="p.remote_host && p.remote_path">
                    <div class="hub-divider"></div>
                    <div class="hub-section-label">☁️ REMOTE (SSH)</div>
                    <div class="hub-item" @click="openIdeRemote('terminal', p.remote_host, p.remote_path)">
                      <i class="fa-solid fa-terminal" style="width:14px;"></i> SSH Terminal
                    </div>
                    <div class="hub-item" :class="{ 'hub-disabled': ideAvailability && !ideAvailability.vscode }" @click="openIdeRemote('vscode', p.remote_host, p.remote_path)">
                      <img src="/vscode-icon.png" class="hub-icon" alt="VSCode" />
                      VSCode (Remote SSH)
                    </div>
                    <div class="hub-item" :class="{ 'hub-disabled': ideAvailability && !ideAvailability.vscode_insiders }" @click="openIdeRemote('vscode_insiders', p.remote_host, p.remote_path)">
                      <img src="/vscode-icon.png" class="hub-icon hub-icon-insiders" alt="VSCode Insiders" />
                      VSCode Insiders (Remote)
                    </div>
                    <div class="hub-item" :class="{ 'hub-disabled': ideAvailability && !ideAvailability.antigravity }" @click="openIdeRemote('antigravity', p.remote_host, p.remote_path)">
                      <img src="/antigravity-icon.png" class="hub-icon" alt="Antigravity" />
                      Antigravity (Remote)
                    </div>
                  </template>

                  <!-- LINKS (only if production_url exists) -->
                  <template v-if="p.production_url">
                    <div class="hub-divider"></div>
                    <div class="hub-section-label">🌐 LINKS</div>
                    <div class="hub-item" @click="openUrl(p.production_url)">
                      <i class="fa-solid fa-globe text-cyan" style="width:14px;"></i> Open Production Site
                    </div>
                  </template>
                </div>
              </div>
              <!-- End Hub Wrapper -->

              <div style="flex: 1; min-width: 0; padding-right: 16px;">
                <div class="project-name">
                  <span style="white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">{{ p.name }}</span>
                </div>
                <div class="project-paths">
                  <span class="path-local"><i class="fa-solid fa-laptop-code text-cyan mr-1"></i> {{ p.local_path }}</span>
                  <span v-if="p.remote_host" class="path-remote" title="Remote path"><i class="fa-solid fa-cloud text-amber mr-1"></i> {{ p.remote_host }}:{{ p.remote_path }}</span>
                </div>
              </div>
            </div>
          </td>
          <td class="col-git-status">
            <div style="display: flex; align-items: center; gap: 6px;">
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
              <button class="btn-action-git" @click="openGitModal(p)" title="Git Actions (Commit & Push to Remote Git)">
                <i class="fa-brands fa-git-alt"></i>
              </button>

              <button class="btn-tech btn-tech-push-special" @click="openSpecialModal(p)" :disabled="projectRuntime[p.id]?.syncing" title="Select specific files to push">
                PUSH SPECIAL
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

              <button class="btn-tech btn-tech-secondary btn-icon-only" @click="openConfig(p)" :disabled="projectRuntime[p.id]?.syncing" title="Edit Configuration">
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

const { projects, projectRuntime, isReloading, startSync, saveProjectsList, openSpecialModal, openConfig, openGitModal } = useProjects();
const { activeLogProjectId, toggleProjectLog } = useLogs();

const projectIcons = ref({});
const activeHub = ref(null);
const ideAvailability = ref(null);
let hubTimer = null;

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

async function onIconEnter(project) {
  clearTimeout(hubTimer);
  activeHub.value = project.id;
  if (ideAvailability.value === null) {
    try {
      ideAvailability.value = await invoke('check_ide_availability');
    } catch {
      ideAvailability.value = { vscode: false, vscode_insiders: false, antigravity: false };
    }
  }
}

function onIconLeave() {
  hubTimer = setTimeout(() => { activeHub.value = null; }, 150);
}

function onHubEnter() {
  clearTimeout(hubTimer);
}

onUnmounted(() => clearTimeout(hubTimer));

const IDE_LOCAL_ARGS = {
  finder:          p => [p],
  terminal:        p => ['-a', 'Terminal', p],
  vscode:          p => ['-a', 'Visual Studio Code', p],
  vscode_insiders: p => ['-a', 'Visual Studio Code - Insiders', p],
  antigravity:     p => ['-a', 'Antigravity', p],
}

async function openIdeLocal(ideName, path) {
  const args = IDE_LOCAL_ARGS[ideName]?.(path)
  if (args) try { await invoke('macos_open', { args }); } catch (e) { console.error(e); }
}

async function openIdeRemote(ideName, host, path) {
  try {
    if (ideName === 'vscode') {
      await invoke('macos_open', { args: [`vscode://vscode-remote/ssh-remote+${host}${path}`] })
    } else if (ideName === 'vscode_insiders') {
      await invoke('macos_open', { args: [`vscode-insiders://vscode-remote/ssh-remote+${host}${path}`] })
    } else {
      await invoke('open_remote_subprocess', { ideName, host, path })
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
/* Project Hub */
.project-hub-wrapper {
  position: relative;
  flex-shrink: 0;
  width: 28px;
  height: 28px;
  cursor: pointer;
}

.project-hub {
  position: absolute;
  top: 34px;
  left: 0;
  z-index: 50;
  background: rgba(10, 15, 22, 0.97);
  border: 1px solid rgba(0, 210, 255, 0.2);
  border-radius: 8px;
  min-width: 210px;
  padding: 6px 0;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.6);
}

.hub-section-label {
  font-size: 9px;
  text-transform: uppercase;
  letter-spacing: 0.1em;
  color: rgba(255, 255, 255, 0.35);
  padding: 4px 12px 2px;
  user-select: none;
}

.hub-item {
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

.hub-item:hover {
  background: rgba(0, 210, 255, 0.08);
  color: #fff;
}

.hub-item.hub-disabled {
  filter: grayscale(1) opacity(0.35);
  cursor: not-allowed;
  pointer-events: none;
}

.hub-icon {
  width: 14px;
  height: 14px;
  object-fit: contain;
  flex-shrink: 0;
}

.hub-icon-insiders {
  filter: hue-rotate(100deg) saturate(1.3);
}

.hub-divider {
  height: 1px;
  background: rgba(255, 255, 255, 0.07);
  margin: 4px 0;
}
</style>
