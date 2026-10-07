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
          <TerminalScopeButton :scope="GLOBAL_SCOPE" />
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
          <div class="skeleton-zone-wrap">
            <div class="skeleton-zone-col">
              <div v-for="i in 3" :key="i" class="skeleton-zone-item">
                <div class="skeleton-box skeleton-box-icon"></div>
                <div class="skeleton-zone-text">
                  <div class="skeleton-box skeleton-box-name"></div>
                  <div class="skeleton-box skeleton-box-path"></div>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- Empty State with Add Project CTA -->
        <div v-else-if="projects.length === 0" class="grid-row-special empty-state" key="empty">
          <div class="empty-state-wrap">
            <i class="fa-solid fa-folder-open mb-2 empty-state-icon"></i>
            <div class="empty-state-text">No projects found. Add one to get started.</div>
            <button class="btn-tech btn-tech-primary btn-empty-add mt-2" @click="handleCreateNew">
              <i class="fa-solid fa-plus mr-1"></i> Add Project
            </button>
          </div>
        </div>

        <!-- Project Rows -->
        <div
             v-for="(p, index) in projects"
             :key="p.id"
             class="grid-row"
             :class="{ 'row-syncing': projectRuntime[p.id]?.syncing, 'row-dragging': dragFromIndex === index, 'row-disabled': p.disabled }"
             draggable="true"
             @dragstart="onRowDragStart(index, $event)"
             @dragover.prevent="onRowDragOver(index, $event)"
             @dragenter.prevent
             @drop.prevent="onRowDrop(index)"
             @dragend="onRowDragEnd"
             @mousedown="onRowMouseDown"
             :title="p.disabled ? 'Disabled - background sync/git checks are skipped for this project' : ''">
          <!-- Cell 1: Project Info -->
          <div class="grid-row-cell col-project-info">
            <div class="project-info-row">
              <div class="project-icon-col">
                <!-- Project Icon (drag handle) -->
                <div
                     class="project-drag-handle icon-glow"
                     title="Drag to reorder"
                     @mousedown="isHandleMouseDown = true">
                  <img v-if="!failedIcons[p.id] && projectIconSrc(p.id, iconTimestamp)" :src="projectIconSrc(p.id, iconTimestamp)" class="project-drag-img" draggable="false" @error="failedIcons[p.id] = true" />
                  <i v-else class="fa-solid fa-folder-open text-cyan project-drag-icon-fallback"></i>
                </div>
                <i class="fa-solid fa-power-off src-power" :class="[p.disabled ? 'is-off' : 'is-on', { 'is-locked': projectRuntime[p.id]?.syncing }]"
                   :title="p.disabled ? 'Background sync/git checks OFF - click to enable' : 'Background sync/git checks ON - click to disable'"
                   @click.stop="!projectRuntime[p.id]?.syncing && setProjectDisabled(p.id, !p.disabled)"></i>
              </div>

              <div class="project-text-col">
                <div class="project-name">
                  <span class="project-name-label">{{ projectDisplayName(p) }}</span>
                  <a v-if="p.production_url" href="#" @click.prevent="openUrl(p.production_url)" title="Open Production Site" class="project-prod-link">
                    <i class="fa-solid fa-globe"></i><i class="fa-solid fa-arrow-up-right-from-square project-prod-icon"></i>
                  </a>
                </div>
                <div class="project-paths">
                  <!-- u-select-text: allow manual copying of project paths. -->
                  <span class="path-local u-select-text" :title="p.local_path"><i class="fa-solid text-cyan mr-1" :class="copiedPathKey === `local-${p.id}` ? 'fa-check' : 'fa-laptop-code'" :title="copiedPathKey === `local-${p.id}` ? 'Copied' : 'Copy local path'" @click.stop="copyLocalPath(p)"></i> {{ p.local_path }}</span>
                  <span v-if="p.remote_host" class="path-remote" :title="`${p.remote_host}:${p.remote_path}`">
                    <i class="fa-solid text-amber mr-1" :class="copiedPathKey === `remote-${p.id}` ? 'fa-check' : 'fa-cloud'" :title="copiedPathKey === `remote-${p.id}` ? 'Copied' : 'Copy remote path'" @click.stop="copyRemotePath(p)"></i><select
                      class="host-select-mini host-select-mini--wide"
                      :value="p.remote_host"
                      @change="setRemoteHost(p.id, $event.target.value)"
                      @click.stop
                      :title="p.remote_host">
                      <option v-for="h in hostOptionsFor(p)" :key="h" :value="h">{{ h }}</option>
                    </select><span class="u-select-text">:{{ p.remote_path }}</span>
                  </span>
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
                        class="btn-cell-trigger btn-action-git"
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
            <TerminalScopeButton :scope="p" />
          </div>

          <!-- Cell 5: Action (OPEN + SELECT-push only) -->
          <div class="grid-row-cell col-action">
            <div class="actions-wrapper">
              <!-- Open Popup Trigger (OPEN Button) -->
              <div class="open-popup-wrapper">
                <button class="btn-tech btn-tech-primary btn-action-open" title="Open Popup" :popovertarget="`open-popup-${p.id}`" :style="`anchor-name: --open-anchor-${p.id}`">
                  <span class="btn-text u-narrow-hide">OPEN</span> <i class="fa-solid fa-caret-up"></i>
                </button>

                <div class="open-popup pin-left" popover :id="`open-popup-${p.id}`" :style="`position-anchor: --open-anchor-${p.id}`" @beforetoggle="onPopupOpen" @click="closeOnAction">
                  <div class="popup-header popup-header-wrap" :title="projectDisplayName(p)">
                     <img v-if="!failedIcons[p.id] && projectIconSrc(p.id, iconTimestamp)" :src="projectIconSrc(p.id, iconTimestamp)" class="popup-project-icon" alt="" @error="failedIcons[p.id] = true" />
                     <i v-else class="fa-solid fa-folder-open text-cyan mr-1 popup-icon-folder-fallback"></i>
                     <span class="popup-title-text">{{ projectDisplayName(p) }}</span>
                     <button class="popup-copy-btn" @click.stop="openReportHtml(p)" title="Open REPORT.html (pulls newer copy from remote first if needed)">
                       <i class="fa-solid fa-file-lines"></i> REPORT
                     </button>
                     <button v-if="p.production_url" class="popup-copy-btn" @click.stop="openSearchConsole(p)" title="Open Google Search Console for this domain">
                       <i class="fa-solid fa-magnifying-glass"></i> GSC
                     </button>
                   </div>
                   <div class="popup-columns">
                     <!-- LOCAL -->
                     <div class="popup-col-local">
                       <div class="popup-section-label">
                         <span>💻 LOCAL</span>
                         <button class="popup-copy-btn" @click.stop="copyLocalPath(p)" :title="copiedPathKey === `local-${p.id}` ? 'Copied!' : 'Copy full path'">
                           <i class="fa-solid" :class="copiedPathKey === `local-${p.id}` ? 'fa-check' : 'fa-copy'"></i> COPY
                         </button>
                       </div>
                       <!-- Popup items disabled when local path is missing or IDE unavailable. -->
                       <div class="popup-item" :class="{ 'popup-disabled': localBlocked(p) }" :title="localTitle(p)" @click="openIdeLocal('finder', p.local_path)">
                         <i class="fa-solid fa-folder-open popup-item-icon popup-icon-amber"></i> Finder
                       </div>
                       <!-- In-app terminal option first for phone companion support. -->
                       <div class="popup-item" :class="{ 'popup-disabled': localBlocked(p) }" :title="localTitle(p)" @click="openNewProjectTerminal(p)">
                         <i class="fa-solid fa-terminal popup-item-icon popup-icon-cyan"></i> In-App Terminal
                       </div>
                       <div class="popup-item" :class="{ 'popup-disabled': localBlocked(p) }" :title="localTitle(p)" @click="openIdeLocal('terminal', p.local_path, p.id)">
                         <i class="fa-solid fa-terminal popup-item-icon"></i> Terminal
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
                       <div class="popup-item" :class="{ 'popup-disabled': localBlocked(p, 'cursor') }" :title="localTitle(p)" @click="openIdeLocal('cursor', p.local_path)">
                         <i class="fa-solid fa-code popup-item-icon"></i> Cursor
                       </div>
                       <!-- DEV/BUILD buttons always rendered, disabled when command unconfigured. -->
                       <div class="popup-run-row">
                         <div class="popup-item popup-run-btn" :class="{ 'popup-disabled': localBlocked(p) || !getDevCmd(p) }" @click="!localBlocked(p) && getDevCmd(p) && runProjectDev(p, getDevCmd(p))" :title="runCmdTitle(p, getDevCmd(p), 'dev')">
                           <i class="fa-solid fa-terminal popup-item-icon popup-icon-green"></i> DEV
                         </div>
                         <div class="popup-item popup-run-btn" :class="{ 'popup-disabled': localBlocked(p) || !getBuildCmd(p) }" @click="!localBlocked(p) && getBuildCmd(p) && runProjectCommand(p, getBuildCmd(p))" :title="runCmdTitle(p, getBuildCmd(p), 'build')">
                           <i class="fa-solid fa-hammer popup-item-icon popup-icon-amber"></i> BUILD
                         </div>
                         <div class="popup-item popup-run-btn" :class="{ 'popup-disabled': localBlocked(p) || !resolveDeployCmd(p) || !!deployBlockedReason(p) }" @click="!localBlocked(p) && resolveDeployCmd(p) && !deployBlockedReason(p) && runProjectDeploy(p)" :title="deployCmdTitle(p)">
                           <i class="fa-solid fa-rocket popup-item-icon popup-icon-cyan"></i> DEPLOY
                         </div>
                       </div>
                     </div>

                     <!-- REMOTE -->
                     <!-- Remote items always accessible, only upload action is gated by sync switch. -->
                     <div v-if="p.remote_host && p.remote_path" class="popup-col-remote">
                       <div class="popup-section-label">
                         <span>☁️ REMOTE (SSH)</span>
                         <button class="popup-copy-btn" @click.stop="copyRemotePath(p)" :title="copiedPathKey === `remote-${p.id}` ? 'Copied!' : 'Copy full path'">
                           <i class="fa-solid" :class="copiedPathKey === `remote-${p.id}` ? 'fa-check' : 'fa-copy'"></i> COPY
                         </button>
                       </div>
                       <!-- In-app SSH terminal first for companion support. -->
                       <div class="popup-item" @click="openProjectRemoteTerminal(p)">
                         <i class="fa-solid fa-terminal popup-item-icon popup-icon-cyan"></i> SSH Terminal (In-App)
                       </div>
                       <div class="popup-item" @click="openIdeRemote('terminal', p.remote_host, p.remote_path, p.id)">
                         <i class="fa-solid fa-terminal popup-item-icon"></i> SSH Terminal
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
                       <div class="popup-item" :class="{ 'popup-disabled': ideMissing('cursor') }" @click="openIdeRemote('cursor', p.remote_host, p.remote_path)">
                         <i class="fa-solid fa-code popup-item-icon"></i> Cursor (Remote SSH)
                       </div>
                       <div class="popup-item"
                            :class="{ 'popup-disabled': projectRuntime[p.id]?.syncing || !syncCheckEnabled }"
                            @click="!projectRuntime[p.id]?.syncing && syncCheckEnabled && requestSelectPush(p.id)"
                            :title="!syncCheckEnabled ? 'Sync check is off - turn it on (power icon in the SYNC column header) to push files' : 'Pick specific files/folders (native file picker) and push only those to Remote - bypasses this project\'s exclude list, unaffected by the DRY toggle'">
                         <i class="fa-solid fa-upload popup-item-icon popup-icon-cyan"></i> Upload (select files)
                       </div>
                     </div>
                   </div>
                 </div>
              </div>

              <button class="btn-tech btn-tech-secondary btn-refresh-project"
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
                <!-- Sync check off disables each sync control itself, not the whole fieldset: a disabled fieldset would also disable the read-only count/conflict badges and their popup. -->
                <fieldset class="remote-actions-fieldset" :title="!syncCheckEnabled ? 'Sync check is off' : ''">
                  <div class="dry-group" :class="[p.dry_run ? 'is-safe' : 'is-danger', projectRuntime[p.id]?.hasPendingPush && projectRuntime[p.id]?.hasPendingPull ? 'is-diverged' : '']">
                    <div class="dry-group-left">
                      <CountBadgeWrap :count="projectRuntime[p.id]?.pushCount || 0"
                                       :count-title="pushTooltip(p)"
                                       :popover-id="`conflict-popup-${p.id}`"
                                       :anchor-name="`--conflict-anchor-${p.id}`"
                                       :delete-armed="p.delete_on_push && !isStop(p, 'push')"
                                       delete-side="left"
                                       delete-title="Mirror: files on the remote that are not here will be deleted."
                                       :conflict-count="conflictCount(p)"
                                       :conflict-title="conflictTooltip(p)"
                                       :deploy-on-push="deployOffersOnPush(p)"
                                       :deploy-title="deployOnPushTooltip(p)"
                                       :has-hooks="hasSyncHooks(p)"
                                       :hooks-title="hooksTooltip(p)">
                        <button
                                class="btn-tech btn-tech-push"
                                :class="{
                                  'btn-sync-clean': !isStop(p, 'push') && projectRuntime[p.id]?.hasPendingPush === false,
                                  'btn-sync-checking': !isStop(p, 'push') && projectRuntime[p.id]?.hasPendingPush === null,
                                  'btn-sync-diverged': !isStop(p, 'push') && projectRuntime[p.id]?.hasPendingPush && projectRuntime[p.id]?.hasPendingPull,
                                  'btn-sync-stop': isStop(p, 'push'),
                                  'btn-sync-stale': !!staleNote(p, 'push')
                                }"
                                :disabled="!syncCheckEnabled || (projectRuntime[p.id]?.syncing && !isStop(p, 'push')) || (!isStop(p, 'push') && configBlocked(p))"
                                @click="isStop(p, 'push') ? requestCancelSync(p.id) : requestSync(p.id, 'push')"
                                :title="staleNote(p, 'push') + (isStop(p, 'push') ? 'Stop this sync now (kills rsync/ssh)' : configBlocked(p) ? CONFIG_NOT_READY_TITLE : !syncCheckEnabled ? 'Sync check is off' : projectRuntime[p.id]?.pushCount > 0 ? `Push Local → Remote (${projectRuntime[p.id].pushCount} file(s))${conflictCount(p) ? ' — ' + conflictTooltip(p) : ''}` : 'Push Local to Remote')">
                          <i class="fa-solid" :class="isStop(p, 'push') ? 'fa-stop' : 'fa-cloud-arrow-up'"></i> <span class="btn-text u-narrow-hide">{{ isStop(p, 'push') ? 'STOP' : 'PUSH' }}</span>
                        </button>
                      </CountBadgeWrap>

                      <!-- Read-only breakdown popover shared by every lit badge; native Popover API + CSS anchor positioning, same mechanism as .open-popup above. -->
                      <div v-if="anyBadgeLit(p)" class="conflict-popup" popover :id="`conflict-popup-${p.id}`" :style="`position-anchor: --conflict-anchor-${p.id}`" @toggle="onConflictPopoverToggle($event, p)">
                        <div class="conflict-popup-header">Sync Changes</div>
                        <div class="conflict-popup-sides">
                          <div v-for="side in SYNC_SIDES" :key="side.key" class="conflict-popup-side" :class="`is-${side.key}`">
                            <div class="conflict-popup-side-head"><i class="fa-solid" :class="side.icon"></i> {{ side.label }} <b>{{ projectRuntime[p.id]?.[side.countKey] || 0 }}</b></div>
                            <div class="conflict-popup-side-sub">{{ side.sub }}</div>
                            <div class="conflict-popup-rows">
                              <div v-for="[dir, n] in sideDirRows(p, side.key)" :key="dir" class="conflict-popup-row" :title="dir">
                                <span class="conflict-row-path">{{ dir }}</span>
                                <span class="conflict-row-meta">{{ n }}</span>
                              </div>
                              <div v-if="!sideDirRows(p, side.key).length" class="conflict-row-empty">nothing</div>
                            </div>
                            <div v-if="side.key === 'push' && sidePushNote(p)" class="conflict-popup-side-note">{{ sidePushNote(p) }}</div>
                          </div>
                        </div>
                        <div v-if="conflictCount(p)" class="conflict-popup-conflicts">
                          <div class="conflict-popup-conflicts-head">⚠ {{ conflictCount(p) }} conflict{{ conflictCount(p) === 1 ? '' : 's' }} - left out of both</div>
                          <div class="conflict-popup-rows">
                            <div v-for="c in projectRuntime[p.id]?.conflicts || []" :key="c.path" class="conflict-popup-row" :title="c.path">
                              <span class="conflict-row-path">{{ c.path }}</span>
                              <span class="conflict-row-meta">{{ c.local_size }}B vs {{ c.remote_size }}B{{ c.verified === false ? ' · unverified' : '' }}</span>
                            </div>
                          </div>
                        </div>
                        <div class="conflict-explain-row">
                          <button class="btn-tech btn-tech-secondary conflict-explain-btn"
                                  :disabled="explainDisabled(p)"
                                  :title="explainDisabledTitle(p)"
                                  @click="requestExplain(p)">
                            <i class="fa-solid fa-wand-magic-sparkles"></i> {{ explainState[p.id]?.loading ? 'Preparing…' : 'Explain' }}
                          </button>
                          <button class="btn-tech btn-tech-secondary" title="AI Settings - model and prompt" @click="showAiSettings = true">
                            <i class="fa-solid fa-sliders"></i>
                          </button>
                        </div>
                        <div v-if="explainState[p.id]?.error" class="conflict-explain-error">
                          {{ explainAgyMissing(p) ? 'agy is not installed on this Mac' : explainState[p.id].error }}
                        </div>
                      </div>
                    </div>

                    <div class="dry-toggle-center" title="Toggle Dry Run">
                      <span class="dry-label">DRY</span>
                      <label class="switch switch-sm" :class="{ 'is-sync-off': !syncCheckEnabled }">
                        <!-- DRY toggle change dispatches setDryRun to host. -->
                        <input type="checkbox" :checked="p.dry_run" :disabled="!syncCheckEnabled || projectRuntime[p.id]?.syncing" @change="setDryRun(p.id, $event.target.checked)" />
                        <span class="slider"></span>
                      </label>
                    </div>

                    <div class="dry-group-right">
                      <CountBadgeWrap :count="projectRuntime[p.id]?.pullCount || 0"
                                       :count-title="pullTooltip(p)"
                                       :popover-id="`conflict-popup-${p.id}`"
                                       :delete-armed="p.delete_on_pull && !isStop(p, 'pull')"
                                       delete-title="Mirror: files here that are not on the remote will be deleted.">
                        <button
                                class="btn-tech btn-tech-pull"
                                :class="{
                                  'btn-sync-clean': !isStop(p, 'pull') && projectRuntime[p.id]?.hasPendingPull === false,
                                  'btn-sync-checking': !isStop(p, 'pull') && projectRuntime[p.id]?.hasPendingPull === null,
                                  'btn-sync-diverged': !isStop(p, 'pull') && projectRuntime[p.id]?.hasPendingPush && projectRuntime[p.id]?.hasPendingPull,
                                  'btn-sync-stop': isStop(p, 'pull'),
                                  'btn-sync-stale': !!staleNote(p, 'pull')
                                }"
                                :disabled="!syncCheckEnabled || (projectRuntime[p.id]?.syncing && !isStop(p, 'pull')) || (!isStop(p, 'pull') && configBlocked(p))"
                                @click="isStop(p, 'pull') ? requestCancelSync(p.id) : requestSync(p.id, 'pull')"
                                :title="staleNote(p, 'pull') + (isStop(p, 'pull') ? 'Stop this sync now (kills rsync/ssh)' : configBlocked(p) ? CONFIG_NOT_READY_TITLE : !syncCheckEnabled ? 'Sync check is off' : projectRuntime[p.id]?.pullCount > 0 ? `Pull Remote → Local (${projectRuntime[p.id].pullCount} file(s))` : 'Pull Remote to Local')">
                          <i class="fa-solid" :class="isStop(p, 'pull') ? 'fa-stop' : 'fa-cloud-arrow-down'"></i> <span class="btn-text u-narrow-hide">{{ isStop(p, 'pull') ? 'STOP' : 'PULL' }}</span>
                        </button>
                      </CountBadgeWrap>
                    </div>
                  </div>
                </fieldset>

                <!-- LAST ACTION status display lines. -->
                <div v-if="p.last_sync_action" class="last-action" :title="`${p.last_sync_action} — ${p.last_sync_host || ''}`">
                  <div class="la-line"><span :class="p.last_sync_action.includes('PULL') ? 'la-pull' : 'la-push'">{{ p.last_sync_action }}</span> {{ formatTimeAgo(p.last_sync_time) }}</div>
                  <div v-if="p.last_sync_host" class="la-line la-host">{{ p.last_sync_host }}</div>
                </div>
              </div>

              <button class="btn-tech btn-tech-secondary btn-log-trigger" :class="{ 'log-active': activeLogProjectId === p.id }" @click="toggleProjectLog(p.id)" title="View Project Log">
                <i class="fa-solid fa-file-lines btn-log-icon-only"></i>
                <span class="btn-text u-narrow-hide">LOG</span>
              </button>

              <button class="btn-cell-trigger" @click="openConfig(p)" :disabled="projectRuntime[p.id]?.syncing" title="Edit Configuration" aria-label="Edit Configuration">
                <i class="fa-solid fa-gear"></i>
              </button>
            </div>
          </div>
        </div>
      </transition-group>
    </div>
    <AiSettingsModal :show="showAiSettings" @close="showAiSettings = false" />
  </div>
