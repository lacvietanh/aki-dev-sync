import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import Swal from 'sweetalert2'
import { projects, projectRuntime, isReloading, Toast } from '../store/projectStore'
import { useLogs } from './useLogs'
import { fetchGitStatus } from './useGit'

export const showConfigModal = ref(false)
export const editingProject = ref(null)

const { appendGlobalLog, projectLogs, activeLogProjectId, setupGlobalListener } = useLogs()

export async function loadData(sshHosts, showToast = false) {
  if (isReloading.value) return
  isReloading.value = true
  try {
    if (showToast) appendGlobalLog("SYSTEM", "User triggered manual reload.")
    appendGlobalLog("LOAD", "Initializing workspace and scanning SSH hosts...")
    sshHosts.value = await invoke("get_ssh_hosts")
    appendGlobalLog("LOAD", `Found ${sshHosts.value.length} SSH hosts.`)
    const loaded = await invoke("load_projects")

    for (const p of loaded) {
      // Preserve syncing flag if a sync is in progress during reload
      projectRuntime.value[p.id] = {
        git_status: "...",
        git_log: "",
        remote_url: "",
        syncing: projectRuntime.value[p.id]?.syncing ?? false,
      }
      if (!projectLogs.value[p.id]) projectLogs.value[p.id] = []
    }
    projects.value = loaded
    setupGlobalListener()

    appendGlobalLog("LOAD", `Loaded ${loaded.length} projects successfully.`)

    await Promise.all(projects.value.map(p => fetchGitStatus(p.id)))

    if (showToast) Toast.fire({ icon: 'success', title: 'Data Reloaded!' })
  } catch (err) {
    appendGlobalLog("ERROR", `Failed to load data: ${err}`)
    if (showToast) Toast.fire({ icon: 'error', title: 'Reload failed' })
  } finally {
    isReloading.value = false
  }
}

export async function saveProjectsList() {
  try {
    await invoke("save_projects", { projects: projects.value })
  } catch (err) {
    appendGlobalLog("ERROR", `Failed to save projects: ${err}`)
  }
}

export function openConfig(project) {
  const p = {
    ...project,
    hooks: project.hooks
      ? { ...project.hooks }
      : { pre_pull_cmd: null, post_pull_cmd: null, pre_push_cmd: null, post_push_cmd: null, run_hooks_on_remote: true },
    pull_excludes: [...(project.pull_excludes || [])],
    push_excludes: [...(project.push_excludes || [])],
    production_url: project.production_url ?? "",
  }
  editingProject.value = p
  showConfigModal.value = true
}

export function closeConfig() {
  showConfigModal.value = false
  editingProject.value = null
}

export async function saveConfig() {
  if (!editingProject.value) return

  if (editingProject.value.production_url) {
    const pUrl = editingProject.value.production_url.trim()
    if (!pUrl.startsWith('http://') && !pUrl.startsWith('https://') && pUrl !== "") {
      editingProject.value.production_url = 'https://' + pUrl
    } else {
      editingProject.value.production_url = pUrl
    }
  }

  const index = projects.value.findIndex(p => p.id === editingProject.value.id)

  if (index !== -1) {
    projects.value[index] = { ...editingProject.value }
    appendGlobalLog("CONFIG", `User updated config for project "${editingProject.value.name}".`)
  } else {
    projectRuntime.value[editingProject.value.id] = {
      git_status: "...",
      git_log: "",
      remote_url: "",
      syncing: false,
    }
    projects.value.push({ ...editingProject.value })
    appendGlobalLog("CONFIG", `User created new project "${editingProject.value.name}".`)
  }

  await saveProjectsList()
  const savedProject = projects.value.find(p => p.id === editingProject.value.id)
  if (savedProject) fetchGitStatus(savedProject.id)
  closeConfig()
}

export async function createNewProject(sshHosts) {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const selectedPath = await open({
    directory: true,
    multiple: false,
    title: "Select Local Project Folder"
  })

  if (selectedPath) {
    const folderName = selectedPath.split('/').pop() || "New Project"
    const newId = "project-" + Date.now()

    let productionUrl = ""
    if (folderName.includes(".")) {
      productionUrl = "https://" + folderName
    }

    const p = {
      id: newId,
      name: folderName,
      local_path: selectedPath.endsWith('/') ? selectedPath : selectedPath + "/",
      remote_host: sshHosts.value[0] || "localhost",
      remote_path: "~/",
      production_url: productionUrl,
      pull_excludes: [".DS_Store", "*.log", ".git/", "node_modules/", ".nuxt/", ".output/", ".wrangler/", "dist/", ".claude/"],
      push_excludes: [".DS_Store", "*.log", "node_modules/", ".nuxt/", ".output/", ".wrangler/", "dist/", ".claude/"],
      hooks: { pre_pull_cmd: null, post_pull_cmd: null, pre_push_cmd: null, post_push_cmd: null, run_hooks_on_remote: true },
      last_sync_action: null,
      last_sync_time: null,
      last_sync_status: null,
      sync_git: true,
      dry_run: true,
      delete_on_pull: true,
    }
    openConfig(p)
  }
}

export function confirmRemove() {
  if (!editingProject.value) return

  Swal.fire({
    title: 'Remove Project?',
    text: `Remove "${editingProject.value.name}" from the app list? Your actual code files will NOT be touched.`,
    icon: 'warning',
    showCancelButton: true,
    confirmButtonColor: '#ef4444',
    cancelButtonColor: '#374151',
    confirmButtonText: 'Yes, remove it',
    background: '#131317',
    color: '#F3F4F6'
  }).then((result) => {
    if (result.isConfirmed) {
      const id = editingProject.value.id
      const projectName = editingProject.value.name
      projects.value = projects.value.filter(p => p.id !== id)
      delete projectRuntime.value[id]
      if (activeLogProjectId.value === id) activeLogProjectId.value = null
      saveProjectsList()
      closeConfig()
      appendGlobalLog("REMOVE", `Project "${projectName}" was removed from the local list.`)
    }
  })
}
