<template>
  <div class="projects-table-container">
    <div class="projects-grid" :class="{ 'dragging-active': dragFromIndex !== null }">
      <!-- Header -->
      <div class="grid-header">
        <div class="grid-header-cell col-project-info">PROJECT / PATH</div>
        <div class="grid-header-cell col-git-status" title="LOCAL GIT">
          <span class="th-with-ring">
            GIT
            <RefreshRing :interval-s="refreshSettings.git_interval_s" :refresh-key="gitRefreshKey" stroke-color="rgba(16, 185, 129, 0.6)" />
          </span>
        </div>
        <div class="grid-header-cell col-last-sync">LAST ACT</div>
        <div class="grid-header-cell col-actions">
          <span class="th-with-ring">
            ACTIONS
            <RefreshRing :interval-s="refreshSettings.remote_diff_interval_s" :refresh-key="diffRefreshKey" stroke-color="rgba(255, 140, 0, 0.6)" />
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
          @mousedown="onRowMouseDown"
        >
          <!-- Cell 1: Project Info -->
          <div class="grid-row-cell col-project-info">
            <div style="display: flex; align-items: center; gap: 12px;">
              <!-- Project Icon (drag handle) -->
              <div
                class="project-drag-handle icon-glow"
                title="Kéo để sắp xếp"
                @mousedown="isHandleMouseDown = true"
              >
                <img v-if="projectIcons[p.id]" :src="projectIcons[p.id]" style="width: 100%; height: 100%; object-fit: cover;" draggable="false" />
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
          </div>

          <!-- Cell 2: Git Status -->
          <div class="grid-row-cell col-git-status">
            <div class="git-cell">
              <button class="btn-action-git" @click="openGitModal(p)" title="Git Actions (Commit & Push to Remote Git)" aria-label="Git Actions">
                <i class="fa-brands fa-git-alt"></i>
              </button>
              <span class="git-badge" :class="'git-' + (projectRuntime[p.id]?.git_status || 'Unknown').replace(' ', '-')">
                {{ projectRuntime[p.id]?.git_status || '...' }}
              </span>
            </div>
          </div>

          <!-- Cell 3: Last Sync -->
          <div class="grid-row-cell col-last-sync">
            <div v-if="p.last_sync_action" class="last-sync-badge" :class="p.last_sync_action.includes('PULL') ? 'badge-pull' : 'badge-push'">
              {{ p.last_sync_action }} <span class="sync-time">{{ formatTimeAgo(p.last_sync_time) }}</span>
            </div>
            <div v-else class="text-muted">Never</div>
          </div>

          <!-- Cell 4: Actions -->
          <div class="grid-row-cell col-actions">
            <div class="actions-wrapper">
              <!-- Open Popup Trigger (OPEN Button) -->
              <div class="open-popup-wrapper" @mouseenter="onOpenEnter(p, $event)" @mouseleave="onOpenLeave()">
                <button class="btn-tech btn-tech-primary btn-action-open" title="Open Popup">
                  <span class="btn-text">OPEN</span> <i class="fa-solid fa-caret-down"></i>
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

                      <!-- REMOTE -->
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
                <i class="fa-solid fa-hand-pointer btn-select-icon-only" style="display: none;"></i>
                <span class="btn-text">SELECT</span>
              </button>

              <div class="dry-group" :class="p.dry_run ? 'is-safe' : 'is-danger'">
                <div class="dry-group-left">
                  <label class="btn-tech-git-inline" :class="{ 'active': p.sync_git }" title="Include .git in Push">
                    <input type="checkbox" v-model="p.sync_git" :disabled="projectRuntime[p.id]?.syncing" @change="saveProjectsList()" />
                    <i class="fa-brands fa-git-alt btn-git-icon-only" style="display: none;"></i>
                    <span class="btn-text">.git</span>
                  </label>
                  <button class="btn-tech btn-tech-push" :class="{ 'btn-sync-clean': projectRuntime[p.id]?.hasPendingPush === false, 'btn-sync-checking': projectRuntime[p.id]?.hasPendingPush === null }" @click="startSync(p, 'push')" :disabled="projectRuntime[p.id]?.syncing" title="Push Local to Remote">
                    <i class="fa-solid fa-cloud-arrow-up"></i> <span class="btn-text">PUSH</span>
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
                    <i class="fa-solid fa-cloud-arrow-down"></i> <span class="btn-text">PULL</span>
                  </button>
                </div>
              </div>

              <button class="btn-tech btn-tech-secondary" :class="{ 'log-active': activeLogProjectId === p.id }" @click="toggleProjectLog(p.id)" title="View Project Log">
                <i class="fa-solid fa-file-lines btn-log-icon-only" style="display: none;"></i>
                <span class="btn-text">LOG</span>
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
import { gitRefreshKey, diffRefreshKey } from '../composables/useBackgroundRefresh';
import { refreshSettings } from '../store/refreshStore';
import { Toast } from '../store/projectStore';
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
  terminal: p => ['-a', 'Terminal', p],
  vscode: p => ['-a', 'Visual Studio Code', p],
  vscode_insiders: p => ['-a', 'Visual Studio Code - Insiders', p],
  antigravity: p => ['-a', 'Antigravity IDE', p],
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
  } catch (e) {
    console.error(e);
    Toast.fire({ icon: 'error', title: String(e).replace('Error: ', '') });
  }
}

