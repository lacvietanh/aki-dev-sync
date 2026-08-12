<!--
  Reusable base for one dock panel ("stack"). Collapse STATE is owned by each caller (logStore.isLogExpanded for LogStack.vue, useTerminalPanel.js's terminalStackCollapsed for TerminalStack.vue); this renders the chrome around it, emits update:collapsed, and owns this stack's resize handle.

  The handle lives here, not in each specialization, because two stacks would mean two identical sets of pointer handlers (pattern.A5). That is why a presentational base imports the geometry module: `stackKey` is this stack's identity in useDockLayout.js's per-stack length map.

  Reuses the existing .terminal-header / .terminal-title / .terminal-actions classes and the same 3px .dock-splitter the single dock-level splitter used before the sum model (Extreme Narrow, CLAUDE.md).
-->
<template>
  <!-- Sibling, not child: `.dock-stack { overflow: hidden }` would clip this handle's negative margin and its 11px hit area. None while collapsed — the length is then a fixed header row. -->
  <div
    v-if="!collapsed"
    class="dock-splitter"
    role="separator"
    aria-orientation="horizontal"
    title="Drag to resize this panel · double-click to reset"
    @pointerdown="onSplitterDown"
    @pointermove="onSplitterMove"
    @pointerup="onSplitterUp"
    @pointercancel="onSplitterUp"
    @dblclick="resetStackHeight(stackKey)"
  ></div>
  <div ref="stackEl" class="dock-stack" :style="dockStackFlex(stackKey)">
    <div class="terminal-header">
      <div class="terminal-title"><slot name="title"></slot></div>
      <div class="terminal-actions">
        <slot name="actions"></slot>
        <button
          class="btn-tech btn-tech-secondary btn-terminal-action"
          @click="$emit('update:collapsed', !collapsed)"
          :title="collapsed ? 'Expand panel' : 'Collapse panel'"
        >
          <i class="fa-solid" :class="collapsed ? 'fa-chevron-up' : 'fa-chevron-down'"></i>
          <span class="u-narrow-hide">{{ collapsed ? 'EXPAND' : 'COLLAPSE' }}</span>
        </button>
      </div>
    </div>
    <slot v-if="collapsed" name="peek"></slot>
    <!-- bodyPersist: the body stays mounted and painted, so no xterm is disposed. `overflow: hidden` clips it as the box eases shut, where `v-show` would pop it away in one frame. -->
    <div v-if="bodyPersist" class="dock-stack-body"><slot></slot></div>
    <slot v-else-if="!collapsed"></slot>
  </div>
</template>

<script setup>
import { ref } from 'vue';
import {
  dockDragging,
  dockStackFlex,
  resetStackHeight,
  setStackHeightFromPointer,
} from '../composables/useDockLayout';

const props = defineProps({
  collapsed: { type: Boolean, default: false },
  bodyPersist: { type: Boolean, default: false },
  stackKey: { type: String, required: true },
});
defineEmits(['update:collapsed']);

const stackEl = ref(null);

// setPointerCapture keeps the drag alive once the pointer outruns this 3px element; dockDragging drops the transitions for its duration (`.is-dragging`).
function onSplitterDown(e) {
  e.preventDefault();
  dockDragging.value = true;
  e.currentTarget.setPointerCapture(e.pointerId);
}

// Re-read per frame, not captured on pointerdown: a drag that starts while maximised un-maximises on its first move, and only a live read sees where this stack's floor landed afterwards.
function onSplitterMove(e) {
  if (!dockDragging.value) return;
  setStackHeightFromPointer(props.stackKey, e.clientY, stackEl.value.getBoundingClientRect().bottom);
}

function onSplitterUp(e) {
  if (!dockDragging.value) return;
  dockDragging.value = false;
  try {
    e.currentTarget.releasePointerCapture(e.pointerId);
  } catch {
    // Already gone (window blur mid-drag); dockDragging above is what actually ends the drag.
  }
}
</script>
