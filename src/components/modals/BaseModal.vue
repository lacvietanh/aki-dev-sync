<template>
  <Teleport to="body">
    <div v-if="show" class="modal-overlay" :style="{ zIndex: zIndex }" @click.self="handleOverlayClick">
      <div class="modal-container" :class="containerClass" :style="containerStyle">
        <div class="modal-header" :class="headerClass">
          <h2><slot name="title" /></h2>
          <!-- Icon-only, so it needs a name of its own. Every modal in the app closes through
               this one button, so labelling it here labels all of them. -->
          <button class="btn-close-modal" title="Close" aria-label="Close" @click="$emit('close')">
            <i class="fa-solid fa-xmark"></i>
          </button>
        </div>
        <slot />
      </div>
    </div>
  </Teleport>
</template>

<script setup>
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'

// --- Global modal stack (module-level singleton) ---
const _stack = ref([])
let _idCounter = 0

function _push(id) {
  if (!_stack.value.includes(id)) _stack.value.push(id)
}
function _pop(id) {
  const i = _stack.value.indexOf(id)
  if (i !== -1) _stack.value.splice(i, 1)
}
function _isTop(id) {
  return _stack.value[_stack.value.length - 1] === id
}
// ---

const props = defineProps({
  show: Boolean,
  containerClass: String,
  containerStyle: [String, Object],
  headerClass: String,
})
const emit = defineEmits(['close'])

const modalId = `modal-${++_idCounter}`

watch(() => props.show, (val) => {
  if (val) _push(modalId)
  else _pop(modalId)
}, { immediate: true })

onUnmounted(() => _pop(modalId))

const zIndex = computed(() => {
  const i = _stack.value.indexOf(modalId)
  return i === -1 ? 1000 : 1000 + i * 10
})

function handleEsc(e) {
  if (e.key === 'Escape' && props.show && _isTop(modalId)) emit('close')
}

function handleOverlayClick() {
  if (_isTop(modalId)) emit('close')
}

onMounted(() => window.addEventListener('keydown', handleEsc, true))
onUnmounted(() => window.removeEventListener('keydown', handleEsc, true))
</script>