</template>

<script setup>
import { ref, watch } from 'vue';
import { invoke } from '../utils/tauri';
import { effectiveAgyModel, effectiveExplainPrompt } from '../store/aiSettingsStore';
import AiSettingsModal from './modals/AiSettingsModal.vue';
import { useProjects } from '../composables/useProjects';
import { useLogs } from '../composables/useLogs';
import { useSsh } from '../composables/useSsh';
import { useTerminalTabs } from '../composables/useTerminalTabs';
import { useAppWindow } from '../composables/useAppWindow';
import { refreshIdeAvailability, projectDisplayName } from '../composables/useProjectConfig';
import { gitRefreshKey, diffRefreshKey } from '../composables/useBackgroundRefresh';
import { refreshSettings } from '../store/refreshStore';
import { scheduleExternalTermRescan } from '../composables/useExternalTerminals';
import { Toast, ideAvailability, iconTimestamp, isRefreshing, registerExternalTerminalLaunch } from '../store/projectStore';
import { GLOBAL_SCOPE } from '../store/terminalTabsStore';
import { projectIconSrc } from '../utils/projectIcon';
import { copyText } from '../utils/clipboard';
import { syncCheckEnabled, toggleSyncCheck } from '../store/syncCheckStore';
import { getProjectConfigEntry } from '../store/projectConfigStore';
// R-2 write side: these run the real action on the host whether clicked on the Mac or relayed from a phone. They take a project id (not the object) — see src/store/remoteActions.js.
import { requestSync, requestSelectPush, setDryRun, setRemoteHost, requestRefresh, reorderProjects, requestCancelSync } from '../store/remoteActions';
import { confirmAndDeploy, resolveDeployCmd } from '../composables/useDeploy';
import { deployBlockedReason, deployTargetLabel, shouldOfferDeployAfterPush } from '../composables/projectConfigPure';
import RefreshRing from './RefreshRing.vue';
import TaskCell from './TaskCell.vue';
import TerminalScopeButton from './TerminalScopeButton.vue';
import CountBadgeWrap from './CountBadgeWrap.vue';

