import { ref, watch, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';

export function useAgentUsage(agentName, hostRef) {
  const data = ref(null);
  const loading = ref(false);
  const error = ref(null);
  const stale = ref(false);
  const provisioned = ref(false);

  let pollTimer = null;

  // Provision must be triggered explicitly by the user via `provision()`.
  // The poll path only reads — it never writes to the remote environment.
  const provision = async () => {
    if (!hostRef.value) return false;
    try {
      await invoke('provision_agent_usage', { agentName, host: hostRef.value });
      provisioned.value = true;
      return true;
    } catch (e) {
      console.error(`Failed to provision ${agentName}:`, e);
      return false;
    }
  };

  const checkUsage = async () => {
    if (!hostRef.value) {
      data.value = null;
      error.value = null;
      return;
    }

    loading.value = true;
    error.value = null;
    stale.value = false;

    try {
      const res = await invoke('get_agent_usage', { agentName, host: hostRef.value });
      if (res) {
        try {
          data.value = JSON.parse(res.content);
          
          // Check if data is stale (older than 10 minutes)
          const fetchedAt = parseInt(res.fetched_at, 10);
          const mtime = parseInt(res.file_modified_at, 10);
          
          if (fetchedAt && mtime) {
            // fetchedAt and mtime are in seconds
            if (fetchedAt - mtime > 600) {
              stale.value = true;
            }
          }
        } catch (e) {
          console.error(`Failed to parse ${agentName} usage JSON:`, e);
          error.value = "Invalid usage data format.";
        }
      } else {
        data.value = null;
      }
    } catch (e) {
      console.error(`Error fetching ${agentName} usage:`, e);
      error.value = e.toString();
    } finally {
      loading.value = false;
    }
  };

  const refresh = () => {
    checkUsage();
  };

  const forceSync = async () => {
    if (!hostRef.value) return;
    loading.value = true;
    error.value = null;
    try {
      await invoke('force_sync_agent_usage', { agentName, host: hostRef.value });
      await checkUsage();
    } catch (e) {
      console.error(`Error force syncing ${agentName}:`, e);
      error.value = e.toString();
      loading.value = false;
    }
  };

  watch(() => hostRef.value, (newHost) => {
    provisioned.value = false;
    data.value = null;
    error.value = null;
    if (pollTimer) clearInterval(pollTimer);
    if (newHost) {
      checkUsage();
      pollTimer = setInterval(checkUsage, 30000);
    }
  }, { immediate: true });

  onUnmounted(() => {
    if (pollTimer) clearInterval(pollTimer);
  });

  return {
    data,
    loading,
    error,
    stale,
    provisioned,
    provision,
    refresh,
    forceSync
  };
}
