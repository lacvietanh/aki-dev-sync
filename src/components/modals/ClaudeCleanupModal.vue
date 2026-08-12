<template>
  <BaseModal :show="show" @close="$emit('close')" container-style="width: 420px; max-width: calc(100vw - 32px);">
    <template #title>
      <i class="fa-solid fa-broom"></i> Claude Code Cleanup
      <span class="scope-tag" title="This always acts on ~/.claude on this machine - there is no remote-host target">
        <i class="fa-solid fa-laptop-code"></i> Local
      </span>
    </template>

    <div class="modal-body">
      <div v-if="loading" class="scan-line">
        <i class="fa-solid fa-circle-notch fa-spin"></i> Measuring…
      </div>

      <div v-for="g in groups" :key="g.id" class="group" :class="{ kept: !g.deletable, warn: g.id === 'memory' }">
        <div class="group-head" @click="toggleOpen(g.id)">
          <button
            v-if="g.deletable"
            class="pick"
            :class="groupState(g)"
            :disabled="!pickable(g).length"
            :title="groupState(g) === 'all' ? 'Deselect all in this group' : 'Select all in this group'"
            @click.stop="toggleGroup(g)">
            <i class="fa-solid" :class="groupState(g) === 'some' ? 'fa-minus' : 'fa-check'"></i>
          </button>
          <i v-else class="fa-solid fa-lock lock-icon" title="Never deleted by this app"></i>

          <span class="group-label">{{ g.label }}</span>
          <span class="group-hint">{{ HINTS[g.id] }}</span>
          <span class="group-size">{{ formatBytes(g.bytes) }}</span>
          <i class="fa-solid fa-chevron-down chev" :class="{ open: open.has(g.id) }"></i>
        </div>

        <div v-if="open.has(g.id)" class="entries" :class="{ pickable: g.deletable }">
          <div
            v-for="e in g.entries"
            :key="e.path"
            class="entry"
            :class="{ gone: !e.exists, on: selected.has(e.key) }"
            @click="g.deletable && e.exists && toggleEntry(e.key)">
            <button
              v-if="g.deletable"
              class="pick sm"
              :class="{ all: selected.has(e.key) }"
              :disabled="!e.exists"
              :title="e.path">
              <i class="fa-solid fa-check"></i>
            </button>
            <span class="entry-label" :title="e.path">{{ e.label }}</span>
            <span class="entry-size">{{ e.exists ? formatBytes(e.bytes) : '-' }}</span>
          </div>
        </div>
      </div>

      <div v-if="status.msg" class="status-msg u-select-text" :class="status.err ? 'err' : 'ok'">
        <i class="fa-solid" :class="status.err ? 'fa-triangle-exclamation' : 'fa-check-circle'"></i>
        {{ status.msg }}
      </div>
    </div>

    <div class="modal-footer">
      <button class="btn-modal-action btn-rescan" :disabled="loading || busy" title="Measure again" @click="rescan">
        <i class="fa-solid fa-rotate"></i>
      </button>
      <!-- Two-state button rather than a second dialog: the narrow-UI principle (CLAUDE.md) says a
           state change rides an existing element, and the confirm is more legible on the button that
           carries the action than in a modal stacked over this one. -->
      <button
        class="btn-modal-action btn-cleanup-delete"
        :class="{ arming: arming }"
        :disabled="busy || selectedBytes === 0"
        :title="arming ? 'Click again to delete' : 'Delete the selected groups'"
        @click="onDelete">
        <i class="fa-solid" :class="busy ? 'fa-circle-notch fa-spin' : (arming ? 'fa-triangle-exclamation' : 'fa-trash')"></i>
        {{ buttonLabel }}
      </button>
    </div>
  </BaseModal>
</template>

<script setup>
import { ref, reactive, computed, watch } from 'vue'
import { invoke } from '../../utils/tauri'
import { formatBytes } from '../../utils/bytes'
import { triggerManualRefresh } from '../../store/refreshStore'
import BaseModal from './BaseModal.vue'

const props = defineProps({ show: { type: Boolean, default: false } })
defineEmits(['close'])

// Consequence of deleting each group, in the row itself - the alternative is a tooltip nobody opens
// before clicking a destructive button.
const HINTS = {
  account: 'signs you out',
  data: 'history & transcripts',
  memory: 'written by the agent',
  cache: 'safe, regenerates',
  kept: 'never touched',
}

const groups = ref([])
const loading = ref(false)
const busy = ref(false)
const arming = ref(false)
const selected = reactive(new Set())
const open = reactive(new Set())
const status = reactive({ msg: '', err: false })

