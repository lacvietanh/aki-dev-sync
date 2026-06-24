// @docs docs/arch/usage-claudecode.md
// @docs docs/arch/usage-antigravity.md
// @docs docs/research/claude-usage-1.2.x-analyze.md
import { ref, watch, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { refreshSettings, manualRefreshCount } from '../store/refreshStore';

export function useAgentUsage(agentName, hostRef) {
  const data = ref(null);
  const loading = ref(false);
  const error = ref(null);
  const stale = ref(false);

  let pollTimer = null;
  let provisioned = false;
  let initialSyncDone = false;
  let staleResetSyncDone = false;
  let isSyncing = false;
  let isChecking = false;

  const provision = async () => {
    if (!hostRef.value || provisioned) return;
    provisioned = true; // mark before await to prevent concurrent calls
    try {
      await invoke('provision_agent_usage', { agentName, host: hostRef.value });
    } catch (e) {
      provisioned = false; // reset so next empty result can retry
      console.error(`Failed to provision ${agentName}:`, e);
    }
  };

  const checkUsage = async () => {
    if (!hostRef.value) {
      data.value = null;
      error.value = null;
      return;
    }
    if (isChecking) return;
    isChecking = true;

    loading.value = true;
    error.value = null;

    try {
      const hadData = data.value !== null;
      const res = await invoke('get_agent_usage', { agentName, host: hostRef.value });
      if (res) {
        try {
          data.value = JSON.parse(res.content);
          staleResetSyncDone = false;

          const mtime = parseInt(res.file_modified_at, 10);
          const fiveHour = data.value?.rate_limits?.five_hour;
          const resetIsPast = fiveHour?.resets_at > 0 && (Date.now() / 1000) > fiveHour.resets_at;
          stale.value = resetIsPast || (mtime > 0 && (Date.now() / 1000 - mtime) > 600);

          // Auto force-sync on first load if resets_at is 0 for Claude Code
          if (agentName === 'claudecode' && !initialSyncDone) {
            if (!fiveHour || fiveHour.resets_at === 0) {
              initialSyncDone = true;
              forceSync();
            }
          }
        } catch (e) {
          console.error(`Failed to parse ${agentName} usage JSON:`, e);
          error.value = "Invalid usage data format.";
        }
      } else {
        data.value = null;
        provision(); // fire-and-forget: set up remote on first empty result
        // Auto force-sync on first load if cache file doesn't exist yet for Claude Code
        if (agentName === 'claudecode' && !initialSyncDone) {
          initialSyncDone = true;
          forceSync();
        } else if (agentName === 'claudecode' && hadData && !staleResetSyncDone) {
          // Transition: had data → now null = STALE_RESET. Auto-recover once.
          staleResetSyncDone = true;
          forceSync();
        }
      }
    } catch (e) {
      console.error(`Error fetching ${agentName} usage:`, e);
      error.value = e.toString();
    } finally {
      loading.value = false;
      isChecking = false;
    }
  };

  const forceSync = async () => {
    if (!hostRef.value || isSyncing) return;
    isSyncing = true;
    loading.value = true;
    error.value = null;
    try {
      const raw = await invoke('force_sync_agent_usage', { agentName, host: hostRef.value });
      try {
        const diag = JSON.parse(raw);
        console.log(`[ForceSync] ${agentName}@${hostRef.value}:`, diag);
      } catch (_) {
        console.log(`[ForceSync] ${agentName}@${hostRef.value}: raw=`, raw);
      }
      await checkUsage();
    } catch (e) {
      console.error(`Error force syncing ${agentName}:`, e);
      error.value = e.toString();
      loading.value = false;
    } finally {
      isSyncing = false;
    }
  };

  function restartPollTimer() {
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = null;
    const s = refreshSettings.value.usage_interval_s;
    if (hostRef.value && s > 0) {
      pollTimer = setInterval(checkUsage, s * 1000);
    }
  }

  watch(() => hostRef.value, (newHost) => {
    provisioned = false;
    initialSyncDone = false;
    staleResetSyncDone = false;
    isSyncing = false;
    isChecking = false;
    data.value = null;
    error.value = null;
    if (newHost) {
      checkUsage();
    }
    restartPollTimer();
  }, { immediate: true });

  watch(() => refreshSettings.value.usage_interval_s, restartPollTimer);
  watch(() => manualRefreshCount.value, () => {
    if (hostRef.value) {
      if (agentName === 'claudecode') {
        forceSync();
      } else {
        checkUsage();
      }
    }
  });

  onUnmounted(() => {
    if (pollTimer) clearInterval(pollTimer);
  });

  return {
    data,
    loading,
    error,
    stale,
    refresh: checkUsage,
    forceSync
  };
}
