<template>
  <div class="projects-table-container">
    <div class="projects-grid" :class="{ 'dragging-active': dragFromIndex !== null }">
      <!-- Header -->
      <div class="grid-header">
        <div class="grid-header-cell col-project-info col-project-info-header">
          <span><span class="u-narrow-hide">PROJECTS</span><span class="u-wide-hide">PJ</span> ({{ projects.length }})</span>
          <button
                  class="btn-tech btn-tech-primary btn-new-project-inline"
                  @click="handleCreateNew"
                  :disabled="anySyncing || isReloading"
                  title="New Project"
                  aria-label="New Project">
            <i class="fa-solid fa-plus"></i><span class="u-narrow-hide"> NEW</span>
          </button>
        </div>
        <div class="grid-header-cell col-tasks" title="PROJECT TASKS">TASKS</div>
        <div class="grid-header-cell col-git-status" title="LOCAL GIT">
          <span class="th-with-ring">
            <span class="u-narrow-hide">GIT</span>
            <RefreshRing :interval-s="refreshSettings.git_interval_s" :refresh-key="gitRefreshKey" stroke-color="rgba(16, 185, 129, 0.6)" />
          </span>
        </div>
        <div class="grid-header-cell col-last-sync" title="LAST ACTION">LAST</div>
        <div class="grid-header-cell col-action" title="OPEN / SELECT-PUSH">ACTION</div>
        <div class="grid-header-cell col-sync">
          <span class="th-with-ring">
            SYNC
            <RefreshRing :interval-s="remoteModeEnabled ? refreshSettings.remote_diff_interval_s : 0" :refresh-key="diffRefreshKey" stroke-color="rgba(255, 140, 0, 0.6)" />
          </span>
        </div>
      </div>

      <transition-group tag="div" class="grid-body" name="project-list">
        <!-- Loading State -->
        <div v-if="isReloading && projects.length === 0" class="grid-row-special" key="loading">
          <div style="padding: 20px 12px; width: 100%;">
            <div style="display: flex; flex-direction: column; gap: 15px; width: 100%;">
              <div v-for="i in 3" :key="i" style="display: flex; gap: 15px; align-items: center;">
                <div class="skeleton-box" style="width: 28px; height: 28px; border-radius: 6px;"></div>
                <div style="flex: 1; display: flex; flex-direction: column; gap: 8px;">
                  <div class="skeleton-box" style="height: 12px; width: 30%;"></div>
                  <div class="skeleton-box" style="height: 10px; width: 60%;"></div>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- Empty State -->
        <div v-else-if="projects.length === 0" class="grid-row-special empty-state" key="empty">
          <div style="padding: 20px; text-align: center; width: 100%;">
            <i class="fa-solid fa-folder-open mb-2"></i><br>No projects found. Add one to get started.
          </div>
        </div>

        <!-- Project Rows -->
        <div
             v-for="(p, index) in projects"
             :key="p.id"
             class="grid-row"
             :class="{ 'row-syncing': projectRuntime[p.id]?.syncing, 'row-dragging': dragFromIndex === index }"
             draggable="true"
             @dragstart="onRowDragStart(index, $event)"
             @dragover.prevent="onRowDragOver(index, $event)"
             @dragenter.prevent
             @drop.prevent="onRowDrop(index)"
             @dragend="onRowDragEnd"
             @mousedown="onRowMouseDown">
          <!-- Cell 1: Project Info -->
          <div class="grid-row-cell col-project-info">
            <div class="project-info-row">
              <!-- Project Icon (drag handle) -->
              <div
                   class="project-drag-handle icon-glow"
                   title="Kéo để sắp xếp"
                   @mousedown="isHandleMouseDown = true">
                <img v-if="!failedIcons[p.id]" :src="`aki-devsync-icon://${p.id}?t=${iconTimestamp}`" style="width: 100%; height: 100%; object-fit: cover;" draggable="false" @error="failedIcons[p.id] = true" />
                <i v-else class="fa-solid fa-folder-open text-cyan" style="font-size: 16px;"></i>
              </div>

              <div class="project-text-col">
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
          </div>

          <!-- Cell 2: Tasks -->
          <div class="grid-row-cell col-tasks">
            <TaskCell :project="p" />
          </div>

          <!-- Cell 3: Git Status -->
          <div class="grid-row-cell col-git-status">
            <div class="git-cell">
              <CountBadgeWrap :count="projectRuntime[p.id]?.git_changed_count || 0">
                <button
                        class="btn-action-git"
                        :class="{
                          'git-no-repo': ['No Git', 'Git Error'].includes(projectRuntime[p.id]?.git_status),
                          'git-ahead': projectRuntime[p.id]?.git_status === 'Ahead',
                        }"
                        @click="openGitModal(p)"
                        :title="projectRuntime[p.id]?.git_status === 'No Git' ? 'No Git repository' : projectRuntime[p.id]?.git_status === 'Git Error' ? 'Git error — click to view' : projectRuntime[p.id]?.git_changed_count > 0 ? `Git Actions (${projectRuntime[p.id].git_changed_count} changed file(s))` : projectRuntime[p.id]?.git_status === 'Ahead' ? 'Ahead of remote — click to push' : 'Git Actions (Commit & Push to Remote Git)'"
                        aria-label="Git Actions">
                  <i class="fa-brands fa-git-alt"></i>
                </button>
              </CountBadgeWrap>
            </div>
          </div>

          <!-- Cell 4: Last Sync -->
          <div class="grid-row-cell col-last-sync">
            <div v-if="p.last_sync_action" class="last-sync-badge" :class="p.last_sync_action.includes('PULL') ? 'badge-pull' : 'badge-push'">
              {{ p.last_sync_action }} <span class="sync-time">{{ formatTimeAgo(p.last_sync_time) }}</span>
            </div>
            <div v-else class="text-muted">Never</div>
            <div v-if="p.last_sync_action && p.last_sync_host" class="sync-host" :title="`Last action host: ${p.last_sync_host}`">{{ p.last_sync_host }}</div>
          </div>

          <!-- Cell 5: Action (OPEN + SELECT-push only) -->
          <div class="grid-row-cell col-action">
            <div class="actions-wrapper">
              <!-- Open Popup Trigger (OPEN Button) -->
              <div class="open-popup-wrapper" @mouseenter="onOpenEnter(p, $event)">
                <button class="btn-tech btn-tech-primary btn-action-open" title="Open Popup">
                  <span class="btn-text u-narrow-hide">OPEN</span> <i class="fa-solid fa-caret-up"></i>
                </button>

                <!-- Open Popup (Native CSS Hover with fixed positioning) -->
                <div class="open-popup" :style="projectRuntime[p.id]?.popupStyle">
                  <div class="popup-header" :title="p.name" style="display: flex; align-items: center;">
                    <img v-if="!failedIcons[p.id]" :src="`aki-devsync-icon://${p.id}?t=${iconTimestamp}`" class="popup-project-icon" alt="" @error="failedIcons[p.id] = true" />
                    <i v-else class="fa-solid fa-folder-open text-cyan mr-1" style="font-size: 18px;"></i>
                    <span style="flex: 1; overflow: hidden; text-overflow: ellipsis;">{{ p.name }}</span>
                    <button class="popup-copy-btn" @click.stop="openReportHtml(p)" title="Open REPORT.html (pulls newer copy from remote first if needed)">
                      <i class="fa-solid fa-file-lines"></i> REPORT
                    </button>
                  </div>
                  <div style="display: flex;">
                    <!-- LOCAL -->
                    <div style="flex: 1; min-width: 150px;">
                      <div class="popup-section-label">
                        <span>💻 LOCAL</span>
                        <button class="popup-copy-btn" @click.stop="copyLocalPath(p)" :title="copiedPathKey === `local-${p.id}` ? 'Copied!' : 'Copy full path'">
                          <i class="fa-solid" :class="copiedPathKey === `local-${p.id}` ? 'fa-check' : 'fa-copy'"></i> COPY
                        </button>
                      </div>
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
                      <div v-if="getDevCmd(p) || getBuildCmd(p)" class="popup-run-row">
                        <div v-if="getDevCmd(p)" class="popup-item popup-run-btn" @click="runProjectDev(p.local_path, getDevCmd(p))" :title="getDevCmd(p)">
                          <i class="fa-solid fa-terminal" style="width:14px; color: var(--accent-green, #10b981);"></i> DEV
                        </div>
                        <div v-if="getBuildCmd(p)" class="popup-item popup-run-btn" @click="runProjectCommand(p.local_path, getBuildCmd(p))" :title="getBuildCmd(p)">
                          <i class="fa-solid fa-hammer" style="width:14px; color: #f59e0b;"></i> BUILD
                        </div>
                      </div>
                    </div>

                    <!-- REMOTE -->
                    <div v-if="p.remote_host && p.remote_path && remoteModeEnabled" style="flex: 1; min-width: 180px; border-left: 1px solid rgba(255, 255, 255, 0.07); padding-left: 4px;">
                      <div class="popup-section-label">
                        <span>☁️ REMOTE (SSH)</span>
                        <button class="popup-copy-btn" @click.stop="copyRemotePath(p)" :title="copiedPathKey === `remote-${p.id}` ? 'Copied!' : 'Copy full path'">
                          <i class="fa-solid" :class="copiedPathKey === `remote-${p.id}` ? 'fa-check' : 'fa-copy'"></i> COPY
                        </button>
                      </div>
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
              </div>

              <button class="btn-tech btn-tech-push-special" @click="openSelectDialog(p)" :disabled="projectRuntime[p.id]?.syncing || !remoteModeEnabled" :title="!remoteModeEnabled ? 'Remote Mode is off' : 'Pick specific files/folders (native file picker) and push only those to Remote — bypasses this project\'s exclude list, unaffected by the DRY toggle'">
                <i class="fa-solid fa-upload"></i>
              </button>
            </div>
          </div>

          <!-- Cell 6: Sync (PUSH/DRY/PULL, LOG, config) -->
          <div class="grid-row-cell col-sync">
            <div class="actions-wrapper">
              <fieldset :disabled="projectRuntime[p.id]?.syncing || !remoteModeEnabled" class="remote-actions-fieldset" :title="!remoteModeEnabled ? 'Remote Mode is off' : ''">
                <div class="dry-group" :class="[p.dry_run ? 'is-safe' : 'is-danger', projectRuntime[p.id]?.hasPendingPush && projectRuntime[p.id]?.hasPendingPull ? 'is-diverged' : '']">
                  <div class="dry-group-left">
                    <CountBadgeWrap :count="projectRuntime[p.id]?.pushCount || 0">
                      <button
                              class="btn-tech btn-tech-push"
                              :class="{
                                'btn-sync-clean': projectRuntime[p.id]?.hasPendingPush === false,
                                'btn-sync-checking': projectRuntime[p.id]?.hasPendingPush === null,
                                'btn-sync-diverged': projectRuntime[p.id]?.hasPendingPush && projectRuntime[p.id]?.hasPendingPull
                              }"
                              @click="startSync(p, 'push')"
                              :title="!remoteModeEnabled ? 'Remote Mode is off' : projectRuntime[p.id]?.pushCount > 0 ? `Push Local → Remote (${projectRuntime[p.id].pushCount} file(s))` : 'Push Local to Remote'">
                        <i class="fa-solid fa-cloud-arrow-up"></i> <span class="btn-text u-narrow-hide">PUSH</span>
                      </button>
                    </CountBadgeWrap>
                  </div>

                  <div class="dry-toggle-center" title="Toggle Dry Run">
                    <span class="dry-label">DRY</span>
                    <label class="switch switch-sm">
                      <input type="checkbox" v-model="p.dry_run" @change="saveProjectsList()" />
                      <span class="slider"></span>
                    </label>
                  </div>

                  <div class="dry-group-right">
                    <CountBadgeWrap :count="projectRuntime[p.id]?.pullCount || 0">
                      <button
                              class="btn-tech btn-tech-pull"
                              :class="{
                                'btn-sync-clean': projectRuntime[p.id]?.hasPendingPull === false,
                                'btn-sync-checking': projectRuntime[p.id]?.hasPendingPull === null,
                                'btn-sync-diverged': projectRuntime[p.id]?.hasPendingPush && projectRuntime[p.id]?.hasPendingPull
                              }"
                              @click="startSync(p, 'pull')"
                              :title="!remoteModeEnabled ? 'Remote Mode is off' : projectRuntime[p.id]?.pullCount > 0 ? `Pull Remote → Local (${projectRuntime[p.id].pullCount} file(s))` : 'Pull Remote to Local'">
                        <i class="fa-solid fa-cloud-arrow-down"></i> <span class="btn-text u-narrow-hide">PULL</span>
                      </button>
                    </CountBadgeWrap>
                  </div>
                </div>
              </fieldset>

              <button class="btn-tech btn-tech-secondary" :class="{ 'log-active': activeLogProjectId === p.id }" @click="toggleProjectLog(p.id)" title="View Project Log">
                <i class="fa-solid fa-file-lines btn-log-icon-only"></i>
                <span class="btn-text u-narrow-hide">LOG</span>
              </button>

              <button class="btn-tech btn-tech-secondary btn-icon-only" @click="openConfig(p)" :disabled="projectRuntime[p.id]?.syncing" title="Edit Configuration" aria-label="Edit Configuration">
                <i class="fa-solid fa-gear"></i>
              </button>
            </div>
          </div>
        </div>
      </transition-group>
    </div>
  </div>
