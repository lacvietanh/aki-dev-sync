<!--
  The bottom dock: two independent stacks (TerminalStack above LogStack) in narrow mode. Each stack owns its own collapse state and resize handle (DockStack.vue).

  RIGHT-DOCK (TERM-STACK-R): above useRightDockLayout.js's breakpoint (900px) this box becomes a right-side column (flex:1). In this mode LogStack is rendered in App.vue's dashboard-left (so the terminal column is terminal-only), and the splitter is hidden via CSS. AppHeader sits above dashboard-main so both columns start below the titlebar — preventing the dock header from occupying the titlebar zone and blocking ⋮/COLLAPSE clicks.
-->
<template>
  <div
    class="dashboard-bottom"
    :class="{ 'is-dragging': dockDragging }"
    :style="consoleStyle"
    @transitionrun="onDockTransition"
    @transitionend="onDockTransition"
    @transitioncancel="onDockTransition"
  >
    <TerminalStack />
    <LogStack v-if="!rightDockActive" />
  </div>
</template>

<script setup>
import { computed, onUnmounted } from 'vue';
import { isLogExpanded, activeLogProjectId } from '../store/logStore';
import { dockAnimating, dockDragging, dockHeightCss } from '../composables/useDockLayout';
import { rightDockActive } from '../composables/useRightDockLayout';
import TerminalStack from './dock/TerminalStack.vue';
import LogStack from './dock/LogStack.vue';

// In narrow mode: height from useDockLayout.js. In right-dock mode: flex:1 in CSS fills the remaining space.
const consoleStyle = computed(() =>
  rightDockActive.value ? {} : { height: dockHeightCss.value }
);

// Sets dockAnimating from the live transition on this container (height) and its bubbling stacks (flex-grow/flex-basis).
const DOCK_ANIM_PROPS = new Set(['height', 'flex-grow', 'flex-basis']);
let dockAnimClear = 0;
function onDockTransition(e) {
  if (!DOCK_ANIM_PROPS.has(e.propertyName)) return;
  if (e.type === 'transitionrun') {
    clearTimeout(dockAnimClear);
    dockAnimating.value = true;
    return;
  }
  clearTimeout(dockAnimClear);
  dockAnimClear = setTimeout(() => { dockAnimating.value = false; }, 60);
}

// Esc collapses the log stack from anywhere, except inside a modal or terminal (xterm needs Esc for vim/less).
function handleEsc(e) {
  if (e.key !== 'Escape') return;
  if (document.querySelector('.modal-overlay')) return;
  if (document.activeElement?.closest?.('.pty-terminal')) return;
  if (!isLogExpanded.value) return;
  activeLogProjectId.value = null;
  isLogExpanded.value = false;
}

window.addEventListener('keydown', handleEsc, true);
onUnmounted(() => {
  window.removeEventListener('keydown', handleEsc, true);
  clearTimeout(dockAnimClear);
});
</script>
