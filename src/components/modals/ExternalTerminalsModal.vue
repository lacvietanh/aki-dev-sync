<!--
  External Terminal.app sessions — the detail behind the TERM cell's slate badge.

  DELIBERATELY NOT A SCREEN MIRROR. The app cannot read another application's window contents, and
  the MVP does not pretend to: it reports what the OS can actually tell us about each session — the
  directory it stands in, how long it has been alive, and what it is executing right now. That last
  one is the part worth opening the modal for ("which window is the one running the dev server?").

  One "session" is the ROOT of a cwd subtree (src-tauri/src/system.rs), i.e. one window/tab, not the
  four processes inside it — the same rule the badge counts by, so the two can never disagree.
-->
<template>
  <BaseModal
    :show="showExternalTermModal"
    @close="closeExternalTermModal"
    container-class="exterm-modal"
  >
    <template #title>
      <i class="fa-solid fa-window-maximize mr-1"></i>
      Terminal.app sessions
      <span v-if="externalTermSessions.length" class="exterm-count">{{ externalTermSessions.length }}</span>
    </template>

    <div class="modal-body scrollable">
      <!-- Error sits ABOVE the list, never instead of it: a failed re-scan must not blank out the
           sessions we did successfully read a moment ago (they are almost certainly still true). -->
      <div v-if="externalTermError" class="exterm-error mb-3">
        <i class="fa-solid fa-triangle-exclamation"></i> {{ externalTermError }}
      </div>

      <div v-if="externalTermLoading && !externalTermSessions.length" class="exterm-empty">
        <i class="fa-solid fa-circle-notch fa-spin"></i> Scanning…
      </div>
      <div v-else-if="!externalTermSessions.length && !externalTermError" class="exterm-empty">
        No Terminal.app window is open.
      </div>

      <div v-for="s in externalTermSessions" :key="s.pid" class="exterm-row">
        <div class="exterm-head">
          <span class="exterm-name" :title="s.cwd">
            <i class="fa-solid" :class="s.project_path ? 'fa-folder-open' : 'fa-terminal'"></i>
            {{ projectNameOf(s) }}
          </span>
          <!-- Narrow mode: pid keeps its number (the one field you actually retype into `kill`),
               tty and uptime drop out rather than wrapping the row onto three lines. -->
          <span class="exterm-meta">
            <span class="exterm-tag" title="Process id">{{ s.pid }}</span>
            <span class="exterm-tag u-narrow-hide" title="Controlling terminal">{{ s.tty }}</span>
            <span class="exterm-tag u-narrow-hide" title="Running for">{{ s.etime }}</span>
          </span>
        </div>
        <div class="exterm-cwd" :title="s.cwd">{{ s.cwd }}</div>
        <div v-if="s.running.length" class="exterm-procs">
          <div v-for="p in s.running" :key="p.pid" class="exterm-proc" :title="p.command">
            <span class="exterm-proc-pid">{{ p.pid }}</span>
            <span class="exterm-proc-cmd">{{ p.command }}</span>
          </div>
        </div>
        <div v-else class="exterm-idle">idle shell — {{ s.command }}</div>
      </div>
    </div>

    <div class="modal-footer">
      <button class="btn-secondary" :disabled="externalTermLoading" @click="refreshExternalTermSessions">
        <i class="fa-solid fa-rotate" :class="{ 'fa-spin': externalTermLoading }"></i>
        <span class="u-narrow-hide"> Refresh</span>
      </button>
      <button class="btn-secondary" @click="closeExternalTermModal">Close</button>
    </div>
  </BaseModal>
</template>

<script setup>
import BaseModal from './BaseModal.vue'
import { projects } from '../../store/projectStore'
import {
  showExternalTermModal,
  externalTermSessions,
  externalTermLoading,
  externalTermError,
  refreshExternalTermSessions,
  closeExternalTermModal,
} from '../../composables/useExternalTerminals'

// `project_path` comes back as the ORIGINAL path string the scan was given (the backend resolves
// symlinks for matching but reports the un-resolved name), so this lookup is a plain string compare
// against the same field the store holds.
function projectNameOf(s) {
  if (!s.project_path) return shortDir(s.cwd)
  return projects.value.find(p => p.local_path === s.project_path)?.name || shortDir(s.cwd)
}

/** Last path segment, `~` for home. A full path is already on the line below; the heading wants the
 *  short name a person would say out loud. */
function shortDir(p) {
  const trimmed = String(p || '').replace(/\/+$/, '')
  return trimmed.split('/').pop() || trimmed || '/'
}
</script>

<style scoped>
.exterm-count {
  margin-left: 6px;
  padding: 1px 6px;
  border-radius: 8px;
  background: var(--bg-tertiary);
  color: var(--text-muted);
  font-size: 11px;
}

/* Local, not a shared `.alert-box`: that class is scoped to IntroModal.vue and has never been a
   global. One error line does not justify promoting it. */
.exterm-error {
  padding: 8px 10px;
  border: 1px solid var(--accent-red);
  border-radius: 4px;
  background: rgba(239, 68, 68, 0.08);
  color: var(--accent-red);
  font-size: 12px;
}

.exterm-empty {
  padding: 24px 8px;
  text-align: center;
  color: var(--text-muted);
  font-size: 12px;
}

.exterm-row {
  padding: 8px 0;
  border-bottom: 1px solid var(--border-card);
}
.exterm-row:last-child {
  border-bottom: none;
}

.exterm-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.exterm-name {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-light);
  font-size: 12px;
  font-weight: 700;
}

.exterm-meta {
  display: inline-flex;
  gap: 4px;
  flex-shrink: 0;
}

.exterm-tag {
  padding: 1px 5px;
  border: 1px solid var(--border-color);
  border-radius: 3px;
  color: var(--text-muted);
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", Menlo, Courier, monospace;
  font-size: 10px;
  line-height: 1.4;
}

.exterm-cwd {
  margin-top: 2px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-muted);
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", Menlo, Courier, monospace;
  font-size: 11px;
}

.exterm-procs {
  margin-top: 4px;
}

.exterm-proc {
  display: flex;
  gap: 6px;
  align-items: baseline;
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", Menlo, Courier, monospace;
  font-size: 11px;
}

.exterm-proc-pid {
  flex-shrink: 0;
  color: var(--text-muted);
}

.exterm-proc-cmd {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--accent-cyan);
}

.exterm-idle {
  margin-top: 4px;
  color: var(--text-muted);
  font-size: 11px;
  font-style: italic;
}
</style>
