<template>
  <BaseModal :show="showConfigModal && !!editingProject" @close="closeConfig">
    <template #title>
      <i class="fa-solid fa-gear mr-1"></i> Configuration: {{ editingProject?.name }}
      <!-- Read-only reason lives as a title suffix + tooltip without extra rows (Extreme Narrow), same as Tasks. -->
      <span v-if="configBlocked" class="config-readonly-tag" :title="configEntry.error || undefined"> — {{ configEntry.status }}</span>
    </template>
    <div class="modal-body scrollable">
      <div class="config-section-header"><span class="owner-badge owner-project">in the project</span></div>
      <div class="form-grid mb-1">
        <div class="form-group">
          <label>Project Name</label>
          <input type="text" v-model="editingProject.name" />
        </div>
        <div class="form-group full-width">
          <label>Production URL <i class="fa-solid fa-circle-info help-icon" title="Used by the web icon button next to the project name to open the production site in a browser"></i></label>
          <input type="text" v-model="editingProject.production_url" placeholder="https://..." />
        </div>
      </div>

      <div class="config-section-header"><span class="owner-badge owner-machine">on this Mac</span></div>
      <div class="form-grid mb-1">
        <div class="form-group">
          <label>Remote Host</label>
          <select :value="editingProject.remote_host" @change="onRemoteHostChange">
            <option v-for="h in sshHosts" :key="h" :value="h">{{ h }}</option>
          </select>
        </div>
        <!-- Both fields are `required` in the destructive sense: empty makes rsync operate on `/`
             (docs/plan/done/1.20.1-flow-audit-fixes.md §2.1). The invalid state rides the input's own
             border + title - no extra row or error label (UI Extreme Narrow). -->
        <div class="form-group full-width">
          <label>Local Path (Absolute)</label>
          <input type="text" v-model="editingProject.local_path" placeholder="/Volumes/DEV/..."
                 :class="{ 'input-invalid': localPathInvalid }"
                 :title="localPathInvalid ? pathError : ''" />
        </div>
        <div class="form-group full-width">
          <label>Remote Destination Directory</label>
          <input type="text" v-model="editingProject.remote_path" placeholder="~/"
                 :class="{ 'input-invalid': remotePathInvalid }"
                 :title="remotePathInvalid ? pathError : ''" />
        </div>
      </div>

      <!-- RUN COMMANDS (runs on this machine's shell; the commands themselves are project facts stored in
           project.json, per docs/research/akidevsync-project-config-scope-2.md). -->
      <div class="full-width config-group commands-group mb-1 mt-1">
        <h4 class="group-title commands-title">
          <i class="fa-solid fa-terminal mr-1"></i> RUN COMMANDS
          <span class="owner-badge owner-project">in the project</span>
        </h4>
        <p class="commands-hint">Chạy trên Mac Terminal của bạn. Để trống → dùng mặc định theo stack.</p>
        <div class="commands-row">
          <div class="form-group">
            <label class="text-green-dim">DEV <span class="default-hint">{{ devCmdDefault }}</span></label>
            <input
              type="text"
              v-model="editingProject.dev_cmd_override"
              :placeholder="devCmdDefault || 'e.g. npm run dev'"
              class="code-input"
            />
          </div>
          <div class="form-group">
            <label class="text-amber-dim">BUILD <span class="default-hint">{{ buildCmdDefault }}</span></label>
            <input
              type="text"
              v-model="editingProject.build_cmd_override"
              :placeholder="buildCmdDefault || 'e.g. npm run build'"
              class="code-input"
            />
          </div>
          <div class="form-group">
            <label class="text-cyan-dim">DEPLOY <span class="default-hint">{{ deployCmdDefault }}</span></label>
            <input
              type="text"
              v-model="editingProject.deploy_cmd"
              :placeholder="deployCmdDefault || 'e.g. npm run deploy'"
              class="code-input"
            />
          </div>
        </div>
        <!-- Deploy target (docs/plan/deploy-action.md, amended 2026-09-28): a remote deploy names its own
             host, never the sync host above - that one is a dropdown away from any other box. -->
        <div class="deploy-target-row">
          <div class="form-group">
            <label class="text-cyan-dim">Deploy Runs On</label>
            <select v-model="deployRunOn">
              <option value="local">Local (this Mac)</option>
              <option value="remote">Remote (SSH)</option>
            </select>
          </div>
          <template v-if="deployRunOn === 'remote'">
            <div class="form-group">
              <label class="text-cyan-dim">Deploy Host</label>
              <select v-model="deployHost" :class="{ 'input-invalid': !deployHost }" :title="deployHost ? '' : 'Remote deploy needs its own host - it never uses the sync host'">
                <option value="" disabled>Pick a host</option>
                <option v-for="h in sshHosts" :key="h" :value="h">{{ h }}</option>
              </select>
            </div>
            <div class="form-group">
              <label class="text-cyan-dim">Deploy Path</label>
              <input type="text" v-model="deployPath" :placeholder="editingProject.remote_path || '~/app'" class="code-input" />
            </div>
          </template>
          <div class="form-group checkbox-inline">
            <input type="checkbox" id="deploy-on-push" v-model="deployOnPush" />
            <label for="deploy-on-push">{{ deployRunOn === 'remote' ? 'Offer Deploy after a successful PUSH to the deploy host' : 'Offer Deploy after a successful PUSH' }}</label>
          </div>
        </div>
      </div>

      <!-- STACK PRESETS -->
      <div class="full-width config-group mb-1 mt-1 dashed-group">
        <h4 class="group-title text-muted dashed-group-title"><i class="fa-solid fa-layer-group mr-1"></i> EXCLUDE PRESETS</h4>
        <div class="row-gap-8">
          <button class="btn-secondary btn-compact" @click="applyPreset('nuxt4')">Nuxt 4</button>
          <button class="btn-secondary btn-compact" @click="applyPreset('tauriv2')">Tauri v2 (Rust)</button>
          <button class="btn-secondary btn-compact" @click="applyPreset('default')">Aki Default</button>
        </div>
        <p class="text-muted hint-note">Applies standard exclude filters for both PUSH and PULL (overwrites current excludes).</p>
      </div>

      <!-- PROJECT ICON -->
      <div class="full-width config-group mb-1 mt-1 dashed-group">
        <h4 class="group-title text-muted dashed-group-title"><i class="fa-solid fa-image mr-1"></i> PROJECT ICON</h4>
        <div class="row-gap-10">
          <img v-if="!iconLoadFailed && iconPreviewSrc" :src="iconPreviewSrc" alt=""
               class="project-icon-preview" @error="iconLoadFailed = true" />
          <button class="btn-secondary btn-compact" :disabled="reloadingIcon" @click="reloadIcon">
            <i class="fa-solid fa-rotate mr-1"></i> {{ reloadingIcon ? 'Reloading...' : 'Reload Icon' }}
          </button>
        </div>
        <p class="text-muted hint-note">Detected by project type: Tauri (src-tauri/tauri.conf.json) checks src-tauri/icons/32x32.png, 64x64.png, icon.png, 128x128.png. Nuxt (nuxt.config.ts|js) or web (package.json/index.html) checks public/favicon/icon-48.png, public/favicon.ico, public/favicon/favicon.ico, public/favicon/icon-192.png, public/icon.png, favicon.ico, icon.png. Other project types check the same list minus icon-192.png. Among the candidates that exist, the smallest file wins - and if that one is over 250 KB, no icon is shown rather than the next candidate. Reload after adding or replacing an icon file.</p>
      </div>

      <!-- PUSH + PULL side-by-side -->
      <div class="excludes-split full-width mt-1">
        <!-- PUSH GROUP -->
        <div class="config-group push-group">
          <h4 class="group-title text-push"><i class="fa-solid fa-arrow-up mr-1"></i> PUSH (Local → Remote)</h4>
          <div class="form-group mb-1">
            <label class="text-push">Excludes (1 per line)</label>
            <textarea class="large-textarea border-push" v-model="pushExcludesText" rows="5"></textarea>
          </div>
          <div class="form-group">
            <div class="scripts-toggle" @click="togglePushScripts = !togglePushScripts">
              <label :class="hasPushScripts ? 'text-push' : 'text-muted'" :style="{ cursor: 'pointer', fontSize: '11px', fontWeight: hasPushScripts ? '800' : '600' }">
                <i class="fa-solid fa-code mr-1"></i> Pre &amp; Post Scripts
                <i :class="[togglePushScripts ? 'fa-solid fa-chevron-up' : 'fa-solid fa-chevron-down', 'chevron-gap']"></i>
              </label>
            </div>
            <div v-show="togglePushScripts" class="col-gap-8">
              <div class="form-group">
                <label class="text-push label-dim">Pre-Push</label>
                <textarea class="large-textarea code-font border-push" v-model="editingProject.hooks.pre_push_cmd" rows="2"></textarea>
              </div>
              <div class="form-group">
                <label class="text-push label-dim">Post-Push</label>
                <textarea class="large-textarea code-font border-push" v-model="editingProject.hooks.post_push_cmd" rows="2"></textarea>
              </div>
            </div>
          </div>
        </div>

        <!-- PULL GROUP -->
        <div class="config-group pull-group">
          <h4 class="group-title text-pull"><i class="fa-solid fa-arrow-down mr-1"></i> PULL (Remote → Local)</h4>
          <div class="form-group mb-1">
            <label class="text-pull">Excludes (1 per line)</label>
            <textarea class="large-textarea border-pull" v-model="pullExcludesText" rows="5"></textarea>
          </div>
          <div class="form-group">
            <div class="scripts-toggle" @click="togglePullScripts = !togglePullScripts">
              <label :class="hasPullScripts ? 'text-pull' : 'text-muted'" :style="{ cursor: 'pointer', fontSize: '11px', fontWeight: hasPullScripts ? '800' : '600' }">
                <i class="fa-solid fa-code mr-1"></i> Pre &amp; Post Scripts
                <i :class="togglePullScripts ? 'fa-solid fa-chevron-up' : 'fa-solid fa-chevron-down'" class="chevron-gap"></i>
              </label>
            </div>
            <div v-show="togglePullScripts" class="col-gap-8">
              <div class="form-group">
                <label class="text-pull label-dim">Pre-Pull</label>
                <textarea class="large-textarea code-font border-pull" v-model="editingProject.hooks.pre_pull_cmd" rows="2"></textarea>
              </div>
              <div class="form-group">
                <label class="text-pull label-dim">Post-Pull</label>
                <textarea class="large-textarea code-font border-pull" v-model="editingProject.hooks.post_pull_cmd" rows="2"></textarea>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div class="form-group full-width hooks-section mt-1">
        <div class="checkbox-group mb-0">
          <input type="checkbox" id="disabled-modal" v-model="editingProject.disabled" />
          <label for="disabled-modal">
            <i class="fa-solid fa-pause mr-1"></i>
            Disable this project - skip background sync/git checks (reduce system load)
          </label>
        </div>
        <div class="checkbox-group mb-0 mt-1">
          <input type="checkbox" id="run-remote-modal" v-model="editingProject.hooks.run_hooks_on_remote" />
          <label for="run-remote-modal">Execute hooks on Remote Host via SSH (uncheck for Local Shell)</label>
        </div>
        <div class="checkbox-group mb-0 mt-1">
          <input type="checkbox" id="ignore-hook-errors-modal" v-model="editingProject.hooks.ignore_hook_errors" />
          <label for="ignore-hook-errors-modal">Ignore hook errors - sync continues even if a hook exits non-zero</label>
        </div>
        <div class="checkbox-group mb-0 mt-1">
          <input type="checkbox" id="delete-on-pull-modal" v-model="editingProject.delete_on_pull" />
          <label for="delete-on-pull-modal" class="text-pull">
            <i class="fa-solid fa-triangle-exclamation mr-1"></i>
            PULL with <code>--delete</code> - removes local files not present on remote
          </label>
        </div>
        <div class="checkbox-group mb-0 mt-1">
          <input type="checkbox" id="delete-on-push-modal" v-model="editingProject.delete_on_push" />
          <label for="delete-on-push-modal" class="text-push">
            <i class="fa-solid fa-triangle-exclamation mr-1"></i>
            PUSH with <code>--delete</code> - removes remote files not present on local
          </label>
        </div>
      </div>
    </div>
    <div class="modal-footer">
      <button class="btn-delete" @click="confirmRemove">
        <i class="fa-solid fa-folder-minus mr-1"></i> Remove from List
      </button>
      <div>
        <button class="btn-secondary mr-1" @click="closeConfig">Cancel</button>
        <button class="btn-save" :disabled="!!pathError || configBlocked" :title="pathError || (configBlocked ? `.akidevsync/project.json is ${configEntry.status} — cannot save` : '')" @click="saveConfig"><i class="fa-solid fa-floppy-disk mr-1"></i> Save Changes</button>
      </div>
    </div>
  </BaseModal>
