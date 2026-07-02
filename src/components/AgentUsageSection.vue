<template>
  <div class="agent-usage-section">
    <div class="usage-split-layout">
      <AgentUsageSlot
        slot-id="A"
        default-top-tab="local"
        default-local-sub="ag"
        :ag="ag"
        :cc-local="ccLocal"
        :cc-remote="ccRemote"
      />

      <div class="column-divider"></div>

      <AgentUsageSlot
        slot-id="B"
        default-top-tab="remote"
        default-local-sub="ag"
        :ag="ag"
        :cc-local="ccLocal"
        :cc-remote="ccRemote"
      />
    </div>
  </div>
</template>

<script setup>
import { ref, computed, reactive } from 'vue';
import AgentUsageSlot from './AgentUsageSlot.vue';
import { useSsh } from '../composables/useSsh';
import { useAgentUsage } from '../composables/useAgentUsage';
import { remoteModeEnabled } from '../store/remoteModeStore';

const { selectedSshHost } = useSsh();

// Three independent, toggleable usage sources shared by both display slots. Polling is
// driven purely by each source's own `enabled` flag (persisted), not by which slot (if
// any) currently has it selected for display — so a slot can show a source that's off
// (rendered as "Monitoring off" or last-known cached data by AgentUsage) without that
// implicitly turning it on, and turning a source on/off doesn't care who's looking at it.
function useToggleableSource(agentKey, resolveHost, storageKey, defaultEnabled) {
  const enabled = ref(
    localStorage.getItem(storageKey) !== null
      ? localStorage.getItem(storageKey) === 'true'
      : defaultEnabled
  );
  function toggle() {
    enabled.value = !enabled.value;
    localStorage.setItem(storageKey, String(enabled.value));
  }
  const hostRef = computed(() => (enabled.value ? resolveHost() : null));
  const hook = useAgentUsage(agentKey, hostRef);
  return reactive({ enabled, toggle, ...hook });
}

// Local sources cost nothing (no SSH round trip) — on by default, each with its own
// independent power switch inside the LOCAL tab.
const ag = useToggleableSource('antigravity', () => 'local', 'aki-src-ag-enabled', true);
const ccLocal = useToggleableSource('claudecode', () => 'local', 'aki-src-cclocal-enabled', true);

// Remote has no switch of its own — it is entirely governed by the single global Remote
// Mode switch in AppHeader (`remoteModeStore`), same as project pull/push/select/open and
// the background remote-diff checks. `enabled` here just mirrors that global flag so
// AgentUsage/AgentUsageSlot can read it the same way they read the other two sources'
// `enabled` (cached-badge / "Monitoring off" behavior, disabling the host picker, etc.).
const ccRemoteHostRef = computed(() => (remoteModeEnabled.value ? selectedSshHost.value : null));
const ccRemote = reactive({ enabled: remoteModeEnabled, ...useAgentUsage('claudecode', ccRemoteHostRef) });
</script>

<style scoped>
.agent-usage-section {
  background: rgba(22, 22, 26, 0.6);
  border-bottom: 1px solid var(--border-color);
  padding: 6px 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.usage-split-layout {
  display: flex;
  gap: 12px;
  align-items: stretch;
}

.column-divider {
  width: 1px;
  background: rgba(255, 255, 255, 0.05);
  margin: 0 4px;
}
</style>