</template>

<script setup>
import { ref, watch, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useProjects } from '../composables/useProjects';
import { useLogs } from '../composables/useLogs';
import { useSsh } from '../composables/useSsh';
import { gitRefreshKey, diffRefreshKey } from '../composables/useBackgroundRefresh';
import { refreshSettings } from '../store/refreshStore';
import { Toast, ideAvailability, iconTimestamp } from '../store/projectStore';
import { remoteModeEnabled } from '../store/remoteModeStore';
import RefreshRing from './RefreshRing.vue';
import TaskCell from './TaskCell.vue';
import CountBadgeWrap from './CountBadgeWrap.vue';

const { projects, projectRuntime, anySyncing, isReloading, startSync, saveProjectsList, openSelectDialog, openConfig, openGitModal, createNewProject } = useProjects();
const { activeLogProjectId, toggleProjectLog } = useLogs();
const { sshHosts } = useSsh();

function handleCreateNew() {
  createNewProject(sshHosts);
}

const failedIcons = ref({});
watch([projects, iconTimestamp], () => {
  failedIcons.value = {};
});

// Popup is `position: fixed`, so viewport coordinates are the right frame of reference. It is
// horizontally centered on the window (clamped to a small viewport margin so it never crops
// against an edge) rather than pinned to the trigger's left edge, which used to let a wide popup
// run off the right side of the window. The popup element is already in the DOM at
// `visibility: hidden` (not `display: none`) when this fires, so its real rendered width can be
// measured before it becomes visible.
function onOpenEnter(project, event) {
  if (event) {
    const rect = event.currentTarget.getBoundingClientRect();
    if (!projectRuntime.value[project.id]) {
      projectRuntime.value[project.id] = {};
    }
    const popupEl = event.currentTarget.querySelector('.open-popup');
    const margin = 8;
    let left = rect.left;
    if (popupEl) {
      const popupWidth = popupEl.getBoundingClientRect().width || popupEl.offsetWidth || 0;
      left = window.innerWidth / 2 - popupWidth / 2;
      left = Math.min(Math.max(left, margin), window.innerWidth - popupWidth - margin);
    }
    projectRuntime.value[project.id].popupStyle = {
      position: 'fixed',
      bottom: `${window.innerHeight - rect.top}px`,
      left: `${left}px`,
      transformOrigin: 'bottom center'
    };
  }
}

