<!--
  Reusable base for one dock panel ("stack"). Purely presentational — collapse STATE is owned by
  each stack's own caller (logStore.isLogExpanded for LogStack.vue,
  useTerminalPanel.js's terminalStackCollapsed for TerminalStack.vue); this component only renders
  the header/body/peek chrome around it and emits update:collapsed when its chevron is clicked.

  Reuses the EXISTING .terminal-header / .terminal-title / .terminal-actions classes from main.css
  (Extreme Narrow, CLAUDE.md: no new visual language for a layout change).
-->
<template>
  <div class="dock-stack" :class="{ 'is-collapsed': collapsed }">
    <div class="terminal-header">
      <div class="terminal-title" :class="titleClass">
        <slot name="title">
          <i v-if="icon" class="fa-solid mr-1" :class="icon"></i>
          {{ title }}
        </slot>
      </div>
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
    <!-- bodyPersist (opt-in, terminal stack only): keep the body MOUNTED (and, deliberately, always
         PAINTED — no `v-show`) while collapsed, so collapsing does not dispose every xterm. Toggling
         `display:none` the instant `collapsed` flips would pop the content away in one frame while
         `.dock-stack`'s flex-basis/flex-grow are still easing over their own 0.25s — the box shrinks
         smoothly around content that already vanished, which reads as "the transition doesn't
         actually happen" (or, on expand, as content snapping in a frame before the box has finished
         growing to fit it). `.dock-stack { overflow: hidden }` (main.css) does the hiding instead: as
         flex-basis eases down to the collapsed header height, the body is clipped out of view in the
         same motion as the box, so the content's disappearance and the box's shrink are the SAME
         animation rather than two unsynchronized ones. The wrapper div is unavoidable — a <slot>
         renders a fragment and needs an element to hang the class on. -->
    <div v-if="bodyPersist" class="dock-stack-body"><slot></slot></div>
    <slot v-else-if="!collapsed"></slot>
  </div>
</template>

<script setup>
defineProps({
  title: { type: String, default: '' },
  collapsed: { type: Boolean, default: false },
  icon: { type: String, default: '' },
  titleClass: { type: [String, Object, Array], default: '' },
  // false (default) — collapsing destroys the body, the cheap behaviour a log stack wants.
  // true — collapsing only hides it (see the template comment); the specialization opts in.
  bodyPersist: { type: Boolean, default: false },
})
defineEmits(['update:collapsed'])
</script>