</template>

<script setup>
import { ref, computed, watch } from 'vue'
import BaseModal from './BaseModal.vue'
import { useProjects } from '../../composables/useProjects'
import { useSsh } from '../../composables/useSsh'
import { projectPathIssue, resolveHostSwitch } from '../../composables/useProjectConfig'
import { canSaveProjectConfig } from '../../composables/projectConfigPure'
import { getProjectConfigEntry } from '../../store/projectConfigStore'
import { projects, iconTimestamp, refreshProjectIcons } from '../../store/projectStore'
import { projectIconSrc } from '../../utils/projectIcon'

const { showConfigModal, editingProject, closeConfig, saveConfig, confirmRemove, Toast, projectRuntime } = useProjects()
const { sshHosts } = useSsh()

// The exact same decision `applyProjectConfig` (the writer) uses -
// `canSaveProjectConfig` - so the button can never disagree with what a save would actually do. A brand-new
// project (not yet in `projects.value`) is always allowed; an existing one is blocked whenever its
// project.json read is not 'ok'/'missing' - 'unknown' (e.g. mid-Refresh) included, unlike before.
const configEntry = computed(() => getProjectConfigEntry(editingProject.value?.id))
const isNewProject = computed(() => !projects.value.some((p) => p.id === editingProject.value?.id))
const configBlocked = computed(() => !canSaveProjectConfig(isNewProject.value, configEntry.value.status))