const { projects, projectRuntime, anySyncing, isReloading, setProjectDisabled, openConfig, openGitModal, createNewProject } = useProjects();
const { activeLogProjectId, toggleProjectLog } = useLogs();
const { sshHosts } = useSsh();
const { openNewProjectTerminal, openProjectRemoteTerminal: openProjectRemoteTerminalTab, openExplainTerminal, openRunCommand } = useTerminalTabs();

// `false` on a companion — see openReportHtml.
const { nativeWindow } = useAppWindow();

function handleCreateNew() {
  createNewProject(sshHosts);
}

// Retain currently stored host even if missing from SSH config.
function hostOptionsFor(p) {
  return p.remote_host && !sshHosts.value.includes(p.remote_host)
    ? [p.remote_host, ...sshHosts.value]
    : sshHosts.value;
}

// During active sync, the active direction button becomes the STOP button.
function isStop(p, direction) {
  const rt = projectRuntime.value[p.id];
  if (!rt?.syncing) return false;
  return (rt.syncDirection || 'push') === direction;
}

// Check if project local path is missing on disk.
function isPathMissing(p) {
  return projectRuntime.value[p.id]?.local_path_missing === true;
}

// Badge/popover breakdown (docs/plan/done/conflict-detection-and-agy-report.md §4) — read-only, never an action.
// Reachable from any lit badge (push count, pull count or conflict), not only when conflicts > 0.
function conflictCount(p) {
  return projectRuntime.value[p.id]?.conflicts?.length || 0;
}