// Selection is per entry, not per group: the group checkbox is a shortcut over its own entries and
// never a unit of its own. `agent-memory` is the reason - it sits alone in its own group so that
// "select everything in Data" cannot reach authored content, while still being one click away for
// someone who genuinely wants a clean slate.
const selectedBytes = computed(() =>
  groups.value
    .filter((g) => g.deletable)
    .flatMap((g) => g.entries)
    .filter((e) => selected.has(e.key))
    .reduce((sum, e) => sum + e.bytes, 0)
)

/** Entries in a group that can actually be ticked - a path that does not exist is not a choice. */
function pickable(g) {
  return g.deletable ? g.entries.filter((e) => e.exists) : []
}

function groupState(g) {
  const items = pickable(g)
  if (!items.length) return 'none'
  const on = items.filter((e) => selected.has(e.key)).length
  if (on === 0) return 'none'
  return on === items.length ? 'all' : 'some'
}

const buttonLabel = computed(() => {
  if (busy.value) return 'Deleting…'
  if (selectedBytes.value === 0) return 'Nothing selected'
  if (arming.value) return `Confirm - delete ${formatBytes(selectedBytes.value)}`
  return `Delete ${formatBytes(selectedBytes.value)}`
})

function toggleOpen(id) {
  open.has(id) ? open.delete(id) : open.add(id)
}

function toggleEntry(key) {
  selected.has(key) ? selected.delete(key) : selected.add(key)
  // Changing what would be deleted must cancel a pending confirm, or the second click deletes a
  // different set than the first click armed.
  arming.value = false
}

function toggleGroup(g) {
  const items = pickable(g)
  const turnOff = groupState(g) === 'all'
  items.forEach((e) => (turnOff ? selected.delete(e.key) : selected.add(e.key)))
  arming.value = false
}

async function rescan() {
  loading.value = true
  status.msg = ''
  try {
    groups.value = await invoke('scan_claude_cleanup')
  } catch (e) {
    status.msg = String(e)
    status.err = true
  } finally {
    loading.value = false
  }
}

async function onDelete() {
  if (!arming.value) {
    arming.value = true
    return
  }
  arming.value = false
  busy.value = true
  status.msg = ''
  try {
    // Keys, never paths — the backend resolves each one against its own catalogue and refuses
    // anything else (docs/feat/claudecode-cleanup.md § Safety). Re-derived from the current scan
    // rather than sent straight from `selected`, so a key left over from a previous scan cannot ride
    // along.
    const keys = groups.value
      .filter((g) => g.deletable)
      .flatMap((g) => g.entries.filter((e) => e.exists && selected.has(e.key)).map((e) => e.key))

    const report = await invoke('run_claude_cleanup', { keys })
    status.err = report.errors.length > 0
    status.msg = report.errors.length
      ? `Freed ${formatBytes(report.freedBytes)}. ${report.errors.length} failed: ${report.errors[0]}`
      : `Freed ${formatBytes(report.freedBytes)}.`
    selected.clear()
    await rescan()
    // The Account group holds the very files the usage panel reads (auth-cache.json,
    // rate-limits-cache.json - docs/arch/usage-claudecode.md). Without this the panel keeps showing
    // quota numbers whose source no longer exists.
    triggerManualRefresh()
  } catch (e) {
    status.msg = String(e)
    status.err = true
  } finally {
    busy.value = false
  }
}

watch(
  () => props.show,
  (val) => {
    if (!val) return
    selected.clear()
    open.clear()
    arming.value = false
    status.msg = ''
    rescan()
  }
)
</script>

<style scoped>
.scope-tag {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 9px;
  font-weight: 700;
  color: #94a3b8;
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 4px;
  padding: 2px 6px;
  letter-spacing: 0.3px;
  margin-left: 2px;
}

.scope-tag i {
  color: #94a3b8;
  font-size: 9px;
}

.modal-body {
  padding: 12px 16px 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.scan-line {
  font-size: 11px;
  color: #64748b;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 0 4px;
}

.group {
  border: 1px solid rgba(255, 255, 255, 0.09);
  border-radius: 6px;
  overflow: hidden;
}

.group-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 9px;
  cursor: pointer;
  background: rgba(255, 255, 255, 0.03);
  transition: background 0.15s;
}

.group-head:hover {
  background: rgba(255, 255, 255, 0.06);
}