const iconPreviewSrc = computed(() => projectIconSrc(editingProject.value?.id, iconTimestamp.value))
const iconLoadFailed = ref(false)
const reloadingIcon = ref(false)
watch(() => editingProject.value?.id, () => { iconLoadFailed.value = false })

async function reloadIcon() {
  reloadingIcon.value = true
  try {
    await refreshProjectIcons()
    iconTimestamp.value = Date.now()
    iconLoadFailed.value = false
  } finally {
    reloadingIcon.value = false
  }
}

const togglePushScripts = ref(false)
const togglePullScripts = ref(false)

// The dialog's own Remote Host select must use the same switch logic as the table dropdown
// (remoteActions.setRemoteHost) - never keep the outgoing host's hooks on the new one. `remote_path` is a
// project fact (1.32.1) and is left untouched by this switch. Mutates the draft `editingProject` only;
// nothing is saved until the user hits Save.
function onRemoteHostChange(event) {
  const newHost = event.target.value
  const project = editingProject.value
  if (!project || project.remote_host === newHost) return
  const next = resolveHostSwitch(project, newHost)
  project.targets = next.targets
  project.remote_host = next.remote_host
  project.hooks = next.hooks
}

// Same predicate the sync path and saveConfig use, so the button state can never disagree with what the app will actually accept.
const pathIssue = computed(() => projectPathIssue(editingProject.value))
const pathError = computed(() => pathIssue.value.message)
const localPathInvalid = computed(() => pathIssue.value.field === 'local_path')
const remotePathInvalid = computed(() => pathIssue.value.field === 'remote_path')