function anyBadgeLit(p) {
  const rt = projectRuntime.value[p.id];
  return !!(rt && (rt.pushCount > 0 || rt.pullCount > 0 || conflictCount(p) > 0));
}

// Last status check failed: counts on the buttons are from the last good check, so say so instead of letting them pose as current.
function staleNote(p, dir) {
  const err = projectRuntime.value[p.id]?.syncCheckError;
  return err && syncCheckEnabled.value && !isStop(p, dir) ? `Status check failed - counts are from the last good check: ${err}\n\n` : '';
}

// Push-direction tooltip: local edits pending push, split from .git and from remote-behind noise so it never lumps push (local-side) and pull (remote-side) together as one "local edits" figure.
function pushTooltip(p) {
  const rt = projectRuntime.value[p.id];
  if (!rt) return '';
  const parts = [];
  if (rt.pushCount) parts.push(`${rt.pushCount} to push`);
  if (rt.gitCount) parts.push(`${rt.gitCount} .git`);
  if (rt.remoteBehind) parts.push('remote is behind');
  const n = conflictCount(p);
  if (n) parts.push(`${n} conflict${n === 1 ? '' : 's'} excluded`);
  return parts.join(' · ');
}

// Pull-direction tooltip: remote edits pending pull - remote-side, so never folded into "local edits".
function pullTooltip(p) {
  const rt = projectRuntime.value[p.id];
  if (!rt) return '';
  const parts = [];
  if (rt.pullCount) parts.push(`${rt.pullCount} to pull`);
  const n = conflictCount(p);
  if (n) parts.push(`${n} conflict${n === 1 ? '' : 's'} excluded`);
  return parts.join(' · ');
}

