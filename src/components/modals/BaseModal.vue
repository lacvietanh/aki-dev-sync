<template>
  <Teleport to="body">
    <div v-if="show" class="modal-overlay" @click.self="$emit('close')">
      <div class="modal-container" :class="containerClass" :style="containerStyle">
        <div class="modal-header" :class="headerClass">
          <h2><slot name="title" /></h2>
          <button class="btn-close-modal" @click="$emit('close')">
            <i class="fa-solid fa-xmark"></i>
          </button>
        </div>
        <slot />
      </div>
    </div>
  </Teleport>
</template>

<script setup>
import { onMounted, onUnmounted } from 'vue'

const props = defineProps({
  show: Boolean,
  containerClass: String,
  containerStyle: [String, Object],
  headerClass: String,
})
const emit = defineEmits(['close'])

function handleEsc(e) {
  if (e.key === 'Escape' && props.show) emit('close')
}

onMounted(() => window.addEventListener('keydown', handleEsc, true))
onUnmounted(() => window.removeEventListener('keydown', handleEsc, true))
</script>
