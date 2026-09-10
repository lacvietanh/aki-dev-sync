import { ref, computed } from 'vue'

export const sshHosts = ref([])
// Per-user override of the default host, seeded from localStorage at boot. Read-only: nothing writes it; a slot with no explicit host resolves through here, else the first host in ~/.ssh/config.
export const _storedHost = ref(localStorage.getItem('aki-selected-ssh-host') || '')
export const selectedSshHost = computed(() => _storedHost.value || sshHosts.value[0] || '')
export const showSshModal = ref(false)
export const sshConfigText = ref('')
export const hasSshUndo = ref(false)
export const hasSshRedo = ref(false)