async function openUrl(url) {
  try { await invoke('macos_open', { args: [url] }); } catch (e) { console.error(e); }
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
  --grid-cols: 13.5rem 5rem 3.8rem 1fr;
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
  text-align: left;
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
}

.grid-header-cell:first-child,
.grid-row-cell:first-child {
  padding-left: 12px;
  text-align: left;
}

.grid-header-cell:last-child,
.grid-row-cell:last-child {
  padding-right: 12px;
}

.grid-row-special {
  display: flex;
  width: 100%;
}

.col-git-status,
.col-last-sync,
.col-actions {
  padding-left: 0 !important;
  padding-right: 0 !important;
}

/* Reset widths from main.css to let CSS Grid control layout */
.col-project-info,
.col-git-status,
.col-last-sync,
.col-actions {
  width: auto !important;
  max-width: none !important;
}

.th-with-ring {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding-right: 8px;
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
  background-color: rgba(0, 0, 0, 0.45); /* Nền tối mờ phủ lên trên ảnh */
  background-image:
    radial-gradient(circle, rgba(255, 255, 255, 0.8) 1.2px, transparent 1.2px);
  background-size: 5px 5px;
  background-position: center;
  opacity: 0;
  transition: opacity 0.15s;
  pointer-events: none;
  border-radius: 6px;
  z-index: 1; /* Nổi lên trên cùng ảnh icon */
}

.project-drag-handle:hover::before {
  opacity: 1;
}

.project-drag-handle:active {
  cursor: grabbing;
}

.grid-row:hover .project-drag-handle::before {
  opacity: 0.55;
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

.git-cell {
  display: flex;
  align-items: center;
  gap: 4px;
}

/* Giảm padding toàn diện cho các nút bấm trong bảng */
.actions-wrapper .btn-tech {
  padding: 0 8px !important;
}

.actions-wrapper .btn-tech-git-inline {
  padding: 0 8px !important;
}

.actions-wrapper .btn-action-open {
  padding: 0 6px !important;
}

@media (max-width: 800px) {
  .projects-table-container {
    --grid-cols: 11rem 4.5rem 3.5rem 1fr;
    --grid-gap: 0.25rem;
  }

  .col-git-status,
  .col-last-sync,
  .col-actions {
    padding-left: 0 !important;
    padding-right: 0 !important;
  }

  .actions-wrapper .btn-tech,
  .actions-wrapper .btn-action-open,
  .actions-wrapper .btn-tech-git-inline {
    padding: 0 12px !important;
  }

  .actions-wrapper .btn-tech .btn-text,
  .actions-wrapper .btn-tech-git-inline .btn-text {
    display: none !important;
  }

  .actions-wrapper .btn-select-icon-only,
  .actions-wrapper .btn-git-icon-only,
  .actions-wrapper .btn-log-icon-only {
    display: inline-block !important;
  }

  .actions-wrapper .btn-action-open i {
    margin-left: 0 !important;
  }
}

/* Open Popup */
.open-popup-wrapper {
  position: relative;
  display: inline-flex;
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
  filter: drop-shadow(0 0 2px rgba(255, 255, 255, 0.18));
}

.popup-icon-insiders {
  filter: hue-rotate(-50deg) saturate(2) brightness(1.2) drop-shadow(0 0 2px rgba(255, 255, 255, 0.18));
}
</style>