.pick {
  width: 15px;
  height: 15px;
  flex-shrink: 0;
  border-radius: 4px;
  border: 1px solid rgba(255, 255, 255, 0.2);
  background: transparent;
  color: transparent;
  font-size: 8px;
  line-height: 1;
  cursor: pointer;
  padding: 0;
  transition: all 0.15s;
}

.pick.all,
.pick.some {
  background: #d97757;
  border-color: #d97757;
  color: #1a1a1a;
}

/* Partial selection reads as filled-but-hollow, so "some of this group" is distinguishable from
   "all of it" at a glance rather than only via the icon. */
.pick.some {
  background: rgba(217, 119, 87, 0.35);
  color: #f5c4ae;
}

.pick:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.pick.sm {
  width: 13px;
  height: 13px;
  font-size: 7px;
}

/* The memory group holds content the agent wrote, not state it cached - it is deletable like
   anything else, but it should never be mistaken for the Cache row above it. */
.group.warn {
  border-color: rgba(245, 158, 11, 0.35);
}

.group.warn .group-hint {
  color: #d19a4a;
}

.lock-icon {
  width: 15px;
  flex-shrink: 0;
  font-size: 10px;
  color: #475569;
  text-align: center;
}

.group-label {
  font-size: 11px;
  font-weight: 700;
  color: #e2e8f0;
}

.group-hint {
  flex: 1;
  min-width: 0;
  font-size: 10px;
  color: #64748b;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.group-size {
  font-size: 11px;
  font-weight: 700;
  color: #94a3b8;
  font-family: 'JetBrains Mono', 'Fira Code', ui-monospace, monospace;
}

.chev {
  font-size: 8px;
  color: #475569;
  transition: transform 0.15s;
}

.chev.open {
  transform: rotate(180deg);
}

.group.kept .group-label,
.group.kept .group-size {
  color: #64748b;
}

.entries {
  padding: 4px 9px 7px 17px;
  display: flex;
  flex-direction: column;
  gap: 1px;
  border-top: 1px solid rgba(255, 255, 255, 0.06);
}

.entries:not(.pickable) {
  padding-left: 32px;
}

.entry {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 10px;
  color: #94a3b8;
  padding: 2px 4px;
  border-radius: 4px;
}

.entries.pickable .entry {
  cursor: pointer;
}

.entries.pickable .entry:hover {
  background: rgba(255, 255, 255, 0.04);
}

.entry.on {
  color: #e2e8f0;
}

.entry.gone {
  color: #3f4a5a;
  cursor: default;
}

.entry-label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.entry-size {
  font-family: 'JetBrains Mono', 'Fira Code', ui-monospace, monospace;
}

.status-msg {
  font-size: 11px;
  padding: 7px 10px;
  border-radius: 6px;
  display: flex;
  align-items: flex-start;
  gap: 7px;
  line-height: 1.4;
  margin-top: 2px;
  /* Rust errors arrive verbatim - a long unbroken path must not widen the modal. */
  overflow-wrap: anywhere;
}

.status-msg i {
  margin-top: 1px;
  flex-shrink: 0;
}

.status-msg.ok {
  background: rgba(16, 185, 129, 0.1);
  color: #34d399;
  border: 1px solid rgba(16, 185, 129, 0.2);
}

.status-msg.err {
  background: rgba(239, 68, 68, 0.1);
  color: #f87171;
  border: 1px solid rgba(239, 68, 68, 0.2);
}

.modal-footer {
  display: flex;
  gap: 8px;
  padding: 10px 16px 14px;
  border-top: 1px solid rgba(255, 255, 255, 0.07);
}

.btn-rescan {
  background: rgba(255, 255, 255, 0.04);
  border-color: rgba(255, 255, 255, 0.1);
  color: #64748b;
}

.btn-rescan:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.08);
  color: #94a3b8;
}

.btn-cleanup-delete {
  flex: 1;
  background: rgba(239, 68, 68, 0.12);
  border-color: rgba(239, 68, 68, 0.35);
  color: #f87171;
}

.btn-cleanup-delete:hover:not(:disabled) {
  background: rgba(239, 68, 68, 0.2);
}

.btn-cleanup-delete.arming {
  background: rgba(239, 68, 68, 0.3);
  border-color: #ef4444;
  color: #fecaca;
}

/* Narrow mode (SSoT 700px, main.css) - this file's scoped padding outranks the global narrow
   rule, so the trim has to be repeated here. */
@media (max-width: 700px) {
  .modal-body   { padding: 10px 10px 8px; }
  .modal-footer { padding: 8px 10px 10px; }
  .entries      { padding-left: 11px; }
  .entries:not(.pickable) { padding-left: 24px; }
}
</style>