// --- Drag to reorder ---
const dragFromIndex = ref(null);
const isHandleMouseDown = ref(false);

function onRowDragStart(index, event) {
  if (!isHandleMouseDown.value) {
    event.preventDefault();
    return;
  }
  // Reset trạng thái mousedown ngay lập tức sau khi xác thực dragstart
  isHandleMouseDown.value = false;
  dragFromIndex.value = index;
  // Bắt buộc phải set data đối với WebKit/macOS để drop được kích hoạt
  event.dataTransfer.effectAllowed = 'move';
  event.dataTransfer.setData('text/plain', String(index));
}

function onRowMouseDown(event) {
  // Nếu mousedown không nằm trên drag handle, reset trạng thái
  if (!event.target.closest('.project-drag-handle')) {
    isHandleMouseDown.value = false;
  }
}

function onRowDragOver(index, event) {
  if (dragFromIndex.value === null || dragFromIndex.value === index) return;

  // Tính toán toạ độ để xác định chuột đã vượt qua trung điểm của hàng đích chưa.
  // Điều này ngăn chặn triệt để hiện tượng nhảy hàng liên tục (feedback loop/jittering) khi vừa chạm biên.
  const rect = event.currentTarget.getBoundingClientRect();
  const threshold = rect.top + rect.height / 2;
  const fromIndex = dragFromIndex.value;

  // Kéo xuống: chỉ swap khi chuột đi qua nửa dưới của hàng đích
  if (fromIndex < index && event.clientY < threshold) return;

  // Kéo lên: chỉ swap khi chuột đi qua nửa trên của hàng đích
  if (fromIndex > index && event.clientY > threshold) return;

  const arr = [...projects.value];
  const [movedItem] = arr.splice(fromIndex, 1);
  arr.splice(index, 0, movedItem);

  projects.value = arr;
  dragFromIndex.value = index;
}