const hasPushScripts = computed(() => {
  return !!(editingProject.value?.hooks?.pre_push_cmd?.trim() || editingProject.value?.hooks?.post_push_cmd?.trim())
})

const hasPullScripts = computed(() => {
  return !!(editingProject.value?.hooks?.pre_pull_cmd?.trim() || editingProject.value?.hooks?.post_pull_cmd?.trim())
})

const pullExcludesText = computed({
  get() { return editingProject.value?.pull_excludes ? editingProject.value.pull_excludes.join("\n") : "" },
  set(val) { if (editingProject.value) editingProject.value.pull_excludes = val.split("\n").map(s => s.trim()).filter(s => s !== "") }
})

const pushExcludesText = computed({
  get() { return editingProject.value?.push_excludes ? editingProject.value.push_excludes.join("\n") : "" },
  set(val) { if (editingProject.value) editingProject.value.push_excludes = val.split("\n").map(s => s.trim()).filter(s => s !== "") }
})

// Show the detected stack defaults as placeholder hints
const devCmdDefault = computed(() => {
  if (!editingProject.value) return ''
  const stack = projectRuntime.value[editingProject.value.id]?.stack_info
  return stack?.dev_cmd || ''
})

const buildCmdDefault = computed(() => {
  if (!editingProject.value) return ''
  const stack = projectRuntime.value[editingProject.value.id]?.stack_info
  return stack?.build_cmd || ''
})

