<template>
  <!-- Renders nothing itself — Swal.fire is a portal-style overlay driven imperatively below.
       Mounted on BOTH host and companion (docs/plan/1.20.0-terminal-and-remote-sync.md §3): the
       requirement is that the STATE (pendingDialog) is mirrored, not that the widget is
       hand-built, so this keeps the visual diff at zero by reusing Swal under the hood. -->
  <div style="display:none" aria-hidden="true"></div>
</template>

<script setup>
import { watch } from 'vue'
import Swal from 'sweetalert2'
import { pendingDialog, resolveDialog } from '../store/dialogStore'

// The dialog id this screen currently has a Swal open for (or null). Used to (a) avoid
// re-opening the same dialog on a redundant mirror re-render and (b) recognise, once Swal.fire's
// promise settles, whether this screen's answer is still the one that matters (first-answer-wins,
// see showDialog below).
let openId = null

const SWAL_DEFAULTS = {
  background: '#131317',
  color: '#F3F4F6',
  allowOutsideClick: false,
}

watch(
  pendingDialog,
  (d) => {
    if (!d) {
      // Cleared by whichever screen answered first (could be this one, via showDialog below, or
      // the other one via a mirrored delta). If we still have a Swal open for a dialog that is
      // now gone, close it so this screen does not keep showing a decision nobody is waiting on.
      if (openId && Swal.isVisible()) Swal.close()
      openId = null
      return
    }
    if (d.id === openId) return // already showing this exact dialog
    openId = d.id
    showDialog(d)
  },
  { immediate: true }
)

async function showDialog(d) {
  const opts = {
    ...SWAL_DEFAULTS,
    title: d.title,
    text: d.text,
    html: d.html,
    icon: d.icon,
    width: d.width,
    showCancelButton: true,
    confirmButtonColor: d.confirmButtonColor,
    cancelButtonColor: d.cancelButtonColor || '#374151',
    confirmButtonText: d.confirmButtonText || 'OK',
    cancelButtonText: d.cancelButtonText || 'Cancel',
  }
  if (d.background) opts.background = d.background
  if (d.color) opts.color = d.color

  if (d.kind === 'typed') {
    opts.input = 'text'
    opts.inputPlaceholder = d.inputPlaceholder || ''
    // Local, UX-only gate (mirrors the old preConfirm) so the Enter/Confirm button gives
    // immediate feedback on a mismatch. NOT the authoritative check — the code awaiting
    // askConfirm() on the host re-validates `answer.typed` itself (remote-control.md §3.4:
    // "the typed confirmation travels with the answer and is validated on the Mac").
    if (d.requireText) {
      opts.preConfirm = (val) => {
        if (val !== d.requireText) {
          Swal.showValidationMessage(d.mismatchText || `Type "${d.requireText}" to confirm`)
          return false
        }
        return val
      }
    }
  } else if (d.kind === 'select') {
    opts.input = 'select'
    opts.inputOptions = d.inputOptions || {}
    opts.inputPlaceholder = d.inputPlaceholder || ''
  }

  const result = await Swal.fire(opts)

  // While this screen's Swal was open, the OTHER screen may have already answered (first-answer-
  // wins) — the watcher above then nulled pendingDialog and force-closed this Swal, which is what
  // just made Swal.fire's promise settle. Only send an answer if this dialog is still the one
  // pending; otherwise this screen's click is the losing one and becomes a silent no-op.
  if (!pendingDialog.value || pendingDialog.value.id !== d.id) return
  openId = null

  if (d.kind === 'typed') {
    resolveDialog(d.id, { confirmed: result.isConfirmed, typed: result.value })
  } else if (d.kind === 'select') {
    resolveDialog(d.id, { confirmed: result.isConfirmed, value: result.value })
  } else {
    resolveDialog(d.id, { confirmed: result.isConfirmed })
  }
}
</script>
