<!--
  The bottom dock: two independent, always-visible stacks (TerminalStack above LogStack) instead
  of a single panel tabbed between LOG and TERMINAL. Each stack owns its own collapse state
  (logStore.isLogExpanded / useTerminalPanel.js's terminalStackCollapsed); this component is just
  the flex container plus the one cross-stack concern (Esc) that neither stack should own alone.
-->
<template>
  <div
    class="dashboard-bottom"
    :class="{ 'is-all-collapsed': allCollapsed, 'is-dragging': dockDragging }"
    :style="{ height: allCollapsed ? null : dockHeightCss }"
  >
    <!--
      Splitter. Not rendered while both stacks are collapsed: the dock is then two header rows sized
      by their own content (`.is-all-collapsed { height: auto }`), so a drag would set a height that
      rule immediately overrides — a handle that visibly does nothing (METHOD-flow-audit: a gesture
      with no effect is a flow break, not a harmless extra).

      Pointer events, not mouse: one code path covers the trackpad, a mouse and the phone's touch
      screen, and `setPointerCapture` is what keeps the drag alive when the pointer outruns this
      3px-tall element — which, at any real drag speed, it always does.
    -->
    <div
      v-if="!allCollapsed"
      class="dock-splitter"
      role="separator"
      aria-orientation="horizontal"
      title="Drag to resize the panel · double-click to reset"
      @pointerdown="onSplitterDown"
      @pointermove="onSplitterMove"
      @pointerup="onSplitterUp"
      @pointercancel="onSplitterUp"
      @dblclick="resetDockHeight"
    ></div>
    <TerminalStack />
    <LogStack />
  </div>
</template>

<script setup>
import { onMounted, onUnmounted } from 'vue';
import { isLogExpanded, activeLogProjectId } from '../store/logStore';
import {
  dockAllCollapsed as allCollapsed,
  dockDragging,
  dockHeightCss,
  resetDockHeight,
  setDockHeightFromPointer,
} from '../composables/useDockLayout';
import TerminalStack from './dock/TerminalStack.vue';
import LogStack from './dock/LogStack.vue';

// The dock's own `transition: height` is what makes COLLAPSE feel smooth; during a drag it makes
// the panel chase the pointer instead of tracking it, so `is-dragging` turns it off for the drag's
// duration only.
function onSplitterDown(e) {
  e.preventDefault();
  dockDragging.value = true;
  e.currentTarget.setPointerCapture(e.pointerId);
}

function onSplitterMove(e) {
  if (!dockDragging.value) return;
  setDockHeightFromPointer(e.clientY);
}

function onSplitterUp(e) {
  if (!dockDragging.value) return;
  dockDragging.value = false;
  // Releasing capture can throw if the pointer is already gone (window blur mid-drag); the drag is
  // over either way, and `dockDragging` above is what actually ends it.
  try {
    e.currentTarget.releasePointerCapture(e.pointerId);
  } catch {
    /* already released */
  }
}

// Double-click to reset (useDockLayout.js's resetDockHeight) is the convention every splitter in
// VS Code, Finder and the browser devtools shares — worth having precisely because a mis-drag is
// easy and undoing it by hand is not.

// `allCollapsed` is useDockLayout.js's `dockAllCollapsed` (aliased on import): the same condition
// the MAXIMIZE button hides itself on, defined once so the two cannot drift.

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