const deployCmdDefault = computed(() => {
  if (!editingProject.value) return ''
  const stack = projectRuntime.value[editingProject.value.id]?.stack_info
  return stack?.deploy_cmd || ''
})

// `editingProject.deploy` (docs/plan/deploy-action.md): one per project, untouched by a sync host switch.
// `null` (never configured) reads as "local, off" rather than crashing the bindings.
function deployField(key, fallback) {
  return computed({
    get() { return editingProject.value?.deploy?.[key] || fallback },
    set(val) {
      if (!editingProject.value) return
      editingProject.value.deploy = { run_on: 'local', on_push: false, ...(editingProject.value.deploy || {}), [key]: val }
    },
  })
}
const deployRunOn = deployField('run_on', 'local')
const deployOnPush = deployField('on_push', false)
const deployHost = deployField('host', '')
const deployPath = deployField('path', '')

function applyPreset(stack) {
  if (!editingProject.value) return
  const common = [".DS_Store", "*.log", ".env", ".claude/", ".gemini/"]
  let baseExcludes = []

  if (stack === 'nuxt4') {
    baseExcludes = [...common, "node_modules/", ".nuxt/", ".output/", "dist/"]
  } else if (stack === 'tauriv2') {
    baseExcludes = [...common, "node_modules/", "dist/", "src-tauri/target/", "src-tauri/gen/"]
  } else {
    baseExcludes = [".DS_Store", "*.log", "node_modules/", ".nuxt/", ".output/", ".wrangler/", "dist/", ".claude/"]
  }

  editingProject.value.push_excludes = [...baseExcludes]
  editingProject.value.pull_excludes = [...baseExcludes, ".git/"]

  Toast.fire({ icon: 'success', title: `Preset ${stack.toUpperCase()} applied` })
}
</script>

<style scoped>
.dashed-group { border: 1px dashed var(--gray-600); padding: 12px; border-radius: 8px; }
.dashed-group-title { font-size: 12px; margin-bottom: 8px; }
.row-gap-10 { display: flex; align-items: center; gap: 10px; }
.col-gap-8 { display: flex; flex-direction: column; gap: 8px; }
.btn-compact { font-size: 11px; padding: 4px 12px; }
.hint-note { font-size: 11px; margin-top: 6px; font-style: italic; }
.project-icon-preview { width: 32px; height: 32px; border-radius: 6px; }
.scripts-toggle { cursor: pointer; display: inline-block; margin-bottom: 6px; }
.chevron-gap { margin-left: 4px; }
.text-pull { color: var(--color-remote); }
.text-push { color: var(--color-local); }
.label-dim { opacity: 0.8; }
.excludes-split {
  display: flex;
  gap: 10px;
  align-items: flex-start;
}

