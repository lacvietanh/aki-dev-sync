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
        <div class="grid-header-cell col-terminal" title="TERMINAL">
          <button
            class="btn-cell-trigger th-term-btn"
            :class="{ 'is-live': globalTabCount > 0 }"
            @click="openGlobalTerminal()"
            :title="globalTermTitle"
            aria-label="Global terminal">
            <i class="fa-solid fa-terminal"></i>
            <TerminalCountBadges :tabs="globalTabCount" :external="externalTermGlobalCount" :exited="globalHasExited" />
          </button>
        </div>
        <div class="grid-header-cell col-action" title="OPEN / SELECT-PUSH">ACTION</div>
        <div class="grid-header-cell col-sync">
          <span class="th-with-ring">
            SYNC
            <RefreshRing :interval-s="syncCheckEnabled ? refreshSettings.remote_diff_interval_s : 0" :refresh-key="diffRefreshKey" stroke-color="rgba(255, 140, 0, 0.6)" />
            <i class="fa-solid fa-power-off src-power" :class="syncCheckEnabled ? 'is-on' : 'is-off'" @click="toggleSyncCheck"
               :title="syncCheckEnabled ? 'Sync check ON - click to stop all remote diff/push/pull' : 'Sync check OFF - click to enable'"></i>
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
                   title="Drag to reorder"
                   @mousedown="isHandleMouseDown = true">
                <img v-if="!failedIcons[p.id] && projectIconSrc(p.id, iconTimestamp)" :src="projectIconSrc(p.id, iconTimestamp)" style="width: 100%; height: 100%; object-fit: cover;" draggable="false" @error="failedIcons[p.id] = true" />
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
                  <!-- u-select-text (main.css): paths are copied by hand often enough that the
                       app-wide no-selection default has to be lifted here. -->
                  <span class="path-local u-select-text" :title="p.local_path"><i class="fa-solid fa-laptop-code text-cyan mr-1"></i> {{ p.local_path }}</span>
                  <span v-if="p.remote_host" class="path-remote u-select-text" :title="`${p.remote_host}:${p.remote_path}`"><i class="fa-solid fa-cloud text-amber mr-1"></i> {{ p.remote_host }}:{{ p.remote_path }}</span>
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
                          'git-no-repo': !isPathMissing(p) && ['No Git', 'Git Error'].includes(projectRuntime[p.id]?.git_status),
                          'git-ahead': projectRuntime[p.id]?.git_status === 'Ahead',
                          'git-path-missing': isPathMissing(p),
                        }"
                        @click="openGitModal(p)"
                        :title="isPathMissing(p) ? `Local path not found - volume not mounted?\n${p.local_path}` : projectRuntime[p.id]?.git_status === 'No Git' ? 'No Git repository' : projectRuntime[p.id]?.git_status === 'Git Error' ? 'Git error - click to view' : projectRuntime[p.id]?.git_changed_count > 0 ? `Git Actions (${projectRuntime[p.id].git_changed_count} changed file(s))` : projectRuntime[p.id]?.git_status === 'Ahead' ? 'Ahead of remote - click to push' : 'Git Actions (Commit & Push to Remote Git)'"
                        aria-label="Git Actions">
                  <i class="fa-brands fa-git-alt"></i>
                </button>
              </CountBadgeWrap>
            </div>
          </div>

          <!-- Cell 4: Terminal -->
          <div class="grid-row-cell col-terminal">
            <TerminalCell :project="p" />
          </div>

          <!-- Cell 5: Action (OPEN + SELECT-push only) -->
          <div class="grid-row-cell col-action">
            <div class="actions-wrapper">
              <!-- Open Popup Trigger (OPEN Button) -->
              <div class="open-popup-wrapper"
                   :class="{ 'is-open': openPopupId === p.id }"
                   @mouseenter="onOpenHover(p, $event)"
                   @mouseleave="onOpenHoverLeave(p)">
                <button class="btn-tech btn-tech-primary btn-action-open" title="Open Popup" @click.stop="toggleOpenPopup(p, $event)">
                  <span class="btn-text u-narrow-hide">OPEN</span> <i class="fa-solid fa-caret-up"></i>
                </button>

                <!-- Open Popup — visibility is state (.is-open), not CSS :hover: a phone has no
                     hover, so the popup was unreachable there. Hover still opens it on the Mac. -->
                <div class="open-popup" :style="popupStyles[p.id]">
                  <div class="popup-header" :title="p.name" style="display: flex; align-items: center;">
                    <img v-if="!failedIcons[p.id] && projectIconSrc(p.id, iconTimestamp)" :src="projectIconSrc(p.id, iconTimestamp)" class="popup-project-icon" alt="" @error="failedIcons[p.id] = true" />
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
                      <!-- Everything below consumes the local path, so a missing volume greys the
                           whole list (COPY above deliberately stays live — copying a path you are
                           about to go fix is legitimate). `localBlocked` also treats a not-yet-loaded
                           ideAvailability as UNAVAILABLE rather than available. -->
                      <div class="popup-item" :class="{ 'popup-disabled': localBlocked(p) }" :title="localTitle(p)" @click="openIdeLocal('finder', p.local_path)">
                        <i class="fa-solid fa-folder-open" style="width:14px; color: #fbbf24;"></i> Finder
                      </div>
                      <!-- In-app first: it is the only one of the two that works from a phone,
                           which is the whole reason the in-app terminal exists. -->
                      <div class="popup-item" :class="{ 'popup-disabled': localBlocked(p) }" :title="localTitle(p)" @click="openProjectTerminal(p)">
                        <i class="fa-solid fa-terminal" style="width:14px; color: var(--accent-cyan);"></i> In-App Terminal
                      </div>
                      <div class="popup-item" :class="{ 'popup-disabled': localBlocked(p) }" :title="localTitle(p)" @click="openIdeLocal('terminal', p.local_path)">
                        <i class="fa-solid fa-terminal" style="width:14px;"></i> Terminal
                      </div>
                      <div class="popup-item" :class="{ 'popup-disabled': localBlocked(p, 'vscode') }" :title="localTitle(p)" @click="openIdeLocal('vscode', p.local_path)">
                        <img src="/vscode-icon.png" class="popup-icon" alt="VSCode" /> VSCode
                      </div>
                      <div class="popup-item" :class="{ 'popup-disabled': localBlocked(p, 'vscode_insiders') }" :title="localTitle(p)" @click="openIdeLocal('vscode_insiders', p.local_path)">
                        <img src="/vscode-icon.png" class="popup-icon popup-icon-insiders" alt="VSCode Insiders" /> VSCode Insiders
                      </div>
                      <div class="popup-item" :class="{ 'popup-disabled': localBlocked(p, 'antigravity') }" :title="localTitle(p)" @click="openIdeLocal('antigravity', p.local_path)">
                        <img src="/antigravity-icon.png" class="popup-icon" alt="Antigravity" /> Antigravity IDE
                      </div>
                      <!-- DEV/BUILD are ALWAYS rendered, disabled when nothing resolves - they used
                           to be v-if'd out of the DOM, which silently removed the affordance from
                           every project outside the three stacks system.rs detects a command for,
                           with no visible route back to Project Settings' RUN COMMANDS field.
                           docs/plan/done/dev-build-visibility.md -->
                      <div class="popup-run-row">
                        <div class="popup-item popup-run-btn" :class="{ 'popup-disabled': localBlocked(p) || !getDevCmd(p) }" @click="!localBlocked(p) && getDevCmd(p) && runProjectDev(p, getDevCmd(p))" :title="runCmdTitle(p, getDevCmd(p), 'dev')">
                          <i class="fa-solid fa-terminal" style="width:14px; color: var(--accent-green, #10b981);"></i> DEV
                        </div>
                        <div class="popup-item popup-run-btn" :class="{ 'popup-disabled': localBlocked(p) || !getBuildCmd(p) }" @click="!localBlocked(p) && getBuildCmd(p) && runProjectCommand(p, getBuildCmd(p))" :title="runCmdTitle(p, getBuildCmd(p), 'build')">
                          <i class="fa-solid fa-hammer" style="width:14px; color: #f59e0b;"></i> BUILD
                        </div>
                      </div>
                    </div>

                    <!-- REMOTE -->
                    <!-- The column itself is NOT gated on the sync switch: SSH Terminal, the remote
                         IDE entries and COPY are ways to REACH the server, not rsync traffic, and
                         hiding them was over-reach. Only Upload actually pushes files, so only it
                         is gated (disabled + a tooltip that says why - hiding it would leave the
                         user hunting for a menu item that used to be there). -->
                    <div v-if="p.remote_host && p.remote_path" style="flex: 1; min-width: 180px; border-left: 1px solid rgba(255, 255, 255, 0.07); padding-left: 4px;">
                      <div class="popup-section-label">
                        <span>☁️ REMOTE (SSH)</span>
                        <button class="popup-copy-btn" @click.stop="copyRemotePath(p)" :title="copiedPathKey === `remote-${p.id}` ? 'Copied!' : 'Copy full path'">
                          <i class="fa-solid" :class="copiedPathKey === `remote-${p.id}` ? 'fa-check' : 'fa-copy'"></i> COPY
                        </button>
                      </div>
                      <!-- In-app first, same reasoning as LOCAL's In-App Terminal above: the only one of the two
                           that works from a phone. -->
                      <div class="popup-item" @click="openProjectRemoteTerminal(p)">
                        <i class="fa-solid fa-terminal" style="width:14px; color: var(--accent-cyan);"></i> SSH Terminal (In-App)
                      </div>
                      <div class="popup-item" @click="openIdeRemote('terminal', p.remote_host, p.remote_path)">
                        <i class="fa-solid fa-terminal" style="width:14px;"></i> SSH Terminal
                      </div>
                      <div class="popup-item" :class="{ 'popup-disabled': ideMissing('vscode') }" @click="openIdeRemote('vscode', p.remote_host, p.remote_path)">
                        <img src="/vscode-icon.png" class="popup-icon" alt="VSCode" /> VSCode (Remote SSH)
                      </div>
                      <div class="popup-item" :class="{ 'popup-disabled': ideMissing('vscode_insiders') }" @click="openIdeRemote('vscode_insiders', p.remote_host, p.remote_path)">
                        <img src="/vscode-icon.png" class="popup-icon popup-icon-insiders" alt="VSCode Insiders" /> VSCode Insiders (Remote)
                      </div>
                      <div class="popup-item" :class="{ 'popup-disabled': ideMissing('antigravity') }" @click="openIdeRemote('antigravity', p.remote_host, p.remote_path)">
                        <img src="/antigravity-icon.png" class="popup-icon" alt="Antigravity" /> Antigravity (Remote)
                      </div>
                      <div class="popup-item"
                           :class="{ 'popup-disabled': projectRuntime[p.id]?.syncing || !syncCheckEnabled }"
                           @click="!projectRuntime[p.id]?.syncing && syncCheckEnabled && requestSelectPush(p.id)"
                           :title="!syncCheckEnabled ? 'Sync check is off - turn it on (power icon in the SYNC column header) to push files' : 'Pick specific files/folders (native file picker) and push only those to Remote - bypasses this project\'s exclude list, unaffected by the DRY toggle'">
                        <i class="fa-solid fa-upload" style="width:14px; color: #38bdf8;"></i> Upload (select files)
                      </div>
                    </div>
                  </div>
                </div>
              </div>

              <button class="btn-tech btn-tech-secondary"
                      @click="requestRefresh(p.id)"
                      :disabled="projectRuntime[p.id]?.syncing || isRefreshing(p.id) || !syncCheckEnabled"
                      :title="!syncCheckEnabled ? 'Sync check is off' : isRefreshing(p.id) ? 'Refreshing…' : 'Refresh this project only - git status, remote diff and dev/build commands. Does not touch other projects or the usage monitors (unlike the global refresh in the header).'">
                <i class="fa-solid fa-arrows-rotate" :class="{ 'fa-spin': isRefreshing(p.id) }"></i>
              </button>
            </div>
          </div>

          <!-- Cell 6: Sync (PUSH/DRY/PULL, LOG, config) -->
          <div class="grid-row-cell col-sync">
            <div class="actions-wrapper">
              <div class="sync-cluster">
                <!-- Only the sync-check switch disables the whole group now: while a sync is running
                     one of PUSH/PULL turns into STOP (§3.6) and must stay clickable, and a disabled
                     <fieldset> disables every control inside it regardless of the button's own
                     :disabled. Everything that was disabled-while-syncing still is, per control. -->
                <fieldset :disabled="!syncCheckEnabled" class="remote-actions-fieldset" :title="!syncCheckEnabled ? 'Sync check is off' : ''">
                  <div class="dry-group" :class="[p.dry_run ? 'is-safe' : 'is-danger', projectRuntime[p.id]?.hasPendingPush && projectRuntime[p.id]?.hasPendingPull ? 'is-diverged' : '']">
                    <div class="dry-group-left">
                      <CountBadgeWrap :count="projectRuntime[p.id]?.pushCount || 0"
                                       :delete-armed="p.delete_on_push && !isStop(p, 'push')"
                                       delete-side="left"
                                       delete-title="Mirror: files on the remote that are not here will be deleted.">
                        <button
                                class="btn-tech btn-tech-push"
                                :class="{
                                  'btn-sync-clean': !isStop(p, 'push') && projectRuntime[p.id]?.hasPendingPush === false,
                                  'btn-sync-checking': !isStop(p, 'push') && projectRuntime[p.id]?.hasPendingPush === null,
                                  'btn-sync-diverged': !isStop(p, 'push') && projectRuntime[p.id]?.hasPendingPush && projectRuntime[p.id]?.hasPendingPull,
                                  'btn-sync-stop': isStop(p, 'push')
                                }"
                                :disabled="projectRuntime[p.id]?.syncing && !isStop(p, 'push')"
                                @click="isStop(p, 'push') ? requestCancelSync(p.id) : requestSync(p.id, 'push')"
                                :title="isStop(p, 'push') ? 'Stop this sync now (kills rsync/ssh)' : !syncCheckEnabled ? 'Sync check is off' : projectRuntime[p.id]?.pushCount > 0 ? `Push Local → Remote (${projectRuntime[p.id].pushCount} file(s))` : 'Push Local to Remote'">
                          <i class="fa-solid" :class="isStop(p, 'push') ? 'fa-stop' : 'fa-cloud-arrow-up'"></i> <span class="btn-text u-narrow-hide">{{ isStop(p, 'push') ? 'STOP' : 'PUSH' }}</span>
                        </button>
                      </CountBadgeWrap>
                    </div>

                    <div class="dry-toggle-center" title="Toggle Dry Run">
                      <span class="dry-label">DRY</span>
                      <label class="switch switch-sm">
                        <!-- :checked + @change (NOT v-model): a companion must not mutate its own
                             mirrored `p.dry_run` — the host flips it via setDryRun and the new value
                             mirrors back. On the host this is identical to the old v-model+save. -->
                        <input type="checkbox" :checked="p.dry_run" :disabled="projectRuntime[p.id]?.syncing" @change="setDryRun(p.id, $event.target.checked)" />
                        <span class="slider"></span>
                      </label>
                    </div>

                    <div class="dry-group-right">
                      <CountBadgeWrap :count="projectRuntime[p.id]?.pullCount || 0"
                                       :delete-armed="p.delete_on_pull && !isStop(p, 'pull')"
                                       delete-title="Mirror: files here that are not on the remote will be deleted.">
                        <button
                                class="btn-tech btn-tech-pull"
                                :class="{
                                  'btn-sync-clean': !isStop(p, 'pull') && projectRuntime[p.id]?.hasPendingPull === false,
                                  'btn-sync-checking': !isStop(p, 'pull') && projectRuntime[p.id]?.hasPendingPull === null,
                                  'btn-sync-diverged': !isStop(p, 'pull') && projectRuntime[p.id]?.hasPendingPush && projectRuntime[p.id]?.hasPendingPull,
                                  'btn-sync-stop': isStop(p, 'pull')
                                }"
                                :disabled="projectRuntime[p.id]?.syncing && !isStop(p, 'pull')"
                                @click="isStop(p, 'pull') ? requestCancelSync(p.id) : requestSync(p.id, 'pull')"
                                :title="isStop(p, 'pull') ? 'Stop this sync now (kills rsync/ssh)' : !syncCheckEnabled ? 'Sync check is off' : projectRuntime[p.id]?.pullCount > 0 ? `Pull Remote → Local (${projectRuntime[p.id].pullCount} file(s))` : 'Pull Remote to Local'">
                          <i class="fa-solid" :class="isStop(p, 'pull') ? 'fa-stop' : 'fa-cloud-arrow-down'"></i> <span class="btn-text u-narrow-hide">{{ isStop(p, 'pull') ? 'STOP' : 'PULL' }}</span>
                        </button>
                      </CountBadgeWrap>
                    </div>
                  </div>
                </fieldset>

                <!-- LAST ACTION - two 8px in-flow lines, no separator, no "Never" placeholder
                     (Extreme Narrow: absence of the state IS the "never synced" signal). NEVER
                     position:absolute here - see the collision comment near the narrow-mode
                     .dry-group rule below (an overhang there fights the next row's own content). -->
                <div v-if="p.last_sync_action" class="last-action" :title="`${p.last_sync_action} — ${p.last_sync_host || ''}`">
                  <div class="la-line"><span :class="p.last_sync_action.includes('PULL') ? 'la-pull' : 'la-push'">{{ p.last_sync_action }}</span> {{ formatTimeAgo(p.last_sync_time) }}</div>
                  <div v-if="p.last_sync_host" class="la-line la-host">{{ p.last_sync_host }}</div>
                </div>
              </div>

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
import { ref, computed, watch, onUnmounted } from 'vue';
import { invoke } from '../utils/tauri';
import { useProjects } from '../composables/useProjects';
import { useLogs } from '../composables/useLogs';
import { useSsh } from '../composables/useSsh';
import { useTerminalTabs, tabAlive } from '../composables/useTerminalTabs';
import { useAppWindow } from '../composables/useAppWindow';
import { refreshIdeAvailability } from '../composables/useProjectConfig';
import { gitRefreshKey, diffRefreshKey } from '../composables/useBackgroundRefresh';
import { refreshSettings } from '../store/refreshStore';
import { Toast, ideAvailability, iconTimestamp, isRefreshing, pokeExternalTermCounts, externalTermGlobalCount } from '../store/projectStore';
import { terminalTabs, MAX_TABS_PER_SCOPE } from '../store/terminalTabsStore';
import { projectIconSrc } from '../utils/projectIcon';
import { copyText } from '../utils/clipboard';
import { syncCheckEnabled, toggleSyncCheck } from '../store/syncCheckStore';
// R-2 write side: these run the real action on the host whether clicked on the Mac or relayed
// from a phone. They take a project id (not the object) — see src/store/remoteActions.js.
import { requestSync, requestSelectPush, setDryRun, requestRefresh, reorderProjects, requestCancelSync } from '../store/remoteActions';
import RefreshRing from './RefreshRing.vue';
import TaskCell from './TaskCell.vue';
import TerminalCell from './TerminalCell.vue';
import TerminalCountBadges from './terminal/TerminalCountBadges.vue';
import CountBadgeWrap from './CountBadgeWrap.vue';

const { projects, projectRuntime, anySyncing, isReloading, openConfig, openGitModal, createNewProject } = useProjects();
const { activeLogProjectId, toggleProjectLog } = useLogs();
const { sshHosts } = useSsh();
const { openGlobalTerminal, openProjectTerminal, openProjectRemoteTerminal: openProjectRemoteTerminalTab, openRunCommand } = useTerminalTabs();

// Global-scope mirror of TerminalCell.vue's own badge computeds (docs/plan/done/terminal-ownership-model.md
// §7 — Rule-of-Three found only two real instances, so this stays inline rather than spawning a
// TerminalButton abstraction for two call sites; `TerminalCountBadges` itself is already scope-agnostic).
const globalTabCount = computed(() => terminalTabs.value.filter((t) => t.projectId == null).length)
const globalHasExited = computed(() => terminalTabs.value.some((t) => t.projectId == null && tabAlive.value[t.id] === false))
const globalTermTitle = computed(() => {
  const lines = [
    globalTabCount.value === 0
      ? 'Global terminal (not tied to a project)'
      : `Global terminal, ${globalTabCount.value} of ${MAX_TABS_PER_SCOPE} tabs in this group`,
  ]
  if (externalTermGlobalCount.value > 0) lines.push(`${externalTermGlobalCount.value} external Terminal window(s) not standing in any listed project`)
  if (globalHasExited.value) lines.push('A shell in this group has exited')
  return lines.join('\n')
})
// `false` on a companion — see openReportHtml.
const { nativeWindow } = useAppWindow();

function handleCreateNew() {
  createNewProject(sshHosts);
}

// §3.6 — while a sync runs, the button for THAT direction is the STOP button: same button, changed
// label + colour, no new element (UI Extreme Narrow). The direction is recorded by remoteActions
// at the moment the sync is requested; without it (older runtime state, or a sync started before
// this shipped) PUSH is assumed, since it is the direction every specific-file upload takes and the
// one a mistaken `--delete` mirror is most feared on — better a STOP that is present than none.
function isStop(p, direction) {
  const rt = projectRuntime.value[p.id];
  if (!rt?.syncing) return false;
  return (rt.syncDirection || 'push') === direction;
}

// C-4 (§3.22) — an unmounted volume is NOT "not a git repo": the user's next move is to mount the
// drive, not to run `git init`, so it gets its own colour + tooltip on the SAME badge. The flag is
// produced by get_git_info (WS-2) and defaults false, so this renders exactly as before until it
// lands.
function isPathMissing(p) {
  return projectRuntime.value[p.id]?.local_path_missing === true;
}

const failedIcons = ref({});
watch([projects, iconTimestamp], () => {
  failedIcons.value = {};
});

// { [projectId]: styleObject } — COMPONENT-LOCAL on purpose. This used to live on
// `projectRuntime`, which is a mirrored store ref: every hover on the Mac broadcast a style delta
// that overwrote the phone's own popup position (two screens, two different viewports, one field),
// and the write also resurrected the runtime entry of a project that had just been removed. Where
// a popup sits on screen is per-screen presentation, so it never belongs on the wire.
const popupStyles = ref({});

// Which project's popup is open — at most one, app-wide. `openedByTap` distinguishes a pinned
// popup (tapped/clicked open) from a hover-open one, so leaving with the mouse closes the second
// but not the first.
const openPopupId = ref(null);
const openedByTap = ref(false);

// Popup is `position: fixed`, so viewport coordinates are the right frame of reference. It is centered on the TRIGGER button's own horizontal midpoint (clamped to a small viewport margin so it never crops against an edge), not on the window's midpoint - centering on the window instead of the trigger made the popup drift away from its OPEN button on anything wider than a narrow phone-sized viewport, landing it in the middle of the whole app with no visible link back to the row that opened it. The popup element is already in the DOM at `visibility: hidden` (not `display: none`) when this fires, so its real rendered width can be measured before it becomes visible.
function positionPopup(project, wrapperEl) {
  if (!wrapperEl) return;
  const rect = wrapperEl.getBoundingClientRect();
  const popupEl = wrapperEl.querySelector('.open-popup');
  const margin = 8;
  let left = rect.left;
  if (popupEl) {
    const popupWidth = popupEl.getBoundingClientRect().width || popupEl.offsetWidth || 0;
    const triggerCenter = rect.left + rect.width / 2;
    left = triggerCenter - popupWidth / 2;
    left = Math.min(Math.max(left, margin), window.innerWidth - popupWidth - margin);
  }
  popupStyles.value = {
    ...popupStyles.value,
    [project.id]: {
      position: 'fixed',
      bottom: `${window.innerHeight - rect.top}px`,
      left: `${left}px`,
      transformOrigin: 'bottom center'
    }
  };
}

function openPopup(project, wrapperEl, byTap) {
  positionPopup(project, wrapperEl);
  openPopupId.value = project.id;
  openedByTap.value = byTap;
  // Re-probe which IDEs are installed, but TTL-cached (useProjectConfig.js): this fires on HOVER
  // too, and sweeping the mouse down the OPEN column used to send one invoke per row plus one
  // mirrored-ref write per row. IDE availability changes on the timescale of an app install, not a
  // hover, so the cache makes hovering free while still catching an install within a minute.
  refreshIdeAvailability();
}

function closePopup() {
  openPopupId.value = null;
  openedByTap.value = false;
}

function onOpenHover(project, event) {
  // A tap on a touch device also fires mouseenter first; the click that follows only promotes the
  // already-open popup to "pinned" (see toggleOpenPopup), it never re-toggles it shut.
  if (openPopupId.value === project.id && openedByTap.value) return;
  openPopup(project, event?.currentTarget, false);
}

function onOpenHoverLeave(project) {
  if (openPopupId.value === project.id && !openedByTap.value) closePopup();
}

function toggleOpenPopup(project, event) {
  if (openPopupId.value === project.id && openedByTap.value) {
    closePopup();
    return;
  }
  openPopup(project, event?.currentTarget?.closest('.open-popup-wrapper'), true);
}

// Dismissal for the tap path (a phone has no "move the mouse away"). Registered only while a popup
// is open, and `pointerdown` fires before the `click` that opened it has been dispatched, so the
// opening gesture can never close it again.
function onDocPointerDown(e) {
  if (!e.target.closest?.('.open-popup-wrapper')) closePopup();
}

function onDocKeydown(e) {
  if (e.key === 'Escape') closePopup();
}

watch(openPopupId, (id) => {
  if (id) {
    document.addEventListener('pointerdown', onDocPointerDown, true);
    document.addEventListener('keydown', onDocKeydown);
  } else {
    document.removeEventListener('pointerdown', onDocPointerDown, true);
    document.removeEventListener('keydown', onDocKeydown);
  }
});

onUnmounted(() => {
  document.removeEventListener('pointerdown', onDocPointerDown, true);
  document.removeEventListener('keydown', onDocKeydown);
});

// A null `ideAvailability` means "not asked yet", which must read as UNAVAILABLE, not available:
// the old `ideAvailability && !ideAvailability.vscode` form left every IDE entry enabled until the
// first check landed, so an early click launched nothing and said nothing.
function ideMissing(name) {
  return !ideAvailability.value?.[name];
}

const PATH_MISSING_TITLE = 'Local folder missing on disk';

/** Every LOCAL popup item consumes the project's directory, so a missing volume blocks all of them
 *  (COPY excepted — it is handled separately in the template and stays enabled). */
function localBlocked(p, ide) {
  return isPathMissing(p) || (ide ? ideMissing(ide) : false);
}

function localTitle(p) {
  return isPathMissing(p) ? `${PATH_MISSING_TITLE}\n${p.local_path}` : null;
}

// --- Drag to reorder ---
const dragFromIndex = ref(null);
const isHandleMouseDown = ref(false);

function onRowDragStart(index, event) {
  if (!isHandleMouseDown.value) {
    event.preventDefault();
    return;
  }
  // Reset the mousedown flag as soon as the dragstart is validated.
  isHandleMouseDown.value = false;
  dragFromIndex.value = index;
  // WebKit/macOS only fires drop when the drag carries data, so this set is mandatory.
  event.dataTransfer.effectAllowed = 'move';
  event.dataTransfer.setData('text/plain', String(index));
}

function onRowMouseDown(event) {
  // A mousedown outside the drag handle resets the flag, so only the handle can start a drag.
  if (!event.target.closest('.project-drag-handle')) {
    isHandleMouseDown.value = false;
  }
}

function onRowDragOver(index, event) {
  if (dragFromIndex.value === null || dragFromIndex.value === index) return;

  // Swap only once the pointer has crossed the target row's midpoint. Reacting on the row's edge
  // instead produces a feedback loop: the swap moves the row back under the pointer, which
  // immediately re-triggers the swap (visible as constant jitter).
  const rect = event.currentTarget.getBoundingClientRect();
  const threshold = rect.top + rect.height / 2;
  const fromIndex = dragFromIndex.value;

  // Dragging down: only swap past the lower half of the target row.
  if (fromIndex < index && event.clientY < threshold) return;

  // Dragging up: only swap past the upper half of the target row.
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
  // The drag itself already reordered the local `projects` ref in place (onRowDragOver) for
  // instant visual feedback — that's the sanctioned optimistic mutation. The actual persist
  // (PERSIST-1) goes through the id-based, host-resolved reorderProjects action instead of a bare
  // saveProjectsList(), so a phone-initiated drag reorders the HOST's own `projects.value`, not
  // just the phone's local copy.
  reorderProjects(projects.value.map((p) => p.id));
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
    // same cold-start double-window fix as the SSH terminal.
    if (ideName === 'terminal') {
      await invoke('open_local_terminal', { localPath: path });
      // The badge is a live scan, not a tally, so nothing is incremented here — this only asks the
      // host to re-scan sooner than its next tick, once the new shell has had time to `cd`.
      pokeExternalTermCounts();
      return;
    }
    const args = IDE_LOCAL_ARGS[ideName]?.(path)
    if (args) await invoke('macos_open', { args });
  } catch (e) {
    // `macos_open` now reports a non-zero `open` (missing app, unhandled URI) instead of always
    // succeeding — surface it the same way openIdeRemote does, or the click stays silent.
    console.error(e);
    Toast.fire({ icon: 'error', title: String(e).replace('Error: ', '') });
  }
}

