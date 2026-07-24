import { ref, computed } from 'vue'
import { action } from '../services/action'

export const sshHosts = ref([])
export const _storedHost = ref(localStorage.getItem('aki-selected-ssh-host') || '')
export const selectedSshHost = computed({
  get: () => _storedHost.value || sshHosts.value[0] || '',
  set: v => { _storedHost.value = v; localStorage.setItem('aki-selected-ssh-host', v); }
})

// Which remote host the usage/sync UI targets. `_storedHost`/`sshHosts` already mirror
// host→companion; this action closes C→H so the REMOTE-tab host picker on a phone retargets the
// Mac (which mirrors back), instead of a v-model that only writes the phone's copy. See the
// control matrix ("Remote host select") in docs/feat/remote-control.md.
export const setSelectedSshHost = action('sshStore.setSelectedSshHost', (host) => {
  selectedSshHost.value = host
})
export const showSshModal = ref(false)
export const sshConfigText = ref('')
export const hasSshUndo = ref(false)
export const hasSshRedo = ref(false)