function onRowDrop(index) {
  onRowDragEnd();
}

function onRowDragEnd() {
  dragFromIndex.value = null;
  isHandleMouseDown.value = false;
  saveProjectsList();
}

const IDE_LOCAL_ARGS = {
  finder: p => [p],
  vscode: p => ['-a', 'Visual Studio Code', p],
  vscode_insiders: p => ['-a', 'Visual Studio Code - Insiders', p],
  antigravity: p => ['-a', 'Antigravity IDE', p],
}

async function openIdeLocal(ideName, path) {
  try {
    // Terminal goes through a dedicated command (not `open -a Terminal <path>`) so it gets the
    // same cold-start double-window fix as SSH terminal / run_project_command.
    if (ideName === 'terminal') {
      await invoke('open_local_terminal', { localPath: path });
      return;
    }
    const args = IDE_LOCAL_ARGS[ideName]?.(path)
    if (args) await invoke('macos_open', { args });
  } catch (e) {
    console.error(e);
  }
}

// Shared invoke/Toast wrapper for the popup's run-commands row — BUILD and DEV differ only by
// which Tauri command they call and the success wording (DEV also opens a browser once the dev
// server is up; that decision lives entirely in Rust, see run_project_dev).
async function invokeProjectRun(command, path, cmd, successTitle) {
  try {
    await invoke(command, { localPath: path, cmd });
    Toast.fire({ icon: 'success', title: successTitle });
  } catch (e) {
    console.error(`Failed to run project command (${command}):`, e);
    Toast.fire({ icon: 'error', title: String(e).replace('Error: ', '') });
  }
}