// DEV/BUILD launch into the in-app terminal (docs/plan/done/dev-build-in-app-launch.md, #7) — no
// invoke, no Toast, no in-flight guard: `openRunCommand` is synchronous frontend state plus a
// `pty_write`, and its own (scope, runKind) dedup is what makes a repeat click safe (focuses the
// existing DEV/BUILD tab instead of relaunching), which is a stronger guarantee than the old
// per-project in-flight Set ever gave (that guard reset the instant the invoke settled, so a
// second click a moment later still opened a second window).
function runProjectCommand(project, cmd) {
  openRunCommand(project, cmd, 'build');
}

function runProjectDev(project, cmd) {
  openRunCommand(project, cmd, 'dev');
}

// (host, path) -> absolute path. The remote $HOME never changes within a session, so a
// resolved path is stable - cache it and pay the SSH round-trip at most once per host+path.
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

  // A failure RETHROWS rather than falling back to the raw path. The fallback produced
  // `/~/project`, which every caller then embedded verbatim: `vscode://…/~/project` opens VSCode
  // pointing at a directory that does not exist, and the user gets a broken window instead of an
  // error. Rethrowing lets openIdeRemote's catch Toast the real SSH error. SSH Terminal takes the
  // same path even though a remote shell would re-expand `~` itself — an unreachable host means
  // that terminal would fail on connect anyway, so one consistent error beats a special case.
  const resolvedPath = await invoke('resolve_remote_path', { host, path });
  const full = resolvedPath.startsWith('/') ? resolvedPath : `/${resolvedPath}`;
  resolvedPathCache.set(key, full);
  return full;
}

