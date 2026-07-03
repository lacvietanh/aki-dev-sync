<template>
  <div class="dashboard-layout">
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
  </div>
</template>

<script setup>
import { onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import AppHeader from './components/AppHeader.vue';
import AgentUsageSection from './components/AgentUsageSection.vue';
import ProjectTable from './components/ProjectTable.vue';
import AppConsole from './components/AppConsole.vue';
import ProjectConfigModal from './components/modals/ProjectConfigModal.vue';
import SshConfigModal from './components/modals/SshConfigModal.vue';
import GitModal from './components/modals/GitModal.vue';
import IntroModal from './components/modals/IntroModal.vue';
import ProjectTasksModal from './components/modals/ProjectTasksModal.vue';

import { useProjects } from './composables/useProjects';
import { useSsh } from './composables/useSsh';
import { initGlobalNote } from './composables/useGlobalNote';

const { loadData } = useProjects();
const { sshHosts } = useSsh();

const LEGACY_BASELINE_CLEANUP_KEY = 'aki-legacy-baseline-cleanup-v1';

onMounted(() => {
  loadData(sshHosts, false);
  initGlobalNote();

  if (localStorage.getItem(LEGACY_BASELINE_CLEANUP_KEY) !== 'true') {
    invoke('cleanup_legacy_baselines')
      .then(() => localStorage.setItem(LEGACY_BASELINE_CLEANUP_KEY, 'true'))
      .catch(() => {});
  }
});
</script>
