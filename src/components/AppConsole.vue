<!--
  The bottom dock: two independent, always-visible stacks (TerminalStack above LogStack) instead
  of a single panel tabbed between LOG and TERMINAL. Each stack owns its own collapse state
  (logStore.isLogExpanded / useTerminalPanel.js's terminalStackCollapsed); this component is just
  the flex container plus the one cross-stack concern (Esc) that neither stack should own alone.
-->
<template>
  <div class="dashboard-bottom" :class="{ 'is-all-collapsed': allCollapsed }">
    <TerminalStack />
    <LogStack />
  </div>
</template>

<script setup>
import { computed, onMounted, onUnmounted } from 'vue';
import { isLogExpanded, activeLogProjectId } from '../store/logStore';
import { terminalStackCollapsed } from '../composables/useTerminalPanel';
import TerminalStack from './dock/TerminalStack.vue';
import LogStack from './dock/LogStack.vue';

// The SAME two refs TerminalStack.vue / LogStack.vue bind their own chevrons to — read directly,
// since each stack's collapse state has exactly one owner.
const allCollapsed = computed(() => terminalStackCollapsed.value && !isLogExpanded.value);

// Esc closes the log stack from anywhere in the app EXCEPT while a modal is open or focus is
// inside a terminal (xterm needs Esc for vim/less/etc. — a capture-phase listener above the
// terminal would otherwise steal it before the shell ever sees it).
function handleEsc(e) {
  if (e.key !== 'Escape') return;
  if (document.querySelector('.modal-overlay')) return;
  if (document.activeElement?.closest?.('.pty-terminal')) return;
  if (!isLogExpanded.value) return;
  activeLogProjectId.value = null;
  isLogExpanded.value = false;
}

onMounted(() => window.addEventListener('keydown', handleEsc, true));
onUnmounted(() => window.removeEventListener('keydown', handleEsc, true));
</script>