async function runProjectCommand(path, cmd) {
  return invokeProjectRun('run_project_command', path, cmd, 'Command started in Terminal!');
}

async function runProjectDev(path, cmd) {
  return invokeProjectRun('run_project_dev', path, cmd, 'Dev server starting in Terminal — browser will open when ready.');
}

// (host, path) -> absolute path. The remote $HOME never changes within a session, so a
// resolved path is stable — cache it and pay the SSH round-trip at most once per host+path.
// Only IDE-open needs this now (copy uses the raw path); the cache keeps repeated opens instant.
const resolvedPathCache = new Map();

async function resolveRemoteFullPath(host, path) {
  const needsResolve = path.startsWith('~/') || path === '~' || path.includes('$HOME');
  if (!needsResolve) {
    return path.startsWith('/') ? path : `/${path}`;
  }
  const key = `${host} ${path}`;
  const cached = resolvedPathCache.get(key);
  if (cached) return cached;

  let resolvedPath = path;
  try {
    resolvedPath = await invoke('resolve_remote_path', { host, path });
  } catch (e) {
    console.error('Failed to resolve remote path', e);
  }
  const full = resolvedPath.startsWith('/') ? resolvedPath : `/${resolvedPath}`;
  // Only cache a real resolve (SSH succeeded → value changed); never pin a failed fallback.
  if (resolvedPath !== path) resolvedPathCache.set(key, full);
  return full;
}

const copiedPathKey = ref(null);

function flashCopied(key) {
  copiedPathKey.value = key;
  setTimeout(() => { if (copiedPathKey.value === key) copiedPathKey.value = null; }, 1500);
}

async function copyLocalPath(project) {
  try {
    await navigator.clipboard.writeText(project.local_path);
    flashCopied(`local-${project.id}`);
  } catch (e) {
    console.error('Failed to copy local path', e);
  }
}

async function copyRemotePath(project) {
  try {
    // Copy the stored remote path verbatim — mirror copyLocalPath. `~` is a valid,
    // portable path on the remote (shells/scp/rsync expand it there), so copying it
    // needs zero network work. The old code awaited resolveRemoteFullPath here, which
    // fired a blocking SSH `echo $HOME` per click (system.rs) and froze the UI for
    // seconds — for an operation that is just "copy an existing field".
    await navigator.clipboard.writeText(project.remote_path);
    flashCopied(`remote-${project.id}`);
  } catch (e) {
    console.error('Failed to copy remote path', e);
  }
}

// Pulls REPORT.html from the remote first if it's newer than the local copy (or local has none),
// then opens the local file in the OS default browser — REPORT.html is a self-contained HTML/JS/CSS
// page (akihtmlreport skill output) that the app's own strict CSP would otherwise break.
async function openReportHtml(project) {
  try {
    const path = await invoke('resolve_report_html', {
      localPath: project.local_path,
      remoteHost: project.remote_host || null,
      remotePath: project.remote_path || null,
    });
    await invoke('macos_open', { args: [path] });
  } catch (e) {
    console.error('Failed to open REPORT.html', e);
    Toast.fire({ icon: 'error', title: String(e).replace('Error: ', '') });
  }
}

async function openIdeRemote(ideName, host, path) {
  try {
    const remotePath = await resolveRemoteFullPath(host, path);
    if (ideName === 'vscode') {
      await invoke('macos_open', { args: [`vscode://vscode-remote/ssh-remote+${host}${remotePath}`] })
    } else if (ideName === 'vscode_insiders') {
      await invoke('macos_open', { args: [`vscode-insiders://vscode-remote/ssh-remote+${host}${remotePath}`] })
    } else {
      await invoke('open_remote_subprocess', { ideName, host, path: remotePath })
    }
  } catch (e) {
    console.error(e);
    Toast.fire({ icon: 'error', title: String(e).replace('Error: ', '') });
  }
}

async function openUrl(url) {
  try { await invoke('macos_open', { args: [url] }); } catch (e) { console.error(e); }
}

function getDevCmd(p) {
  return p.dev_cmd_override || projectRuntime.value[p.id]?.stack_info?.dev_cmd || ''
}

