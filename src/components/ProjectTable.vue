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
        <tr v-for="p in projects" :key="p.id" :class="{ 'row-syncing': syncingProjectId === p.id }">
          <td class="col-project-info">
            <div style="display: flex; align-items: center; gap: 12px;">
              <div style="flex-shrink: 0; width: 28px; height: 28px; display: flex; align-items: center; justify-content: center; background: rgba(0,0,0,0.2); border-radius: 6px; overflow: hidden;">
                <img v-if="projectIcons[p.id]" :src="projectIcons[p.id]" style="width: 100%; height: 100%; object-fit: cover;" />
                <i v-else class="fa-solid fa-folder-open text-cyan" style="font-size: 16px;"></i>
              </div>
              <div style="flex: 1; min-width: 0; padding-right: 16px;">
                <div class="project-name" style="display: flex; justify-content: space-between; align-items: center;">
                  <span style="white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">{{ p.name }}</span>
                  <div style="display: flex; gap: 10px; flex-shrink: 0;">
                    <a v-if="p.remote_url" @click.prevent="openUrl(p.remote_url)" class="help-icon" title="Open Remote Git Repo in Browser" style="cursor: pointer; font-size: 16px; opacity: 0.8; transition: opacity 0.2s;" onmouseover="this.style.opacity='1'" onmouseout="this.style.opacity='0.8'"><i class="fa-brands fa-git-alt"></i></a>
                    <a v-if="p.production_url" @click.prevent="openUrl(p.production_url)" class="help-icon text-cyan" title="Open Production Web in Browser" style="cursor: pointer; font-size: 16px; opacity: 0.8; transition: opacity 0.2s;" onmouseover="this.style.opacity='1'" onmouseout="this.style.opacity='0.8'"><i class="fa-solid fa-globe"></i></a>
                  </div>
                </div>
                <div class="project-paths">
                  <span class="path-local" @click="openInFinder(p.local_path)" style="cursor: pointer; text-decoration: underline; text-decoration-color: transparent; transition: text-decoration-color 0.2s;" onmouseover="this.style.textDecorationColor='inherit'" onmouseout="this.style.textDecorationColor='transparent'"><i class="fa-solid fa-laptop-code text-cyan mr-1"></i> {{ p.local_path }}</span>
                  <span class="path-remote" @click="openRemoteTerminal(p.remote_host, p.remote_path)" style="cursor: pointer; text-decoration: underline; text-decoration-color: transparent; transition: text-decoration-color 0.2s;" onmouseover="this.style.textDecorationColor='inherit'" onmouseout="this.style.textDecorationColor='transparent'" title="Open Terminal and SSH to Remote Path"><i class="fa-solid fa-cloud text-amber mr-1"></i> {{ p.remote_host }}:{{ p.remote_path }}</span>
                </div>
              </div>
            </div>
          </td>
          <td class="col-git-status">
            <div style="display: flex; align-items: center; gap: 6px;">
              <span class="git-badge" :class="'git-' + (p.git_status || 'Unknown').replace(' ', '-')">
                {{ p.git_status || '...' }}
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
              <button class="btn-action-terminal" @click="openInTerminal(p.local_path)" title="Open in Terminal">
                <i class="fa-solid fa-terminal"></i>
              </button>
              <button class="btn-action-vscode" @click="openInVscode(p.local_path)" title="Open in VS Code">
                <img src="/vscode-icon.png" alt="VS Code" class="action-vscode-icon" />
              </button>

              <button class="btn-tech btn-tech-push-special" @click="openSpecialModal(p)" :disabled="syncingProjectId !== null" title="Select specific files to push">
                PUSH SPECIAL
              </button>

              <div class="dry-group" :class="p.dry_run ? 'is-safe' : 'is-danger'">
                <div class="dry-group-left">
                  <label class="btn-tech-git-inline" :class="{ 'active': p.sync_git }" title="Include .git in Push">
                    <input type="checkbox" v-model="p.sync_git" :disabled="syncingProjectId !== null" />
                    .git
                  </label>
                  <button class="btn-tech btn-tech-push" @click="startSync(p, 'push')" :disabled="syncingProjectId !== null" title="Push Local to Remote">
                    <i class="fa-solid fa-arrow-up"></i> PUSH
                  </button>
                </div>
                
                <div class="dry-toggle-center" title="Toggle Dry Run">
                  <span class="dry-label">DRY</span>
                  <label class="switch switch-sm">
                    <input type="checkbox" v-model="p.dry_run" :disabled="syncingProjectId !== null" />
                    <span class="slider"></span>
                  </label>
                </div>

                <div class="dry-group-right">
                  <button class="btn-tech btn-tech-pull" @click="startSync(p, 'pull')" :disabled="syncingProjectId !== null" title="Lấy code về local">
                    <i class="fa-solid fa-arrow-down"></i> PULL
                  </button>
                </div>
              </div>

              <button class="btn-tech btn-tech-secondary" :class="{ 'log-active': activeLogProjectId === p.id }" @click="toggleProjectLog(p.id)" title="View Project Log">
                LOG
              </button>

              <button class="btn-tech btn-tech-secondary btn-icon-only" @click="openConfig(p)" :disabled="syncingProjectId !== null" title="Edit Configuration">
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
import { ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useProjects } from '../composables/useProjects';
import { useLogs } from '../composables/useLogs';

const { projects, syncingProjectId, isReloading, startSync, openSpecialModal, openConfig, openGitModal } = useProjects();
const { activeLogProjectId, toggleProjectLog } = useLogs();

const projectIcons = ref({});

watch(() => projects.value, async (newProjects) => {
  for (const p of newProjects) {
    if (projectIcons.value[p.id] === undefined) {
      projectIcons.value[p.id] = null;
      try {
        const base64 = await invoke("get_project_icon_base64", { localPath: p.local_path });
        if (base64) {
          projectIcons.value[p.id] = base64;
        }
      } catch (e) {
        console.error("Failed to load icon", e);
      }
    }
  }
}, { deep: true, immediate: true });

async function openInFinder(localPath) {
  try {
    await invoke("open_local_dir", { path: localPath });
  } catch (err) {
    console.error("Failed to open directory:", err);
  }
}

async function openUrl(url) {
  try {
    await invoke("open_url", { url });
  } catch (err) {
    console.error("Failed to open URL:", err);
  }
}

async function openInTerminal(localPath) {
  try {
    await invoke("open_in_terminal", { path: localPath });
  } catch (err) {
    console.error("Failed to open Terminal:", err);
  }
}

async function openRemoteTerminal(host, path) {
  try {
    await invoke("open_remote_terminal", { host, path });
  } catch (err) {
    console.error("Failed to open remote terminal:", err);
  }
}

async function openInVscode(localPath) {
  try {
    await invoke("open_in_vscode", { path: localPath });
  } catch (err) {
    console.error("Failed to open VS Code:", err);
  }
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