const copiedPathKey = ref(null);

function flashCopied(key) {
  copiedPathKey.value = key;
  setTimeout(() => { if (copiedPathKey.value === key) copiedPathKey.value = null; }, 1500);
}

// utils/clipboard.js owns the non-secure-context fallback (the phone companion is plain http, where
// `navigator.clipboard` does not exist at all); this only decides what the user sees on failure.
async function copyPath(text, flashKey) {
  if (await copyText(text)) flashCopied(flashKey);
  else Toast.fire({ icon: 'error', title: 'Could not copy - select the path in the row and copy it by hand' });
}

async function copyLocalPath(project) {
  return copyPath(project.local_path, `local-${project.id}`);
}

// Copies the stored remote path verbatim - mirror copyLocalPath. `~` is a valid, portable path on
// the remote (shells/scp/rsync expand it there), so copying it needs zero network work. The old
// code awaited resolveRemoteFullPath here, which fired a blocking SSH `echo $HOME` per click
// (system.rs) and froze the UI for seconds - for an operation that is just "copy an existing field".
async function copyRemotePath(project) {
  return copyPath(project.remote_path, `remote-${project.id}`);
}

// Pulls REPORT.html from the remote first if it's newer than the local copy (or local has none),
// then opens the local file in the OS default browser - REPORT.html is a self-contained HTML/JS/CSS
// page (akihtmlreport skill output) that the app's own strict CSP would otherwise break.
async function openReportHtml(project) {
  try {
    const path = await invoke('resolve_report_html', {
      localPath: project.local_path,
      remoteHost: project.remote_host || null,
      remotePath: project.remote_path || null,
    });
    await invoke('macos_open', { args: [path] });
    // The browser window opens on the MAC - which is invisible from a phone, so the tap looked
    // like it did nothing. `nativeWindow` is useAppWindow's existing "this screen owns the real
    // window" capability (false on a companion), reused here so no `isHost` token leaks into a
    // component (ENV-1).
    if (!nativeWindow) Toast.fire({ icon: 'success', title: 'Report opened on the Mac' });
  } catch (e) {
    console.error('Failed to open REPORT.html', e);
    Toast.fire({ icon: 'error', title: String(e).replace('Error: ', '') });
  }
}