function getBuildCmd(p) {
  return p.build_cmd_override || projectRuntime.value[p.id]?.stack_info?.build_cmd || ''
}

function formatTimeAgo(timestamp) {
  if (!timestamp) return "Never";
  const seconds = Math.floor(Date.now() / 1000) - timestamp;
  if (seconds < 60) return "Just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}
</script>

<style scoped>
.projects-table-container {
  width: 100%;
  /* project-info | tasks | git | last-sync | action (OPEN + select-push) | sync (PUSH/DRY/PULL + LOG + gear) */
  --grid-cols: 12rem 2.5rem 2.5rem 2.5rem 7rem 1fr;
  --grid-gap: 0.5rem;
}

.projects-grid {
  display: flex;
  flex-direction: column;
  width: 100%;
}

.grid-header {
  display: grid;
  grid-template-columns: var(--grid-cols);
  column-gap: var(--grid-gap);
  align-items: center;
  width: 100%;
  position: sticky;
  top: 0;
  background: rgba(10, 15, 22, 0.95);
  backdrop-filter: blur(8px);
  border-bottom: 1px solid var(--border-card);
  z-index: 10;
  box-sizing: border-box;
}

.grid-header-cell {
  padding: 6px 0;
  font-size: 10px;
  font-weight: 800;
  color: #a5f3fc;
  letter-spacing: 1px;
  text-transform: uppercase;
  white-space: nowrap;
  text-align: center;
}

/* text-align:center only centers inline content — several cells hold block-level children
   (badges, stacked divs) that ignore it. Flex-centering every non-project column is the only
   way that's actually reliable for both the label row and the row content below it. The
   project-info column is deliberately excluded: it's left-aligned by design and already has
   its own internal flex layout (project-info-row) plus a space-between header
   (.col-project-info-header) that a blanket rule here would fight with. */
.grid-header-cell:not(:first-child) {
  display: flex;
  align-items: center;
  justify-content: center;
}

.grid-body {
  display: flex;
  flex-direction: column;
  width: 100%;
}

.grid-row {
  display: grid;
  grid-template-columns: var(--grid-cols);
  column-gap: var(--grid-gap);
  align-items: center;
  width: 100%;
  border-bottom: 1px solid rgba(255, 255, 255, 0.03);
  transition: background 0.15s;
  box-sizing: border-box;
  -webkit-user-drag: element !important;
}

.grid-row:hover {
  background: rgba(255, 255, 255, 0.02);
}

.row-syncing {
  background: rgba(6, 182, 212, 0.05);
}

.grid-row-cell {
  padding: 6px 0;
  white-space: nowrap;
  align-self: center;
  text-align: center;
}

.grid-row-cell:not(:first-child) {
  display: flex;
  align-items: center;
  justify-content: center;
}

/* LAST holds two stacked divs (the pull/push badge+time, then the host line below it) — they
   need to stay stacked, not sit side by side as flex-row items. */
.col-last-sync {
  flex-direction: column;
}

.grid-header-cell:first-child,
.grid-row-cell:first-child {
  padding-left: 6px;
  text-align: left;
}

/* New Project moved here from the app header (next to the project count) — same
   btn-tech-primary cyan vibe as before, just relocated + a persistent (not hover-only) glow
   so it still reads as the primary create action at a glance. */
.col-project-info-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  padding-right: 6px;
}

.btn-new-project-inline {
  flex-shrink: 0;
  height: 24px;
  padding: 0 10px;
  font-size: 10px;
  box-shadow: 0 0 6px rgba(0, 210, 255, 0.25);
}

.btn-new-project-inline:hover:not(:disabled) {
  box-shadow: 0 0 12px rgba(0, 210, 255, 0.5);
}

.grid-header-cell:last-child,
.grid-row-cell:last-child {
  padding-right: 12px;
}

.grid-row-special {
  display: flex;
  width: 100%;
}

.col-tasks,
.col-git-status,
.col-last-sync,
.col-action,
.col-sync {
  padding-left: 0 !important;
  padding-right: 0 !important;
}

/* Reset widths from main.css to let CSS Grid control layout */
.col-project-info,
.col-tasks,
.col-git-status,
.col-last-sync,
.col-action,
.col-sync {
  width: auto !important;
  max-width: none !important;
}

.th-with-ring {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

/* Icon + name/path block row (moved out of an inline style so the narrow media query below can
   reach the gap — RULE-ui-pattern: no styling logic stranded in inline attributes). */
.project-info-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.project-text-col {
  flex: 1;
  min-width: 0;
  padding-right: 6px;
}


/* Drag handle: project icon vùng */
.project-drag-handle {
  position: relative;
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.2);
  border-radius: 6px;
  overflow: hidden;
  flex-shrink: 0;
  cursor: grab;
  transition: box-shadow 0.15s, outline 0.15s;
}

