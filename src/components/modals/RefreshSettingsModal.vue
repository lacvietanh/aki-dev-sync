<template>
  <BaseModal :show="show" @close="$emit('close')" container-style="max-width: 440px">
    <template #title>
      <i class="fa-solid fa-rotate-right mr-1"></i> Background Refresh Settings
    </template>
    <div class="modal-body">
      <p class="text-muted" style="font-size: 12px; margin-bottom: 16px;">
        Intervals in seconds. Set to <code>0</code> to disable a refresh type.
      </p>

      <div class="refresh-row">
        <div class="refresh-label">
          <i class="fa-solid fa-code-branch" style="color: #94a3b8;"></i>
          <div>
            <div class="refresh-title">Git Status</div>
            <div class="refresh-desc">Uncommitted changes and recent git log per project</div>
          </div>
        </div>
        <div class="refresh-input-group">
          <input type="number" min="0" step="10" v-model.number="local.git_interval_s" class="refresh-input" />
          <span class="refresh-unit">s</span>
        </div>
      </div>

      <div class="refresh-row">
        <div class="refresh-label">
          <i class="fa-solid fa-arrows-rotate" style="color: #f59e0b;"></i>
          <div>
            <div class="refresh-title">Push / Pull Check</div>
            <div class="refresh-desc">Detects file changes between local and remote — lights up Push or Pull when out of sync</div>
          </div>
        </div>
        <div class="refresh-input-group">
          <input type="number" min="0" step="10" v-model.number="local.remote_diff_interval_s" class="refresh-input" />
          <span class="refresh-unit">s</span>
        </div>
      </div>

      <div class="refresh-row">
        <div class="refresh-label">
          <i class="fa-solid fa-chart-bar" style="color: #818cf8;"></i>
          <div>
            <div class="refresh-title">Agent Usage</div>
            <div class="refresh-desc">Claude Code usage on remote host + Antigravity usage locally</div>
          </div>
        </div>
        <div class="refresh-input-group">
          <input type="number" min="0" step="10" v-model.number="local.usage_interval_s" class="refresh-input" />
          <span class="refresh-unit">s</span>
        </div>
      </div>
    </div>
    <div class="modal-footer">
      <div></div>
      <div>
        <button class="btn-secondary mr-1" @click="$emit('close')">Cancel</button>
        <button class="btn-save" @click="save"><i class="fa-solid fa-floppy-disk mr-1"></i> Save</button>
      </div>
    </div>
  </BaseModal>
</template>

<script setup>
import { reactive, watch } from 'vue'
import BaseModal from './BaseModal.vue'
import { refreshSettings } from '../../store/refreshStore'

const props = defineProps({ show: Boolean })
const emit = defineEmits(['close'])

const local = reactive({ ...refreshSettings.value })

watch(() => props.show, (v) => {
  if (v) Object.assign(local, refreshSettings.value)
})

function save() {
  refreshSettings.value = { ...local }
  emit('close')
}
</script>

<style scoped>
.refresh-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 0;
  border-bottom: 1px solid #1e2030;
}
.refresh-row:last-child { border-bottom: none; }

.refresh-label {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  flex: 1;
  font-size: 13px;
}
.refresh-label i { margin-top: 2px; width: 14px; flex-shrink: 0; }

.refresh-title { color: #e2e8f0; font-weight: 600; }
.refresh-desc { color: #64748b; font-size: 11px; margin-top: 2px; }

.refresh-input-group {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}
.refresh-input {
  width: 64px;
  text-align: right;
  background: #1a1a24;
  border: 1px solid #374151;
  color: #e2e8f0;
  border-radius: 4px;
  padding: 4px 6px;
  font-size: 13px;
}
.refresh-input:focus { outline: none; border-color: #3b82f6; }
.refresh-unit { color: #64748b; font-size: 12px; }
</style>
