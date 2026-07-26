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
          :title="collapsed ? 'Expand panel' : (collapseVariant === 'close' ? 'Close panel (shells keep running)' : 'Collapse panel')"
        >
          <i class="fa-solid" :class="collapsed ? 'fa-chevron-up' : (collapseVariant === 'close' ? 'fa-xmark' : 'fa-chevron-down')"></i>
          <span class="u-narrow-hide">{{ collapsed ? 'EXPAND' : (collapseVariant === 'close' ? 'CLOSE' : 'COLLAPSE') }}</span>
        </button>
      </div>
    </div>
    <slot v-if="collapsed" name="peek"></slot>
    <!-- bodyPersist (opt-in, terminal stack only): keep the body MOUNTED while collapsed and merely
         hide it, so collapsing does not dispose every xterm. The wrapper div is unavoidable —
         v-show sets style.display on one element and a <slot> renders a fragment. Without the prop
         the two lines below reduce to exactly the old `v-if="collapsed" peek / v-else default`
         pair, which is why LogStack.vue's render path is untouched. -->
    <div v-if="bodyPersist" class="dock-stack-body" v-show="!collapsed"><slot></slot></div>
    <slot v-else-if="!collapsed"></slot>
  </div>
</template>

<script setup>
defineProps({
  title: { type: String, default: '' },
  collapsed: { type: Boolean, default: false },
  icon: { type: String, default: '' },
  titleClass: { type: [String, Object, Array], default: '' },
  // 'chevron' (default) | 'close' — CLOSE *is* the chevron, restyled: one affordance, zero new DOM
  // nodes (Extreme Narrow). The collapsed state always keeps fa-chevron-up / EXPAND regardless of
  // this prop, so the panel can be brought back from its own header either way.
  collapseVariant: { type: String, default: 'chevron' },
  // false (default) — collapsing destroys the body, the cheap behaviour a log stack wants.
  // true — collapsing only hides it (see the template comment); the specialization opts in.
  bodyPersist: { type: Boolean, default: false },
})
defineEmits(['update:collapsed'])
</script>
