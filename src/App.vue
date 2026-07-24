<template>
  <div class="dashboard-layout">
    <!-- The whole dashboard mounts only when `ready` (host: always; companion: relay socket open).
         On an unpaired/still-connecting phone these components must NOT mount — every store is empty
         and their onMounted `invoke`s would fire over a closed socket. PairingGate covers that gap.
         See useCompanionPairing.js `ready`. -->
    <template v-if="ready">
      <div class="dashboard-top">
        <AppHeader />
        <AgentUsageSection />
        <ProjectTable />
      </div>

      <AppConsole />

      <ProjectConfigModal />
      <SshConfigModal />
      <GitModal />
      <IntroModal />
      <ProjectTasksModal />
    </template>

    <!-- Companion-only: shown whenever NOT ready (enter-code, or connecting…). Renders nothing on
         the host. Last in the tree so it paints over everything. -->
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
import ProjectConfigModal from './components/modals/ProjectConfigModal.vue';
import SshConfigModal from './components/modals/SshConfigModal.vue';
import GitModal from './components/modals/GitModal.vue';
import IntroModal from './components/modals/IntroModal.vue';
import ProjectTasksModal from './components/modals/ProjectTasksModal.vue';
import PairingGate from './components/PairingGate.vue';

import { useProjects } from './composables/useProjects';
import { useSsh } from './composables/useSsh';
import { initGlobalNote } from './composables/useGlobalNote';
import { refreshClaudeMode } from './store/claudeModeStore';
import { refreshProjectIcons } from './store/projectStore';
import { initRemote } from './services';
import { onHostBoot } from './utils/scheduler';
import { useCompanionPairing } from './composables/useCompanionPairing';

const { ready } = useCompanionPairing();
const { loadData } = useProjects();
const { sshHosts } = useSsh();

const LEGACY_BASELINE_CLEANUP_KEY = 'aki-legacy-baseline-cleanup-v1';

onMounted(() => {
  // Remote-control bring-up (docs/plan/remote-control.md §1). Idempotent, safe on both roles:
  // host opens its relay socket + broadcasts store state; companion mirrors incoming state.
  // MUST run before the producers below so the host's first broadcast reflects loaded data.
  initRemote();

  // Seam P (§5): the whole boot sequence is *production* — it reads the disk, hits the network and
  // mutates state. A companion must run none of it: its copy of every store arrives through the
  // mirror, and at this point its socket is not even open yet, so these calls would only produce a
  // burst of failed RPCs. On the host `onHostBoot` runs the callback immediately, unchanged.
  onHostBoot(() => {
    loadData(sshHosts, false);
    initGlobalNote();
    refreshClaudeMode();
    refreshProjectIcons(); // ICON-1: fills projectStore.projectIcons, mirrored to the phone

    if (localStorage.getItem(LEGACY_BASELINE_CLEANUP_KEY) !== 'true') {
      invoke('cleanup_legacy_baselines')
        .then(() => localStorage.setItem(LEGACY_BASELINE_CLEANUP_KEY, 'true'))
        .catch(() => {});
    }
  });
});
</script>