// One-line summary of everything behind the badges, for the conflict badge tooltip.
function conflictTooltip(p) {
  const rt = projectRuntime.value[p.id];
  if (!rt) return '';
  const parts = [];
  if (rt.pushCount) parts.push(`${rt.pushCount} to push`);
  if (rt.pullCount) parts.push(`${rt.pullCount} to pull`);
  if (rt.gitCount) parts.push(`${rt.gitCount} .git`);
  if (rt.remoteBehind) parts.push('remote is behind');
  const n = conflictCount(p);
  if (n) parts.push(`${n} conflict${n === 1 ? '' : 's'} excluded`);
  return parts.join(' · ');
}

// The popup's two columns: push = local-side edits, pull = remote-side edits (colours come from --color-local / --color-remote).
const SYNC_SIDES = [
  { key: 'push', label: 'PUSH', sub: 'Local → Remote', icon: 'fa-cloud-arrow-up', countKey: 'pushCount' },
  { key: 'pull', label: 'PULL', sub: 'Remote → Local', icon: 'fa-cloud-arrow-down', countKey: 'pullCount' },
];

// Top-level directories carrying changes for one side, biggest first: [[dirName, count]].
function sideDirRows(p, side) {
  const byTopDir = projectRuntime.value[p.id]?.byTopDir || {};
  return Object.entries(byTopDir)
    .map(([dir, c]) => [dir, c[side] || 0])
    .filter(([, n]) => n > 0)
    .sort((a, b) => b[1] - a[1]);
}

// Push-side extras that are not file rows: .git churn and a remote that is behind.
function sidePushNote(p) {
  const rt = projectRuntime.value[p.id];
  return [rt?.gitCount && `+ ${rt.gitCount} .git`, rt?.remoteBehind && 'remote is behind'].filter(Boolean).join(' · ');
}

// Per-project Explain state, keyed by project id - never persisted, cleared on popover close is not needed (read-only, re-requestable).
const explainState = ref({});
const showAiSettings = ref(false);
// Cheap pre-flight check: populated once per project on first popover open, so Explain is disabled with its reason BEFORE any click, not only after a failed invoke.
const agyAvailable = ref({});

async function ensureAgyAvailabilityChecked(p) {
  if (agyAvailable.value[p.id] !== undefined) return;
  try {
    agyAvailable.value = { ...agyAvailable.value, [p.id]: await invoke('check_agy_available') };
  } catch (_) {
    agyAvailable.value = { ...agyAvailable.value, [p.id]: false };
  }
}

function explainDisabled(p) {
  return !syncCheckEnabled.value || explainState.value[p.id]?.loading || agyAvailable.value[p.id] === false;
}

function explainDisabledTitle(p) {
  if (!syncCheckEnabled.value) return 'Sync check is off';
  if (agyAvailable.value[p.id] === false) return 'agy is not installed on this Mac';
  return '';
}

function onConflictPopoverToggle(e, p) {
  if (e.newState !== 'open') return;
  ensureAgyAvailabilityChecked(p);
}

function explainAgyMissing(p) {
  return explainState.value[p.id]?.error === 'agy-missing';
}

async function requestExplain(p) {
  explainState.value = { ...explainState.value, [p.id]: { loading: true } };
  try {
    const status = {
      has_local_changes: projectRuntime.value[p.id]?.hasPendingPush || false,
      has_remote_changes: projectRuntime.value[p.id]?.hasPendingPull || false,
      push_count: projectRuntime.value[p.id]?.pushCount || 0,
      pull_count: projectRuntime.value[p.id]?.pullCount || 0,
      conflicts: projectRuntime.value[p.id]?.conflicts || [],
      git_count: projectRuntime.value[p.id]?.gitCount || 0,
      remote_behind: projectRuntime.value[p.id]?.remoteBehind || false,
      by_top_dir: projectRuntime.value[p.id]?.byTopDir || {},
    };
    const agyCmd = await invoke('explain_sync_status', { project: p, status, model: effectiveAgyModel(), prompt: effectiveExplainPrompt() });
    explainState.value = { ...explainState.value, [p.id]: { loading: false } };
    document.getElementById(`conflict-popup-${p.id}`)?.hidePopover();
    openExplainTerminal(p, agyCmd);
  } catch (e) {
    const msg = String(e);
    explainState.value = {
      ...explainState.value,
      [p.id]: { loading: false, error: msg.includes('agy is not installed') ? 'agy-missing' : msg },
    };
  }
}

