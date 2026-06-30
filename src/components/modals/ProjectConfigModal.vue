<template>
  <BaseModal :show="showConfigModal && !!editingProject" @close="closeConfig">
    <template #title>
      <i class="fa-solid fa-gear mr-1"></i> Configuration: {{ editingProject?.name }}
    </template>
    <div class="modal-body scrollable">
      <div class="form-grid mb-1">
        <div class="form-group">
          <label>Project Name</label>
          <input type="text" v-model="editingProject.name" />
        </div>
        <div class="form-group">
          <label>Remote Host</label>
          <select v-model="editingProject.remote_host">
            <option v-for="h in sshHosts" :key="h" :value="h">{{ h }}</option>
          </select>
        </div>
        <div class="form-group full-width">
          <label>Local Path (Absolute)</label>
          <input type="text" v-model="editingProject.local_path" placeholder="/Volumes/DEV/..." />
        </div>
        <div class="form-group full-width">
          <label>Remote Destination Directory</label>
          <input type="text" v-model="editingProject.remote_path" placeholder="~/" />
        </div>
        <div class="form-group full-width">
          <label>Production URL <i class="fa-solid fa-circle-info help-icon" title="Used by the web icon button next to the project name to open the production site in a browser"></i></label>
          <input type="text" v-model="editingProject.production_url" placeholder="https://..." />
        </div>
      </div>

      <!-- RUN COMMANDS — LOCAL MACHINE ONLY -->
      <div class="full-width config-group commands-group mb-1 mt-1">
        <h4 class="group-title commands-title">
          <i class="fa-solid fa-terminal mr-1"></i> RUN COMMANDS
          <span class="local-badge">💻 LOCAL ONLY</span>
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
        </div>
      </div>

      <!-- STACK PRESETS -->
      <div class="full-width config-group mb-1 mt-1" style="border: 1px dashed #4b5563; padding: 12px; border-radius: 8px;">
        <h4 class="group-title text-muted" style="font-size: 12px; margin-bottom: 8px;"><i class="fa-solid fa-layer-group mr-1"></i> EXCLUDE PRESETS</h4>
        <div style="display: flex; gap: 8px;">
          <button class="btn-secondary" style="font-size: 11px; padding: 4px 12px;" @click="applyPreset('nuxt4')">Nuxt 4</button>
          <button class="btn-secondary" style="font-size: 11px; padding: 4px 12px;" @click="applyPreset('tauriv2')">Tauri v2 (Rust)</button>
          <button class="btn-secondary" style="font-size: 11px; padding: 4px 12px;" @click="applyPreset('default')">Aki Default</button>
        </div>
        <p class="text-muted" style="font-size: 11px; margin-top: 6px; font-style: italic;">Applies standard exclude filters for both PUSH and PULL (overwrites current excludes).</p>
      </div>

      <!-- PUSH + PULL side-by-side -->
      <div class="excludes-split full-width mt-1">
        <!-- PUSH GROUP -->
        <div class="config-group push-group">
          <h4 class="group-title text-amber"><i class="fa-solid fa-arrow-up mr-1"></i> PUSH (Local → Remote)</h4>
          <div class="form-group mb-1">
            <label class="text-amber">Excludes (1 per line)</label>
            <textarea class="large-textarea border-push" v-model="pushExcludesText" rows="5"></textarea>
          </div>
          <div class="form-group">
            <div style="cursor: pointer; display: inline-block; margin-bottom: 6px;" @click="togglePushScripts = !togglePushScripts">
              <label :class="hasPushScripts ? 'text-amber' : 'text-muted'" :style="{ cursor: 'pointer', fontSize: '11px', fontWeight: hasPushScripts ? '800' : '600' }">
                <i class="fa-solid fa-code mr-1"></i> Pre &amp; Post Scripts
                <i :class="togglePushScripts ? 'fa-solid fa-chevron-up' : 'fa-solid fa-chevron-down'" style="margin-left: 4px;"></i>
              </label>
            </div>
            <div v-show="togglePushScripts" style="display: flex; flex-direction: column; gap: 8px;">
              <div class="form-group">
                <label class="text-amber" style="opacity: 0.8">Pre-Push</label>
                <textarea class="large-textarea code-font border-push" v-model="editingProject.hooks.pre_push_cmd" rows="2"></textarea>
              </div>
              <div class="form-group">
                <label class="text-amber" style="opacity: 0.8">Post-Push</label>
                <textarea class="large-textarea code-font border-push" v-model="editingProject.hooks.post_push_cmd" rows="2"></textarea>
              </div>
            </div>
          </div>
        </div>

        <!-- PULL GROUP -->
        <div class="config-group pull-group">
          <h4 class="group-title text-blue"><i class="fa-solid fa-arrow-down mr-1"></i> PULL (Remote → Local)</h4>
          <div class="form-group mb-1">
            <label class="text-blue">Excludes (1 per line)</label>
            <textarea class="large-textarea border-pull" v-model="pullExcludesText" rows="5"></textarea>
          </div>
          <div class="form-group">
            <div style="cursor: pointer; display: inline-block; margin-bottom: 6px;" @click="togglePullScripts = !togglePullScripts">
              <label :class="hasPullScripts ? 'text-blue' : 'text-muted'" :style="{ cursor: 'pointer', fontSize: '11px', fontWeight: hasPullScripts ? '800' : '600' }">
                <i class="fa-solid fa-code mr-1"></i> Pre &amp; Post Scripts
                <i :class="togglePullScripts ? 'fa-solid fa-chevron-up' : 'fa-solid fa-chevron-down'" style="margin-left: 4px;"></i>
              </label>
            </div>
            <div v-show="togglePullScripts" style="display: flex; flex-direction: column; gap: 8px;">
              <div class="form-group">
                <label class="text-blue" style="opacity: 0.8">Pre-Pull</label>
                <textarea class="large-textarea code-font border-pull" v-model="editingProject.hooks.pre_pull_cmd" rows="2"></textarea>
              </div>
              <div class="form-group">
                <label class="text-blue" style="opacity: 0.8">Post-Pull</label>
                <textarea class="large-textarea code-font border-pull" v-model="editingProject.hooks.post_pull_cmd" rows="2"></textarea>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div class="form-group full-width hooks-section mt-1">
        <div class="checkbox-group mb-0">
          <input type="checkbox" id="run-remote-modal" v-model="editingProject.hooks.run_hooks_on_remote" />
          <label for="run-remote-modal">Execute hooks on Remote Host via SSH (uncheck for Local Shell)</label>
        </div>
        <div class="checkbox-group mb-0 mt-1">
          <input type="checkbox" id="ignore-hook-errors-modal" v-model="editingProject.hooks.ignore_hook_errors" />
          <label for="ignore-hook-errors-modal">Ignore hook errors — sync continues even if a hook exits non-zero</label>
        </div>
        <div class="checkbox-group mb-0 mt-1">
          <input type="checkbox" id="delete-on-pull-modal" v-model="editingProject.delete_on_pull" />
          <label for="delete-on-pull-modal" style="color: #60a5fa;">
            <i class="fa-solid fa-triangle-exclamation mr-1"></i>
            PULL with <code>--delete</code> — removes local files not present on remote
          </label>
        </div>
        <div class="checkbox-group mb-0 mt-1">
          <input type="checkbox" id="delete-on-push-modal" v-model="editingProject.delete_on_push" />
          <label for="delete-on-push-modal" style="color: #fbbf24;">
            <i class="fa-solid fa-triangle-exclamation mr-1"></i>
            PUSH with <code>--delete</code> — removes remote files not present on local
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
        <button class="btn-save" @click="saveConfig"><i class="fa-solid fa-floppy-disk mr-1"></i> Save Changes</button>
      </div>
    </div>
  </BaseModal>
