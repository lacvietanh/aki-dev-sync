<template>
  <div class="dashboard-layout">
    <div class="dashboard-top">
      <AppHeader />
      <AgentUsageSection />
      <ProjectTable />
    </div>
    
    <AppConsole />

    <SpecialPushModal />
    <ProjectConfigModal />
    <SshConfigModal />
    <GitModal />
    <IntroModal />
    <ProjectTasksModal />
  </div>
</template>

<script setup>
import { onMounted } from 'vue';
import AppHeader from './components/AppHeader.vue';
import AgentUsageSection from './components/AgentUsageSection.vue';
import ProjectTable from './components/ProjectTable.vue';
import AppConsole from './components/AppConsole.vue';
import SpecialPushModal from './components/modals/SpecialPushModal.vue';
import ProjectConfigModal from './components/modals/ProjectConfigModal.vue';
import SshConfigModal from './components/modals/SshConfigModal.vue';
import GitModal from './components/modals/GitModal.vue';
import IntroModal from './components/modals/IntroModal.vue';
import ProjectTasksModal from './components/modals/ProjectTasksModal.vue';

import { useProjects } from './composables/useProjects';
import { useSsh } from './composables/useSsh';

const { loadData } = useProjects();
const { sshHosts } = useSsh();

onMounted(() => {
  loadData(sshHosts, false);
});
</script>