const failedIcons = ref({});
watch([projects, iconTimestamp], () => {
  failedIcons.value = {};
});

// Positioning is CSS Anchor Positioning (main.css .open-popup), not JS - this only handles the side effect of opening.
function onPopupOpen(e) {
  if (e.newState !== 'open') return;
  // TTL-cached IDE availability refresh on popup open.
  refreshIdeAvailability();
}

/** Picking an action dismisses the menu; COPY/REPORT buttons stop propagation, so they leave it open. */
function closeOnAction(e) {
  if (e.target.closest('.popup-item')) e.currentTarget.hidePopover();
}

// Check if IDE availability probe is unready or missing.
function ideMissing(name) {
  return !ideAvailability.value?.[name];
}

const PATH_MISSING_TITLE = 'Local folder missing on disk';
const CONFIG_NOT_READY_TITLE = 'Project settings (.akidevsync/project.json) are not ready - fix before syncing';

// Item 2 (docs/plan/done/settings-and-state-layout.md): PUSH/PULL/status-check must never run against a
// project whose project.json is not the authoritative 'ok' (missing with nothing to seed, corrupt,
// unavailable) - the excludes on `p` may be an empty or stale registry leftover otherwise. The actual
// guard lives in `useSync.js::startSync`/`useSyncStatus.js::checkProjectSyncStatus`; this is the matching
// UI-level disable + tooltip, reusing the same disabled/title idiom as `localBlocked`/`localTitle`.
function configBlocked(p) {
  return getProjectConfigEntry(p.id).status !== 'ok';
}

/** Every LOCAL popup item consumes the project's directory, so a missing volume blocks all of them */
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

  // Swap rows only once pointer crosses target row midpoint.
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
  // Persist optimistic drag reorder via host-resolved reorderProjects.
  reorderProjects(projects.value.map((p) => p.id));
}

const IDE_LOCAL_ARGS = {
  finder: p => [p],
  vscode: p => ['-a', 'Visual Studio Code', p],
  vscode_insiders: p => ['-a', 'Visual Studio Code - Insiders', p],
  antigravity: p => ['-a', 'Antigravity IDE', p],
  cursor: p => ['-a', 'Cursor', p],
}

async function openIdeLocal(ideName, path, projectId) {
  try {
    // Terminal goes through a dedicated command to avoid cold-start double windows.
    if (ideName === 'terminal') {
      await registerExternalTerminalLaunch({ owner: projectId ?? null, path });
      return;
    }
    const args = IDE_LOCAL_ARGS[ideName]?.(path)
    if (args) await invoke('macos_open', { args });
  } catch (e) {
    console.error(e);
    Toast.fire({ icon: 'error', title: String(e).replace('Error: ', '') });
  }
}

// Launch project DEV/BUILD commands in-app with tab deduplication.
function runProjectCommand(project, cmd) {
  openRunCommand(project, cmd, 'build');
}

function runProjectDev(project, cmd) {
  openRunCommand(project, cmd, 'dev');
}

// DEPLOY (docs/plan/done/deploy-action.md): always asks first via confirmAndDeploy, then runs local/remote.
// `resolveDeployCmd` (useDeploy.js) is the ONE impure resolver every caller here uses.
function runProjectDeploy(project) {
  const cmd = resolveDeployCmd(project);
  if (!cmd) return;
  confirmAndDeploy(project, cmd);
}

function deployCmdTitle(p) {
  const cmd = resolveDeployCmd(p);
  if (!cmd) return localTitle(p) || 'No deploy command detected — set one in Project Settings';
  return localTitle(p) || deployBlockedReason(p) || `${cmd} → ${deployTargetLabel(p)}`;
}

// Hook badges (D/H overlays on PUSH, deploy plan § "Visibility without Settings").
function hasSyncHooks(p) {
  const h = p?.hooks;
  return !!(h && (h.pre_pull_cmd || h.post_pull_cmd || h.pre_push_cmd || h.post_push_cmd));
}

function hooksTooltip(p) {
  const h = p?.hooks || {};
  const lines = [];
  if (h.pre_pull_cmd) lines.push(`pre-pull: ${h.pre_pull_cmd}`);
  if (h.post_pull_cmd) lines.push(`post-pull: ${h.post_pull_cmd}`);
  if (h.pre_push_cmd) lines.push(`pre-push: ${h.pre_push_cmd}`);
  if (h.post_push_cmd) lines.push(`post-push: ${h.post_push_cmd}`);
  return lines.join('\n') || 'Sync hooks configured for this host';
}

// D badge: lit only when a push to the CURRENT host would actually offer the deploy (same decision the post-push offer uses).
function deployOffersOnPush(p) {
  return shouldOfferDeployAfterPush({ direction: 'push', isDryRun: false, specificPaths: [], project: p, deployCmd: resolveDeployCmd(p) });
}

function deployOnPushTooltip(p) {
  return `Deploy after push: ${resolveDeployCmd(p)} → ${deployTargetLabel(p)}`;
}

// Cache remote resolved paths across repeated opens.
const resolvedPathCache = new Map();