.project-drag-handle img,
.project-drag-handle i {
  -webkit-user-drag: none !important;
  pointer-events: none;
}

/* Lớp chấm chấm phủ lên góc trên-trái để gợi ý có thể kéo */
.project-drag-handle::before {
  content: '';
  position: absolute;
  inset: 0;
  background-color: rgba(0, 0, 0, 0.45);
  /* Nền tối mờ phủ lên trên ảnh */
  background-image:
    radial-gradient(circle, rgba(255, 255, 255, 0.8) 1.2px, transparent 1.2px);
  background-size: 5px 5px;
  background-position: center;
  opacity: 0;
  transition: opacity 0.15s;
  pointer-events: none;
  border-radius: 6px;
  z-index: 1;
  /* Nổi lên trên cùng ảnh icon */
}

.project-drag-handle:hover::before {
  opacity: 1;
}

.project-drag-handle:active {
  cursor: grabbing;
}


.row-dragging {
  opacity: 0.4;
}

/* Ngăn chặn child elements nhận mouse events khi đang kéo, đảm bảo WebKit ghi nhận sự kiện drop lên grid-row */
.projects-grid.dragging-active .grid-row * {
  pointer-events: none;
}

/* Transition Group list styles */
.project-list-move {
  transition: transform 0.3s cubic-bezier(0.25, 0.8, 0.25, 1);
}

/* Tiny host line under the LAST ACT badge — which remote the action ran against */
.sync-host {
  font-size: 9px;
  line-height: 1.2;
  color: rgba(255, 255, 255, 0.35);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}

.git-cell {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
}

.remote-actions-fieldset {
  display: flex;
  align-items: center;
  gap: 6px;
  border: none;
  margin: 0;
  padding: 0;
}

fieldset:disabled .switch {
  opacity: 0.4;
  cursor: not-allowed;
  pointer-events: none;
}

.actions-wrapper .btn-tech {
  padding: 0 8px !important;
}

.actions-wrapper .btn-tech-push,
.actions-wrapper .btn-tech-pull {
  padding: 0 6px !important;
}

.actions-wrapper .btn-action-open {
  padding: 0 10px !important;
}

.actions-wrapper .btn-action-open i {
  margin-left: 0 !important;
}

/* Open Popup */
.open-popup-wrapper {
  position: relative;
  display: inline-flex;
}

.open-popup {
  position: fixed;
  z-index: 80;
  background: rgba(22, 30, 44, 0.98);
  border: 1px solid rgba(0, 210, 255, 0.25);
  border-radius: 8px;
  padding: 8px 0 6px 0;
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.7);
  backdrop-filter: blur(8px);
  will-change: transform, opacity;

  visibility: hidden;
  opacity: 0;
  transform: scale(0.96);
  transition: opacity 0.15s ease, visibility 0.15s ease, transform 0.15s ease;
  transition-delay: 0.15s;
  pointer-events: none;
}

.open-popup::before {
  content: "";
  position: absolute;
  top: 100%;
  left: 0;
  right: 0;
  height: 12px;
  background: transparent;
}

.open-popup-wrapper:hover .open-popup {
  visibility: visible;
  opacity: 1;
  transform: scale(1);
  transition-delay: 0s;
  pointer-events: auto;
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
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 9px;
  text-transform: uppercase;
  letter-spacing: 0.1em;
  color: rgba(255, 255, 255, 0.35);
  padding: 4px 12px 2px;
  user-select: none;
}

.popup-copy-btn {
  background: transparent;
  border: none;
  color: rgba(255, 255, 255, 0.35);
  cursor: pointer;
  padding: 0 2px;
  font-size: 9px;
  letter-spacing: 0.1em;
  transition: color 0.15s;
}