</template>

<script setup>
import { ref, computed } from 'vue'
import BaseModal from './BaseModal.vue'
import { useProjects } from '../../composables/useProjects'
import { useSsh } from '../../composables/useSsh'

const { showConfigModal, editingProject, closeConfig, saveConfig, confirmRemove, Toast, projectRuntime } = useProjects()
const { sshHosts } = useSsh()

const togglePushScripts = ref(false)
const togglePullScripts = ref(false)

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

/* Stack below ~560px (typical when modal is narrowed by viewport) */
@media (max-width: 560px) {
  .excludes-split {
    flex-direction: column;
  }
  .excludes-split > .config-group {
    width: 100%;
  }
}

.commands-group {
  border: 1px solid rgba(16, 185, 129, 0.2);
  background: rgba(16, 185, 129, 0.04);
  padding: 12px;
  border-radius: 8px;
}

.commands-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: #6ee7b7;
  margin-bottom: 4px;
}

.local-badge {
  font-size: 10px;
  font-weight: 700;
  background: rgba(16, 185, 129, 0.15);
  color: #6ee7b7;
  border: 1px solid rgba(16, 185, 129, 0.3);
  padding: 1px 6px;
  border-radius: 4px;
  letter-spacing: 0.3px;
}

.commands-hint {
  font-size: 11px;
  color: #6b7280;
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
  color: #6ee7b7 !important;
  display: flex;
  align-items: center;
  gap: 6px;
}

.text-amber-dim {
  color: #fbbf24 !important;
  display: flex;
  align-items: center;
  gap: 6px;
}

.default-hint {
  font-size: 10px;
  font-weight: 400;
  color: #6b7280;
  font-style: italic;
  font-family: monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 160px;
}

.code-input {
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid rgba(255, 255, 255, 0.1);
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
  border-color: #10b981;
}
</style>