async function resolveRemoteFullPath(host, path) {
  const needsResolve = path.startsWith('~/') || path === '~' || path.includes('$HOME');
  if (!needsResolve) {
    return path.startsWith('/') ? path : `/${path}`;
  }
  const key = `${host} ${path}`;
  const cached = resolvedPathCache.get(key);
  if (cached) return cached;

  // Resolve full remote path via backend invoke or return cached result.
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

// Copy path string with non-secure-context fallback for mobile companion.
async function copyPath(text, flashKey) {
  if (await copyText(text)) flashCopied(flashKey);
  else Toast.fire({ icon: 'error', title: 'Could not copy - select the path in the row and copy it by hand' });
}

async function copyLocalPath(project) {
  return copyPath(project.local_path, `local-${project.id}`);
}

// Copy remote path string directly to clipboard without SSH lookup.
async function copyRemotePath(project) {
  return copyPath(project.remote_path, `remote-${project.id}`);
}

// Pull latest REPORT.html if needed and open in system browser.
async function openReportHtml(project) {
  try {
    const path = await invoke('resolve_report_html', {
      localPath: project.local_path,
      remoteHost: project.remote_host || null,
      remotePath: project.remote_path || null,
    });
    await invoke('macos_open', { args: [path] });
    // Notify user when report is opened on Mac host.
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

// URI-scheme remote openers share one shape: `<scheme>://vscode-remote/ssh-remote+<host><path>` via `open`.
const REMOTE_URI_SCHEME = {
  vscode: 'vscode',
  vscode_insiders: 'vscode-insiders',
  cursor: 'cursor',
}

async function openIdeRemote(ideName, host, path, projectId) {
  try {
    const remotePath = await resolveRemoteFullPath(host, path);
    const scheme = REMOTE_URI_SCHEME[ideName];
    if (scheme) {
      await invoke('macos_open', { args: [`${scheme}://vscode-remote/ssh-remote+${host}${remotePath}`] })
    } else {
      await invoke('open_remote_subprocess', { ideName, host, path: remotePath, owner: projectId ?? null })
      if (ideName === 'terminal') scheduleExternalTermRescan();
    }
  } catch (e) {
    console.error(e);
    Toast.fire({ icon: 'error', title: String(e).replace('Error: ', '') });
  }
}

async function openUrl(url) {
  try { await invoke('macos_open', { args: [url] }); } catch (e) { console.error(e); }
}

// Search Console addresses a Domain property as `sc-domain:<registrable host>` - no scheme, no `www.`.
function openSearchConsole(p) {
  try {
    const host = new URL(p.production_url).hostname.replace(/^www\./, '')
    openUrl(`https://search.google.com/search-console/inspect?resource_id=${encodeURIComponent('sc-domain:' + host)}`)
  } catch (e) { console.error(e) }
}

// Read DEV command with fallback to detected stack info.
function getDevCmd(p) {
  return (p.dev_cmd_override ?? '').trim() || projectRuntime.value[p.id]?.stack_info?.dev_cmd || ''
}

function getBuildCmd(p) {
  return (p.build_cmd_override ?? '').trim() || projectRuntime.value[p.id]?.stack_info?.build_cmd || ''
}

// Tooltip explaining disabled run button state.
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
  flex: 1;
  overflow-y: auto;
  width: 100%;
  /* Table layout columns */
  --grid-cols: minmax(12rem, 2fr) 2.5rem 2.5rem 2.5rem 7rem 1fr;
  --grid-gap: 0.5rem;
}

/* Shared subgrid for header and rows to align column track widths across the entire table. */
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
  background: var(--glass-strong);
  border-bottom: 1px solid var(--border-card);
  z-index: 10;
  box-sizing: border-box;
}

/* Glass effect opt-in (src/composables/useVisualEffects.js) — see main.css's grouped block for
   the other permanently-mounted chrome surfaces this pairs with. */
html.fx-glass .grid-header {
  backdrop-filter: blur(8px);
}

.grid-header-cell {
  padding: 6px 0;
  font-size: 10px;
  font-weight: 800;
  color: var(--cyan-200);
  letter-spacing: 1px;
  text-transform: uppercase;
  white-space: nowrap;
  text-align: center;
}

/* Flex-centering for non-project header and row cells. */
.grid-header-cell:not(:first-child) {
  display: flex;
  align-items: center;
  justify-content: center;
}

/* display: contents on transition-group wrapper to preserve subgrid relationship. */
.grid-body {
  display: contents;
}

.grid-row {
  display: grid;
  grid-column: 1 / -1;
  grid-template-columns: subgrid;
  align-items: center;
  width: 100%;
  border-bottom: 1px solid var(--surface-faint);
  transition: background 0.15s;
  box-sizing: border-box;
  -webkit-user-drag: element !important;
}

.grid-row:hover {
  background: var(--surface-faint);
}

.row-syncing {
  background: rgba(6, 182, 212, 0.05);
}

.row-disabled {
  opacity: 0.5;
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

.grid-header-cell:first-child,
.grid-row-cell:first-child {
  padding-left: 6px;
  text-align: left;
}

/* Persistent glow for new project action button. */
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
  box-shadow: 0 0 6px var(--accent-cyan-glow);
}

.btn-new-project-inline:hover:not(:disabled) {
  box-shadow: 0 0 12px var(--accent-cyan-halo);
}

.grid-row-special {
  display: flex;
  grid-column: 1 / -1;
  width: 100%;
}

.th-with-ring {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

/* Project icon and text column container. */
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

.project-icon-col {
  display: flex;
  flex-direction: column;
  align-items: center;
  flex-shrink: 0;
}

/* Drag handle style */
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

/* Drag affordance overlay */
.project-drag-handle::before {
  content: '';
  position: absolute;
  inset: 0;
  background-color: rgba(0, 0, 0, 0.45);
  background-image:
    radial-gradient(circle, var(--text-soft) 1.2px, transparent 1.2px);
  background-size: 5px 5px;
  background-position: center;
  opacity: 0;
  transition: opacity 0.15s;
  pointer-events: none;
  border-radius: 6px;
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

/* Suppress pointer events on children during active drag. */
.projects-grid.dragging-active .grid-row * {
  pointer-events: none;
}

/* Transition Group list styles */
.project-list-move {
  transition: transform 0.3s cubic-bezier(0.25, 0.8, 0.25, 1);
}

/* LAST ACTION summary stacked under sync fieldset. */
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

/* Last-action colours follow the side each direction stands for. */
.la-push {
  color: var(--color-local);
}

.la-pull {
  color: var(--color-remote);
}

.la-host {
  color: var(--text-faint);
}

/* Compact cell padding for sync cluster. */
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

.switch.is-sync-off {
  opacity: 0.4;
  cursor: not-allowed;
  pointer-events: none;
}

.btn-refresh-project,
.btn-log-trigger {
  padding: 0 8px;
}

.actions-wrapper .btn-tech-push,
.actions-wrapper .btn-tech-pull {
  padding: 0 6px;
}

.btn-action-open {
  padding: 0 10px;
}

.btn-action-open i {
  margin-left: 0;
}

/* Project-specific open popup header and button styles. */
.popup-header {
  font-size: 11px;
  font-weight: 700;
  color: var(--gray-200);
  padding: 0 12px 8px 12px;
  border-bottom: 1px solid var(--border-color);
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
  color: var(--text-faint);
  padding: 4px 12px 2px;
}

.popup-copy-btn {
  background: transparent;
  border: none;
  color: var(--text-faint);
  cursor: pointer;
  padding: 0 2px;
  font-size: 9px;
  letter-spacing: 0.1em;
  transition: color 0.15s;
}

.popup-copy-btn:hover {
  color: var(--accent-cyan, var(--accent-cyan));
}

/* Keep tooltips accessible on disabled run buttons. */
.popup-item.popup-run-btn.popup-disabled {
  pointer-events: auto;
}

.popup-item.popup-run-btn.popup-disabled:hover {
  background: none;
  color: var(--text-soft);
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
  filter: drop-shadow(0 0 2px var(--border-strong));
}

.popup-icon-insiders {
  filter: hue-rotate(-50deg) saturate(2) brightness(1.2) drop-shadow(0 0 2px var(--border-strong));
}

.popup-project-icon {
  width: 18px;
  height: 18px;
  border-radius: 3px;
  object-fit: contain;
  margin-right: 6px;
  vertical-align: middle;
}

/* STOP button state during active sync (red). */
.btn-tech.btn-sync-stop {
  background-color: var(--accent-red);
  border-color: #7f1d1d;
  color: var(--white);
  text-shadow: 0 1px 3px var(--shade-strong);
  box-shadow: 0 0 10px var(--accent-red-halo);
}

.btn-tech.btn-sync-stop:hover:not(:disabled) {
  background-color: var(--red-400);
  box-shadow: 0 0 12px rgba(239, 68, 68, 0.75);
}

/* Missing local path warning state on git badge (amber). */
.btn-action-git.git-path-missing {
  filter: none;
  background: linear-gradient(135deg, #b45309, #78350f);
  border-color: rgba(245, 158, 11, 0.9);
  box-shadow: 0 0 6px rgba(245, 158, 11, 0.6);
}

/* DIVERGED state outline. */
.dry-group.is-diverged {
  outline: 1px solid rgba(251, 146, 60, 0.5);
  border-radius: 6px;
}

.btn-sync-diverged {
  box-shadow: 0 0 0 1px rgba(251, 146, 60, 0.6) !important;
}

.btn-sync-stale {
  outline: 1px dashed var(--red-400);
  outline-offset: 1px;
}

/* Narrow container layout adjustments (<=700px). */
@container main-view (max-width: 700px) {
  .projects-table-container {
    --grid-cols: minmax(7.5rem, 2fr) 2.1rem 1.9rem 2.0rem 4.2rem 1fr;
    --grid-gap: 0.4rem;
  }

  .project-info-row {
    gap: 6px;
  }

  /* Tighter padding for project text column in narrow mode. */
  .project-text-col {
    padding-right: 0;
  }

  /* Icon-only push/pull buttons in narrow mode. */
  .actions-wrapper .btn-tech-push,
  .actions-wrapper .btn-tech-pull {
    padding: 0 10px;
  }

  /* Compact 32px square buttons for OPEN/LOG in narrow mode. */
  .actions-wrapper .btn-action-open,
  .btn-log-trigger {
    width: 32px;
    padding: 0;
  }

  /* DRY run toggle in-flow placement. */
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

  /* Compact DRY toggle spacing. */
  .dry-toggle-center {
    padding: 0 2px;
  }

  .dry-toggle-center .dry-label {
    font-size: 6px;
    margin-bottom: 1px;
  }

  .dry-toggle-center .switch-sm {
    width: 16px;
    height: 8px;
  }

  /* Vertically centered slider toggle ball in narrow track. */
  .dry-toggle-center .switch-sm .slider:before {
    height: 6px;
    width: 6px;
    bottom: 1px;
    left: 1px;
  }

  .dry-toggle-center .switch-sm input:checked+.slider:before {
    transform: translateX(8px);
  }

  /* Tighten spacing between terminal and action columns in narrow mode. */
  .col-terminal {
    margin-left: 0;
  }

  /* LAST ACTION font-size in narrow mode (9px). */
  .last-action {
    font-size: 9px;
    line-height: 1.2;
  }
}

/* Skeleton loader classes */
.skeleton-zone-wrap {
  padding: 20px 12px;
  width: 100%;
}
.skeleton-zone-col {
  display: flex;
  flex-direction: column;
  gap: 15px;
  width: 100%;
}
.skeleton-zone-item {
  display: flex;
  gap: 15px;
  align-items: center;
}
.skeleton-box-icon {
  width: 28px;
  height: 28px;
  border-radius: 6px;
}
.skeleton-zone-text {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.skeleton-box-name {
  height: 12px;
  width: 30%;
}
.skeleton-box-path {
  height: 10px;
  width: 60%;
}

/* Empty state */
.empty-state-wrap {
  padding: 24px 16px;
  text-align: center;
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
}
.empty-state-icon {
  font-size: 24px;
  color: var(--text-darker);
}
.empty-state-text {
  color: var(--text-muted);
  font-size: 13px;
}
.btn-empty-add {
  font-size: 11px;
  padding: 4px 12px;
}

/* Project Row presentation classes */
.project-drag-img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.project-drag-icon-fallback {
  font-size: 16px;
}
.project-name-label {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.project-prod-link {
  color: var(--accent-cyan);
  font-size: 11px;
  text-decoration: none;
  display: flex;
  align-items: center;
  gap: 4px;
}
.project-prod-icon {
  font-size: 9px;
}

/* Popup classes */
.popup-header-wrap {
  display: flex;
  align-items: center;
}
.popup-icon-folder-fallback {
  font-size: 18px;
}
.popup-title-text {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
}
.popup-columns {
  display: flex;
}
.popup-col-local {
  flex: 1;
  min-width: 150px;
}
.popup-col-remote {
  flex: 1;
  min-width: 180px;
  border-left: 1px solid var(--border-color);
  padding-left: 4px;
}
.popup-item-icon {
  width: 14px;
}
.popup-icon-amber {
  color: var(--accent-amber);
}
.popup-icon-cyan {
  color: var(--accent-cyan);
}
.popup-icon-green {
  color: var(--accent-green);
}
</style>