.popup-copy-btn:hover {
  color: var(--accent-cyan, #00d2ff);
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

.popup-run-row {
  display: flex;
  gap: 0;
  border-top: 1px solid rgba(255, 255, 255, 0.06);
  margin-top: 2px;
  padding-top: 2px;
}

.popup-run-btn {
  flex: 1;
  justify-content: center;
  font-weight: 700;
  font-size: 11px;
  letter-spacing: 0.05em;
}

.popup-icon {
  width: 14px;
  height: 14px;
  object-fit: contain;
  flex-shrink: 0;
  filter: drop-shadow(0 0 2px rgba(255, 255, 255, 0.18));
}

.popup-icon-insiders {
  filter: hue-rotate(-50deg) saturate(2) brightness(1.2) drop-shadow(0 0 2px rgba(255, 255, 255, 0.18));
}

.popup-project-icon {
  width: 18px;
  height: 18px;
  border-radius: 3px;
  object-fit: contain;
  margin-right: 6px;
  vertical-align: middle;
}

/* DIVERGED state — orange outline only, zero extra space */
.dry-group.is-diverged {
  outline: 1px solid rgba(251, 146, 60, 0.5);
  border-radius: 6px;
}

.btn-sync-diverged {
  box-shadow: 0 0 0 1px rgba(251, 146, 60, 0.6) !important;
}

/* Narrow mode (<=700px) — single shared breakpoint for the whole app (see
   docs/plan/done/narrow-mode-and-ux-1.14.0.md, "Shared contract"). Label hiding uses the global
   .u-narrow-hide / .u-wide-hide utilities from main.css; this block only covers layout that a
   utility class can't express — column widths, gaps. */
@media (max-width: 700px) {
  .projects-table-container {
    /* Project name column: 12rem -> 6.5rem (widened back up from an initial 4.8rem/40% guess —
       that was too tight to show any of the remote path, this leaves a few characters visible).
       GIT column: 2.5rem -> 1.7rem read as too tight against LAST, opened back up to 2.1rem.
       TASKS column trimmed a touch (2.5rem -> 2.1rem) — it's just an icon+badge, was carrying
       more blank space than it needed. Action column (OPEN + select-push) also narrows since
       OPEN's label now hides at the same 700px breakpoint via u-narrow-hide. */
    --grid-cols: 6.5rem 2.1rem 1.9rem 2.5rem 4.2rem 1fr;
    --grid-gap: 0.4rem;
  }

  .project-info-row {
    gap: 6px;
  }

  /* The project name/path block had unused padding trailing short names — tighten it so
     ellipsis-truncated paths get a couple more characters of room instead of dead space. */
  .project-text-col {
    padding-right: 0;
  }

  /* PUSH/PULL lose their text label at this width — match them to the OPEN button's icon-only
     footprint (10px) rather than the wider guess that was breaking the layout. Must win over the
     `.actions-wrapper .btn-tech-push/-pull { padding: 0 6px !important }` rule above, which
     otherwise silently wins on specificity + !important regardless of this media query. */
  .actions-wrapper .btn-tech-push,
  .actions-wrapper .btn-tech-pull {
    padding: 0 10px !important;
  }

  /* Reduce OPEN button padding slightly so it fits in the action column */
  .actions-wrapper .btn-action-open {
    padding: 0 6px !important;
  }

  /* Every attempt to hang DRY off the bottom edge (position: absolute, overlapping the row's
     border) ended up colliding with the row below it — nothing anchored to a row's own box can
     overlap outside it without fighting that next row's own positioned content for paint order.
     Kept in normal flow instead: small, but a real flex item between PUSH and PULL, so it can
     never visually merge with anything else. */
  .dry-group {
    margin: 0 2px;
    gap: 3px;
    padding: 2px 4px;
    overflow: visible;
  }

  .dry-group-left,
  .dry-group-right {
    padding: 0;
  }

  /* PUSH's count badge (CountBadgeWrap) overhangs 5px past the button's top-right corner.
     To prevent it from overlapping the "DRY" text in the tight narrow layout, we add extra margin.
     PULL's badge (in the right group) overhangs to the right, which causes it to poke outside
     the border of the .dry-group box, so we add a little margin there too. */

  .dry-toggle-center {
    padding: 0 2px !important;
  }

  .dry-toggle-center .dry-label {
    font-size: 6px;
    margin-bottom: 1px;
  }

  .dry-toggle-center .switch-sm {
    width: 16px;
    height: 8px;
  }

  /* Ball must be vertically centered in the 8px track: (8 - 6) / 2 = 1px on each side.
     The base .switch-sm rule (main.css) uses bottom: 2px, sized for the 12px track — left
     uncorrected here, the ball sat flush against the top edge instead of centered. */
  .dry-toggle-center .switch-sm .slider:before {
    height: 6px;
    width: 6px;
    bottom: 1px;
    left: 1px;
  }

  .dry-toggle-center .switch-sm input:checked+.slider:before {
    transform: translateX(8px);
  }

  /* GIT sits noticeably closer to LAST than the gap elsewhere reads as needing — pull LAST left
     a touch rather than shrinking --grid-gap globally (that would also tighten LAST↔ACTION and
     ACTION↔SYNC, which need the opposite). */
  .col-last-sync {
    margin-left: -6px;
  }

  /* Shrink the LAST column's own text (PUSH/PULL badge, relative time, host line) to help it
     fit in the now-tighter gap. */
  .last-sync-badge {
    font-size: 9px;
  }

  .sync-time {
    font-size: 8px;
  }

  .sync-host {
    font-size: 8px;
  }
}
</style>
