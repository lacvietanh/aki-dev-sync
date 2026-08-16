<template>
  <!-- Imperative Swal.fire overlay driven by pendingDialog state mirroring. -->
  <div style="display:none" aria-hidden="true"></div>
</template>

<script setup>
import { watch } from 'vue'
import Swal from 'sweetalert2'
import { pendingDialog, resolveDialog } from '../store/dialogStore'

// Active dialog ID currently open on this screen (first-answer-wins dedup).
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
      // Close stale modal if cleared by another screen answering first.
      if (openId && Swal.isVisible()) Swal.close()
      openId = null
      return
    }
    // Show new or promoted dialog when ID differs from current open modal.
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
    // Client UX validation before submitting typed confirmation.
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

  // Ignore resolution if another screen already answered this dialog.
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
