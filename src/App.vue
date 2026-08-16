<template>
  <div class="dashboard-layout">
    <!-- Mounts only when ready (host: always; companion: relay socket open). See useCompanionPairing.js. -->
    <template v-if="ready">
      <!-- AppHeader spans full window width as titlebar outside dashboard-left columns. -->
      <AppHeader />

      <div class="dashboard-main" :class="{ 'is-right-dock': rightDockActive }">
        <!-- dashboard-left: project list column. Capped at 440px in right-dock, full-width in narrow. -->
        <div class="dashboard-left">
          <AgentUsageSection />
          <ProjectTable />
          <!-- Global Event Log: rendered in main column in right-dock mode, inside AppConsole in narrow mode. -->
          <LogStack v-if="rightDockActive" />
        </div>

        <AppConsole />
      </div>

      <ProjectConfigModal />
      <SshConfigModal />
      <GitModal />
      <IntroModal />
      <ProjectTasksModal />
      <ExternalTerminalsModal />

      <!-- Mirrored dialog host for pendingDialog across both roles (docs/plan/done/remote-control.md §3.4). -->
      <DialogHost />
    </template>

    <!-- Companion-only overlay when not ready; last in tree to paint over content. -->
    <PairingGate />
  </div>
</template>

<script setup>
import { onMounted } from 'vue';
import { invoke } from './utils/tauri';
import AppHeader from './components/AppHeader.vue';
import AgentUsageSection from './components/AgentUsageSection.vue';
import ProjectTable from './components/ProjectTable.vue';
import AppConsole from './components/AppConsole.vue';
import LogStack from './components/dock/LogStack.vue';
import ProjectConfigModal from './components/modals/ProjectConfigModal.vue';
import SshConfigModal from './components/modals/SshConfigModal.vue';
import GitModal from './components/modals/GitModal.vue';
import IntroModal from './components/modals/IntroModal.vue';
import ProjectTasksModal from './components/modals/ProjectTasksModal.vue';
import ExternalTerminalsModal from './components/modals/ExternalTerminalsModal.vue';
import DialogHost from './components/DialogHost.vue';
import PairingGate from './components/PairingGate.vue';

import { useProjects } from './composables/useProjects';
import { useSsh } from './composables/useSsh';
import { initGlobalNote } from './composables/useGlobalNote';
import { initTerminalTabs } from './composables/useTerminalTabs';
import { refreshClaudeMode } from './store/claudeModeStore';
import { refreshProjectIcons } from './store/projectStore';
import { initRemote } from './services';
import { onHostBoot } from './utils/scheduler';
import { useCompanionPairing } from './composables/useCompanionPairing';
import { useVisualViewportHeight } from './composables/useVisualViewportHeight';
import { rightDockActive } from './composables/useRightDockLayout';

useVisualViewportHeight();

const { ready } = useCompanionPairing();
const { loadData } = useProjects();
const { sshHosts } = useSsh();

const LEGACY_BASELINE_CLEANUP_KEY = 'aki-legacy-baseline-cleanup-v1';

onMounted(() => {
  // Remote-control bring-up (docs/plan/done/remote-control.md §1); must run before boot producers.
  initRemote();

  // Host-only boot sequence (Seam P §5); companion receives store state via remote mirror.
  onHostBoot(() => {
    loadData(sshHosts, false);
    initGlobalNote();
    initTerminalTabs();
    refreshClaudeMode();
    refreshProjectIcons();

    if (localStorage.getItem(LEGACY_BASELINE_CLEANUP_KEY) !== 'true') {
      invoke('cleanup_legacy_baselines')
        .then(() => localStorage.setItem(LEGACY_BASELINE_CLEANUP_KEY, 'true'))
        .catch(() => {});
    }
  });
});
</script>
