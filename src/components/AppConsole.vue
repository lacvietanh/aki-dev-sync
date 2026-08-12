<!--
  The bottom dock: two independent, always-visible stacks (TerminalStack above LogStack) instead of a single panel tabbed between LOG and TERMINAL. Each stack owns its own collapse state (logStore.isLogExpanded / useTerminalPanel.js's terminalStackCollapsed) and, under the sum height model, its own length and its own resize handle (DockStack.vue). What is left here is the flex container, the dock's summed height, and the one cross-stack concern (Esc) neither stack should own alone.
-->
<template>
  <div
    class="dashboard-bottom"
    :class="{ 'is-dragging': dockDragging }"
    :style="{ height: dockHeightCss }"
    @transitionrun="onDockTransition"
    @transitionend="onDockTransition"
    @transitioncancel="onDockTransition"
  >
    <TerminalStack />
    <LogStack />
  </div>
</template>

<script setup>
import { onMounted, onUnmounted } from 'vue';
import { isLogExpanded, activeLogProjectId } from '../store/logStore';
import { dockAnimating, dockDragging, dockHeightCss } from '../composables/useDockLayout';
import TerminalStack from './dock/TerminalStack.vue';
import LogStack from './dock/LogStack.vue';

// Sets useDockLayout's dockAnimating from the live transition on this container (height) and its bubbling stacks (flex-grow/flex-basis).
const DOCK_ANIM_PROPS = new Set(['height', 'flex-grow', 'flex-basis']);
let dockAnimClear = 0;
function onDockTransition(e) {
  if (!DOCK_ANIM_PROPS.has(e.propertyName)) return;
  if (e.type === 'transitionrun') {
    clearTimeout(dockAnimClear);
    dockAnimating.value = true;
    return;
  }
  // end/cancel: the properties end within the same frame, so coalesce them into one clear.
  clearTimeout(dockAnimClear);
  dockAnimClear = setTimeout(() => { dockAnimating.value = false; }, 60);
}

// Esc closes the log stack from anywhere, except while a modal is open or focus is inside a terminal (xterm needs Esc for vim/less).
function handleEsc(e) {
  if (e.key !== 'Escape') return;
  if (document.querySelector('.modal-overlay')) return;
  if (document.activeElement?.closest?.('.pty-terminal')) return;
  if (!isLogExpanded.value) return;
  activeLogProjectId.value = null;
  isLogExpanded.value = false;
}

onMounted(() => window.addEventListener('keydown', handleEsc, true));
onUnmounted(() => {
  window.removeEventListener('keydown', handleEsc, true);
  clearTimeout(dockAnimClear);
});
</script>
