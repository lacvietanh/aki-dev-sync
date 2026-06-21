<template>
  <div v-if="showConfigModal && editingProject" class="modal-overlay">
    <div class="modal-container">
      <div class="modal-header">
        <h2><i class="fa-solid fa-gear mr-1"></i> Configuration: {{ editingProject.name }}</h2>
        <button class="btn-close-modal" @click="closeConfig"><i class="fa-solid fa-xmark"></i></button>
      </div>
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
            <label>Production URL <i class="fa-solid fa-circle-info help-icon" title="Mục đích chỉ để tiện mở ra bằng nút icon web cạnh tên dự án"></i></label>
            <input type="text" v-model="editingProject.production_url" placeholder="https://..." />
          </div>
        </div>

        <!-- PUSH GROUP -->
        <div class="full-width config-group push-group mb-1 mt-1">
          <h4 class="group-title text-amber"><i class="fa-solid fa-arrow-up mr-1"></i> PUSH CONFIGURATION (Local → Remote)</h4>
          <div class="form-group mb-1">
            <label class="text-amber">PUSH Excludes (1 per line) - Usually pushes .git/</label>
            <textarea class="large-textarea border-push" v-model="pushExcludesText" rows="3"></textarea>
          </div>
          <div class="form-group">
            <div style="cursor: pointer; display: inline-block; margin-bottom: 6px;" @click="togglePushScripts = !togglePushScripts">
              <label :class="hasPushScripts ? 'text-amber' : 'text-muted'" :style="{ cursor: 'pointer', fontSize: '11px', fontWeight: hasPushScripts ? '800' : '600' }">
                <i class="fa-solid fa-code mr-1"></i> Pre & Post Push Scripts
                <i :class="togglePushScripts ? 'fa-solid fa-chevron-up' : 'fa-solid fa-chevron-down'" style="margin-left: 4px;"></i>
              </label>
            </div>
            <div v-show="togglePushScripts" style="display: flex; flex-direction: column; gap: 12px;">
              <div class="form-group">
                <label class="text-amber" style="opacity: 0.8">Pre-Push Script</label>
                <textarea class="large-textarea code-font border-push" v-model="editingProject.hooks.pre_push_cmd" rows="2"></textarea>
              </div>
              <div class="form-group">
                <label class="text-amber" style="opacity: 0.8">Post-Push Script</label>
                <textarea class="large-textarea code-font border-push" v-model="editingProject.hooks.post_push_cmd" rows="2"></textarea>
              </div>
            </div>
          </div>
        </div>

        <!-- PULL GROUP -->
        <div class="full-width config-group pull-group mb-1 mt-1">
          <h4 class="group-title text-blue"><i class="fa-solid fa-arrow-down mr-1"></i> PULL CONFIGURATION (Remote → Local)</h4>
          <div class="form-group mb-1">
            <label class="text-blue">PULL Excludes (1 per line) - Usually includes .git/</label>
            <textarea class="large-textarea border-pull" v-model="pullExcludesText" rows="3"></textarea>
          </div>
          <div class="form-group">
            <div style="cursor: pointer; display: inline-block; margin-bottom: 6px;" @click="togglePullScripts = !togglePullScripts">
              <label :class="hasPullScripts ? 'text-blue' : 'text-muted'" :style="{ cursor: 'pointer', fontSize: '11px', fontWeight: hasPullScripts ? '800' : '600' }">
                <i class="fa-solid fa-code mr-1"></i> Pre & Post Pull Scripts
                <i :class="togglePullScripts ? 'fa-solid fa-chevron-up' : 'fa-solid fa-chevron-down'" style="margin-left: 4px;"></i>
              </label>
            </div>
            <div v-show="togglePullScripts" style="display: flex; flex-direction: column; gap: 12px;">
              <div class="form-group">
                <label class="text-blue" style="opacity: 0.8">Pre-Pull Script</label>
                <textarea class="large-textarea code-font border-pull" v-model="editingProject.hooks.pre_pull_cmd" rows="2"></textarea>
              </div>
              <div class="form-group">
                <label class="text-blue" style="opacity: 0.8">Post-Pull Script</label>
                <textarea class="large-textarea code-font border-pull" v-model="editingProject.hooks.post_pull_cmd" rows="2"></textarea>
              </div>
            </div>
          </div>
        </div>

        <div class="form-group full-width hooks-section mt-1">
          <div class="checkbox-group mb-0">
            <input type="checkbox" id="run-remote-modal" v-model="editingProject.hooks.run_hooks_on_remote" />
            <label for="run-remote-modal">Execute hooks on Remote Host via SSH (uncheck for Local Shell)</label>
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
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useProjects } from '../../composables/useProjects';
import { useSsh } from '../../composables/useSsh';

const { showConfigModal, editingProject, closeConfig, saveConfig, confirmRemove } = useProjects();
const { sshHosts } = useSsh();

const togglePushScripts = ref(false);
const togglePullScripts = ref(false);

const hasPushScripts = computed(() => {
  return !!(editingProject.value?.hooks?.pre_push_cmd?.trim() || editingProject.value?.hooks?.post_push_cmd?.trim());
});

const hasPullScripts = computed(() => {
  return !!(editingProject.value?.hooks?.pre_pull_cmd?.trim() || editingProject.value?.hooks?.post_pull_cmd?.trim());
});

const pullExcludesText = computed({
  get() { return editingProject.value?.pull_excludes ? editingProject.value.pull_excludes.join("\n") : ""; },
  set(val) { if(editingProject.value) editingProject.value.pull_excludes = val.split("\n").map(s => s.trim()).filter(s => s !== ""); }
});

const pushExcludesText = computed({
  get() { return editingProject.value?.push_excludes ? editingProject.value.push_excludes.join("\n") : ""; },
  set(val) { if(editingProject.value) editingProject.value.push_excludes = val.split("\n").map(s => s.trim()).filter(s => s !== ""); }
});

function handleEsc(e) {
  if (e.key === 'Escape' && showConfigModal.value) {
    closeConfig();
  }
}

onMounted(() => window.addEventListener('keydown', handleEsc, true));
onUnmounted(() => window.removeEventListener('keydown', handleEsc, true));
</script>
