import { ref } from 'vue'

export const globalLogs = ref([])
export const projectLogs = ref({})
export const activeLogProjectId = ref(null)
export const isLogExpanded = ref(false)
export const consoleRef = ref(null)
export const copied = ref(false)
export let globalListener = null
export function setGlobalListener(fn) { globalListener = fn }