.excludes-split > .config-group {
  flex: 1;
  min-width: 0;
  margin-top: 0 !important;
}


/* Invalid path = the field itself turns red. No error row, no helper label - the reason lives in
   the native tooltip on the input and on the disabled Save button (UI Extreme Narrow). */
.input-invalid {
  border-color: var(--accent-red) !important;
  background: var(--accent-red-wash);
}

.commands-group {
  border: 1px solid var(--accent-green-edge);
  background: rgba(16, 185, 129, 0.04);
  padding: 12px;
  border-radius: 8px;
}

.commands-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--emerald-300);
  margin-bottom: 4px;
}

.config-section-header {
  margin: 4px 0 2px;
}

/* Owner-scope tag (docs/plan/settings-and-state-layout.md § D): where a field is saved, not who can edit it. */
.owner-badge {
  font-size: 9px;
  font-weight: 600;
  font-style: normal;
  padding: 0 5px;
  border-radius: 3px;
  letter-spacing: 0.2px;
  white-space: nowrap;
}

.owner-project {
  color: var(--emerald-300);
  background: var(--accent-green-wash);
  border: 1px solid var(--accent-green-edge);
}

.owner-machine {
  color: var(--text-darker);
  background: rgba(148, 163, 184, 0.08);
  border: 1px solid var(--border-color);
}

.config-readonly-tag {
  font-size: 11px;
  font-weight: 600;
  color: var(--amber-400);
}

.commands-hint {
  font-size: 11px;
  color: var(--text-darker);
  font-style: italic;
  margin: 0 0 10px;
}

.commands-row {
  display: flex;
  gap: 10px;
}

.commands-row > .form-group {
  flex: 1;
  min-width: 0;
}

.text-green-dim {
  color: var(--emerald-300) !important;
  display: flex;
  align-items: center;
  gap: 6px;
}

.text-amber-dim {
  color: var(--amber-400) !important;
  display: flex;
  align-items: center;
  gap: 6px;
}

.text-cyan-dim {
  color: var(--accent-cyan) !important;
  display: flex;
  align-items: center;
  gap: 6px;
}

.deploy-target-row {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-top: 8px;
}

.checkbox-inline {
  flex-direction: row;
  align-items: center;
  gap: 6px;
  display: flex;
}

.checkbox-inline label {
  font-size: 11px;
  color: var(--text-darker);
}

.default-hint {
  font-size: 10px;
  font-weight: 400;
  color: var(--text-darker);
  font-style: italic;
  font-family: monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 160px;
}

.code-input {
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  padding: 6px 8px;
  color: #a7f3d0;
  font-size: 12px;
  font-family: Monaco, Consolas, monospace;
  outline: none;
  transition: border-color 0.2s;
  width: 100%;
  box-sizing: border-box;
}

.code-input:focus {
  border-color: var(--accent-green);
}

/* Narrow mode - SSoT breakpoint is 700px (main.css). This file used to carry a rogue 560px block
   for .excludes-split; folded in here. Checked at the wider trigger: at 700px the modal body is
   ~660px wide, so two exclude columns are ~325px each - a textarea of `.gitignore` lines still
   reads fine there, but the PUSH/PULL pair plus their Pre/Post script textareas is already the
   tightest thing in this modal, and stacking at 700 costs only vertical scroll in a modal that
   already scrolls. Same reasoning for the DEV/BUILD command pair. */
@media (max-width: 700px) {
  .excludes-split {
    flex-direction: column;
  }

  .excludes-split > .config-group {
    width: 100%;
  }

  .commands-row {
    flex-direction: column;
  }

  .deploy-target-row {
    flex-direction: column;
    align-items: flex-start;
  }

  /* Stacked, the label owns the full row - let the default-command hint use it instead of
     truncating at a fixed 160px. */
  .default-hint {
    max-width: 100%;
  }
}
</style>