async function openProjectRemoteTerminal(project) {
  try {
    const sshCmd = await invoke('build_remote_ssh_command', { host: project.remote_host, path: project.remote_path });
    openProjectRemoteTerminalTab(project, sshCmd);
  } catch (e) {
    console.error(e);
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

// resolved = (override ?? '').trim() || stackDefault || ''. A present-but-blank override is the
// same as no override: the text field in Project Settings cannot express null once touched, so
// typing then deleting persists Some(""), and that must keep falling through to the detected stack
// default. docs/plan/done/dev-build-visibility.md
function getDevCmd(p) {
  return (p.dev_cmd_override ?? '').trim() || projectRuntime.value[p.id]?.stack_info?.dev_cmd || ''
}

function getBuildCmd(p) {
  return (p.build_cmd_override ?? '').trim() || projectRuntime.value[p.id]?.stack_info?.build_cmd || ''
}

// Tooltip is the only thing carrying WHY a run button is dead (UI Extreme Narrow: no extra label).
// Missing local folder wins over "no command" - it blocks every LOCAL item, and its wording must
// stay identical to the other blocked items'.
function runCmdTitle(p, cmd, kind) {
  return localTitle(p) || cmd || `No ${kind} command detected — set one in Project Settings`
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
  /* Extra width goes mostly to the project name/path column (2fr) instead of all of it to sync as
     before: the name and the two paths are what actually get ellipsis-truncated. Sync keeps a
     plain 1fr share rather than a fixed px so its real button cluster (PUSH/DRY/PULL + LOG + gear)
     can never be clipped by a guessed number - a bare `1fr` track is `minmax(auto, 1fr)` under the
     hood, so its floor is the cluster's own min-content width, not a guessed rem value. An earlier
     revision spelled this out as `minmax(6rem, 1fr)`, but an explicit length there REPLACES that
     auto floor instead of adding to it - once the project column above started actually competing
     for space (2fr), sync was regularly squeezed below 6rem's worth of guessed content, and the
     buttons/badges inside it visibly crowded and overlapped. */
  --grid-cols: minmax(12rem, 2fr) 2.5rem 2.5rem 2.5rem 7rem 1fr;
  --grid-gap: 0.5rem;
}

/* .grid-header and every .grid-row used to be independent `display: grid` boxes that each just happened to share the same --grid-cols value; a content-sized track (a bare `1fr` == `minmax(auto, 1fr)`, needed by the SYNC column so its PUSH/DRY/PULL/LOG/gear cluster is never guessed too small - see the f6ebc4a/a3b46709 history above) computes its auto-floor from only the min-content of items inside that ONE grid, so the header's short "SYNC" label and a row's full button cluster picked different real pixel widths for the same column - the header/body misalignment reported. Making `.projects-grid` the one real grid and every `.grid-header`/`.grid-row` a `subgrid` of it (via `display: contents` on the non-visual `.grid-body` wrapper in between) forces a single, shared auto-floor computed once across all of them, so alignment can't drift no matter what any individual row's content needs. */
.projects-grid {
  display: grid;
  grid-template-columns: var(--grid-cols);
  column-gap: var(--grid-gap);
  width: 100%;
}

.grid-header {
  display: grid;
  grid-column: 1 / -1;
  grid-template-columns: subgrid;
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

/* text-align:center only centers inline content - several cells hold block-level children
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

/* `display: contents`: the transition-group's own wrapper div carries no visual styling of its own (no border/background/hover), so removing its box is free - it just needs to stop being an extra layer between `.projects-grid` and each `.grid-row`, so every row becomes a direct subgrid child of the one real grid above instead of a grandchild the column tracks can't reach. */
.grid-body {
  display: contents;
}

.grid-row {
  display: grid;
  grid-column: 1 / -1;
  grid-template-columns: subgrid;
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

/* The TERM header label IS the global-terminal button (R1) — same colour/size as the other header
   labels, no button chrome until hover. */
/* Now the exact same `.btn-cell-trigger` (main.css) every per-project TerminalCell.vue button uses
   — same box, same badge anchoring, so the header instance cannot visually drift from the cell
   instance the way the old icon-only `.th-term-btn` did (its glyph-sized box put .cell-badge's
   -6px corner overlay squarely on top of the icon instead of outside it). Only override: sit
   centered in the header cell rather than carrying the row cell's own margin/spacing, if any. */
.th-term-btn {
  margin: 0 auto;
}

.grid-header-cell:first-child,
.grid-row-cell:first-child {
  padding-left: 6px;
  text-align: left;
}

/* New Project moved here from the app header (next to the project count) - same
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
  grid-column: 1 / -1;
  width: 100%;
}

.col-tasks,
.col-git-status,
.col-terminal,
.col-action,
.col-sync {
  padding-left: 0 !important;
  padding-right: 0 !important;
}

/* Reset widths from main.css to let CSS Grid control layout */
.col-project-info,
.col-tasks,
.col-git-status,
.col-terminal,
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
   reach the gap - RULE-ui-pattern: no styling logic stranded in inline attributes). */
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


/* Drag handle: the project icon area */
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

/* Dotted overlay on hover — the affordance that says this icon can be dragged. */
.project-drag-handle::before {
  content: '';
  position: absolute;
  inset: 0;
  /* Dim wash over the icon image. */
  background-color: rgba(0, 0, 0, 0.45);
  background-image:
    radial-gradient(circle, rgba(255, 255, 255, 0.8) 1.2px, transparent 1.2px);
  background-size: 5px 5px;
  background-position: center;
  opacity: 0;
  transition: opacity 0.15s;
  pointer-events: none;
  border-radius: 6px;
  /* Sits above the icon image. */
  z-index: 1;
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

/* Children must not take mouse events while a drag is running, or WebKit never registers the drop
   on .grid-row itself. */
.projects-grid.dragging-active .grid-row * {
  pointer-events: none;
}

/* Transition Group list styles */
.project-list-move {
  transition: transform 0.3s cubic-bezier(0.25, 0.8, 0.25, 1);
}

/* R6: LAST ACTION now lives under the sync fieldset, not its own column. Wrapper stacks the
   fieldset and the two-line action summary; the summary trims padding on .col-sync (see below)
   to buy back the extra lines' height. */
.sync-cluster {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1px;
  min-width: 0;
}

.last-action {
  font-size: 8px;
  line-height: 1.05;
  color: var(--text-darker);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}

/* Colour-only PUSH/PULL distinction - no separate badge element (Extreme Narrow). Must match the PUSH/PULL buttons themselves (main.css .btn-tech-push is orange/#FF8C00 == --accent-amber, .btn-tech-pull is blue/#0088ff, closest existing var is --accent-cyan) - these were swapped (push showed cyan, pull showed amber) so LAST ACTION read as the opposite colour of the button that just fired. */
.la-push {
  color: var(--accent-amber);
}

.la-pull {
  color: var(--accent-cyan);
}

.la-host {
  color: rgba(255, 255, 255, 0.35);
}

/* Row-height buy-back for the two new LAST ACTION lines: SYNC cell had 6px top/bottom padding
   (.grid-row-cell's default), trimmed to 3px here; .dry-group's own 2px main.css padding trimmed
   to 1px too. Net row growth measured against the ~47px project-info cell stays within ~5px. */
.grid-row-cell.col-sync {
  padding-top: 3px;
  padding-bottom: 3px;
}

.col-sync .dry-group {
  padding: 1px;
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

/* State, not `:hover` — a phone can never satisfy a hover selector, so the popup (and everything
   only reachable through it) was unusable on the companion. `.is-open` is set by hover-enter on a
   pointer device AND by a tap/click, so the Mac's behaviour is unchanged. */
.open-popup-wrapper.is-open .open-popup {
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

/* DEV/BUILD are the one disabled state whose reason is NOT self-evident from the row itself ("no
   command detected" vs. a missing folder that greys the whole LOCAL list), so their `title` has to
   survive: `pointer-events: none` above removes the element from hit-testing entirely, which also
   suppresses the native tooltip. Both click handlers no-op on the same condition, so remaining
   hit-testable cannot run anything. */
.popup-item.popup-run-btn.popup-disabled {
  pointer-events: auto;
}

.popup-item.popup-run-btn.popup-disabled:hover {
  background: none;
  color: rgba(255, 255, 255, 0.8);
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

/* STOP state (§3.6) - the SAME PUSH/PULL button while that project is syncing, just red and
   relabelled. No new element: the one control the user needs mid-panic is already under their
   cursor. Two classes + scoped attribute, so it wins over .btn-tech-push/-pull in main.css. */
.btn-tech.btn-sync-stop {
  background-color: #ef4444;
  border-color: #7f1d1d;
  color: #ffffff;
  text-shadow: 0 1px 3px rgba(0, 0, 0, 0.8);
  box-shadow: 0 0 10px rgba(239, 68, 68, 0.5);
}

.btn-tech.btn-sync-stop:hover:not(:disabled) {
  background-color: #f87171;
  box-shadow: 0 0 12px rgba(239, 68, 68, 0.75);
}

/* C-4 (§3.22) - "local path not found" on the EXISTING git badge. Deliberately NOT the greyed-out
   .git-no-repo look: the two states demand different actions (mount the drive vs. git init), so
   they must not read the same. Amber = something is wrong and it is not git's fault. */
.btn-action-git.git-path-missing {
  filter: none;
  background: linear-gradient(135deg, #b45309, #78350f);
  border-color: rgba(245, 158, 11, 0.9);
  box-shadow: 0 0 6px rgba(245, 158, 11, 0.6);
}

/* DIVERGED state - orange outline only, zero extra space */
.dry-group.is-diverged {
  outline: 1px solid rgba(251, 146, 60, 0.5);
  border-radius: 6px;
}

.btn-sync-diverged {
  box-shadow: 0 0 0 1px rgba(251, 146, 60, 0.6) !important;
}

/* Narrow mode (<=700px) - single shared breakpoint for the whole app (see
   docs/plan/done/narrow-mode-and-ux-1.14.0.md, "Shared contract"). Label hiding uses the global
   .u-narrow-hide / .u-wide-hide utilities from main.css; this block only covers layout that a
   utility class can't express - column widths, gaps. */
@media (max-width: 700px) {
  .projects-table-container {
    /* Project name column: 12rem -> 6.5rem (widened back up from an initial 4.8rem/40% guess  - 
       that was too tight to show any of the remote path, this leaves a few characters visible).
       GIT column: 2.5rem -> 1.7rem read as too tight against LAST, opened back up to 2.1rem.
       TASKS column trimmed a touch (2.5rem -> 2.1rem) - it's just an icon+badge, was carrying
       more blank space than it needed. Action column (OPEN + select-push) also narrows since
       OPEN's label now hides at the same 700px breakpoint via u-narrow-hide.
       Project column floor raised 6.5rem -> 7.5rem, and it now carries the flex weight (2fr) so a
       wider phone/window spends the extra width on the name+paths, not on the sync cluster.
       Sync track is a bare 1fr, not minmax(4.5rem, 1fr), for the identical reason the wide-mode
       track above dropped its own fixed minimum: 4.5rem (72px) is smaller than this cluster's own
       min-content even icon-only (PUSH ~34px + DRY ~21px + PULL ~34px + two 3px gaps + 8px padding
       + 4px margin = ~107px for the fieldset alone, plus LOG ~29px and the gear button's fixed 28px
       plus two 6px gaps from actions-wrapper = ~176px total) - a fixed length there overrides the
       auto/min-content floor a bare 1fr gets for free, so the fixed-length version was shrinking
       the whole cluster below its own icons and crushing PUSH/DRY/PULL/LOG/gear into each other. */
    --grid-cols: minmax(7.5rem, 2fr) 2.1rem 1.9rem 2.5rem 4.2rem 1fr;
    --grid-gap: 0.4rem;
  }

  .project-info-row {
    gap: 6px;
  }

  /* The project name/path block had unused padding trailing short names - tighten it so
     ellipsis-truncated paths get a couple more characters of room instead of dead space. */
  .project-text-col {
    padding-right: 0;
  }

  /* PUSH/PULL lose their text label at this width - match them to the OPEN button's icon-only
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
     border) ended up colliding with the row below it - nothing anchored to a row's own box can
     overlap outside it without fighting that next row's own positioned content for paint order.
     Kept in normal flow instead: small, but a real flex item between PUSH and PULL, so it can
     never visually merge with anything else. */
  .dry-group {
    margin: 0 2px;
    gap: 3px;
    padding: 1px 4px;
    overflow: visible;
  }

  .dry-group-left,
  .dry-group-right {
    padding: 0;
  }

  /* The DRY toggle is squeezed hard here so PUSH/PULL's count badges (CountBadgeWrap, a 6px
     overhang past each button's top-right corner — main.css .cell-badge) have room to sit without
     overlapping the "DRY" text. */
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
     The base .switch-sm rule (main.css) uses bottom: 2px, sized for the 12px track - left
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

  /* GIT sits noticeably closer to TERM than the gap elsewhere reads as needing - pull TERM left
     a touch rather than shrinking --grid-gap globally (that would also tighten TERM↔ACTION and
     ACTION↔SYNC, which need the opposite). */
  .col-terminal {
    margin-left: -6px;
  }

  /* LAST ACTION's two lines get even tighter at this width - re-homed here from the old
     .last-sync-badge/.sync-time/.sync-host rules (that column no longer exists). */
  .last-action {
    font-size: 7px;
  }
}
</style>
