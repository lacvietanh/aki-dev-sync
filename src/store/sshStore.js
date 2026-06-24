import { ref, computed } from 'vue'

export const sshHosts = ref([])
export const _storedHost = ref(localStorage.getItem('aki-selected-ssh-host') || '')
export const selectedSshHost = computed({
  get: () => _storedHost.value || sshHosts.value[0] || '',
  set: v => { _storedHost.value = v; localStorage.setItem('aki-selected-ssh-host', v); }
})
export const showSshModal = ref(false)
export const sshConfigText = ref('')
export const hasSshUndo = ref(false)
export const hasSshRedo = ref(false)
