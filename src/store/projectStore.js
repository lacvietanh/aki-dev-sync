import { ref, computed } from 'vue'
import Swal from 'sweetalert2'

export const Toast = Swal.mixin({
  toast: true,
  position: 'bottom',
  showConfirmButton: false,
  timer: 3000,
  timerProgressBar: true,
  background: '#131317',
  color: '#e2e8f0',
  didOpen: (toast) => {
    toast.addEventListener('mouseenter', Swal.stopTimer)
    toast.addEventListener('mouseleave', Swal.resumeTimer)
  }
})

// Persisted config — synced with projects.json via save_projects
export const projects = ref([])

// Ephemeral runtime — never serialized, lost on restart (intentional: all derived)
// Shape: { [id]: { git_status, git_log, remote_url, syncing } }
export const projectRuntime = ref({})

export const isReloading = ref(false)

// Preloaded IDE availability and cache-busting timestamp for icons
export const ideAvailability = ref(null)
export const iconTimestamp = ref(Date.now())

// True when any project is currently syncing — used by header/console
export const anySyncing = computed(() =>
  Object.values(projectRuntime.value).some(r => r.syncing)
)
