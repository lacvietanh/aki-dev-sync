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
import { ref, computed, reactive, watch } from 'vue';
import AgentUsageSlot from './AgentUsageSlot.vue';
import { useSsh } from '../composables/useSsh';
import { useAgentUsage } from '../composables/useAgentUsage';
import { remoteModeEnabled } from '../store/remoteModeStore';
import { claudeMode } from '../store/claudeModeStore';

const { selectedSshHost } = useSsh();

// Three independent, toggleable usage sources shared by both display slots. Polling is
// driven purely by each source's own `enabled` flag (persisted), not by which slot (if
// any) currently has it selected for display — so a slot can show a source that's off
// (rendered as "Monitoring off" or last-known cached data by AgentUsage) without that
// implicitly turning it on, and turning a source on/off doesn't care who's looking at it.
// `lockedRef`, when provided, blocks manual toggle() calls (guarded again at the UI layer
// in AgentUsageSlot.vue) — used for Claude Code local monitoring, which reads straight from
// the native Anthropic account API/pricing and is meaningless once Proxy mode reroutes
// traffic elsewhere (see claudeModeStore.js).
function useToggleableSource(agentKey, resolveHost, storageKey, defaultEnabled, lockedRef = null) {
  const enabled = ref(
    localStorage.getItem(storageKey) !== null
      ? localStorage.getItem(storageKey) === 'true'
      : defaultEnabled
  );
  function toggle() {
    if (lockedRef?.value) return;
    enabled.value = !enabled.value;
    localStorage.setItem(storageKey, String(enabled.value));
  }
  const hostRef = computed(() => (enabled.value ? resolveHost() : null));
  const hook = useAgentUsage(agentKey, hostRef);
  return reactive({ enabled, toggle, locked: lockedRef || computed(() => false), ...hook });
}

// Local sources cost nothing (no SSH round trip) — on by default, each with its own
// independent power switch inside the LOCAL tab.
const ag = useToggleableSource('antigravity', () => 'local', 'aki-src-ag-enabled', true);
const ccLocalLocked = computed(() => claudeMode.value === 'proxy');
const ccLocal = useToggleableSource('claudecode', () => 'local', 'aki-src-cclocal-enabled', true, ccLocalLocked);

// Proxy mode ON forces monitoring off (locked, can't be manually re-enabled — see toggle()
// above). Proxy mode OFF just unlocks the switch; it does NOT auto-restore a prior enabled
// state, by design, to keep this behavior simple and predictable — the user turns it back
// on themselves, same as any other fresh "off" state.
watch(claudeMode, (mode) => {
  if (mode === 'proxy') ccLocal.enabled = false;
});

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
  /* Fixed viewport (docs/plan/done/narrow-mode-and-ux-1.14.0.md §B1): the usage area used to
     grow/shrink with content, jumping the whole UI on load/error/account switch. 161px
     measured from the bottom edge of the 42px titlebar covers the tallest normal (non-error)
     state — two CC bars or the AG 4-circle layout — with a small buffer; content beyond that
     scrolls instead of pushing layout below it. */
  height: 161px;
  overflow-y: auto;
  overflow-x: hidden;
  box-sizing: border-box;
}

/* Narrower, low-contrast scrollbar than the app-wide 6px rule (main.css) — this element only. */
.agent-usage-section::-webkit-scrollbar {
  width: 4px;
}
.agent-usage-section::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.1);
  border-radius: 4px;
}
.agent-usage-section::-webkit-scrollbar-thumb:hover {
  background: rgba(255, 255, 255, 0.2);
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

/* Horizontal padding/gaps here were sized for the wide layout — tighten them at narrow so the
   LOCAL/REMOTE columns get more of the scarce width instead of losing it to whitespace. */
@media (max-width: 700px) {
  .agent-usage-section {
    padding: 6px 2px;
  }

  .usage-split-layout {
    gap: 2px;
  }

  .column-divider {
    margin: 0;
  }
}
</style>
