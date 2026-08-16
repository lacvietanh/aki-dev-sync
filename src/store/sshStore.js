import { ref, computed } from 'vue'
import { action } from '../services/action'

export const sshHosts = ref([])
export const _storedHost = ref(localStorage.getItem('aki-selected-ssh-host') || '')
export const selectedSshHost = computed({
  get: () => _storedHost.value || sshHosts.value[0] || '',
  set: v => { _storedHost.value = v; localStorage.setItem('aki-selected-ssh-host', v); }
})

// C→H action: retargets selected host on host (docs/feat/remote-control.md) and mirrors back to companions.
export const setSelectedSshHost = action('sshStore.setSelectedSshHost', (host) => {
  selectedSshHost.value = host
})
export const showSshModal = ref(false)
export const sshConfigText = ref('')
export const hasSshUndo = ref(false)
export const hasSshRedo = ref(false)
