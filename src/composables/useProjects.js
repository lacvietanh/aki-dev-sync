import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useLogs } from "./useLogs";
import Swal from "sweetalert2";

const Toast = Swal.mixin({
  toast: true,
  position: 'bottom-end',
  showConfirmButton: false,
  timer: 3000,
  timerProgressBar: true,
  background: '#131317',
  color: '#e2e8f0',
  didOpen: (toast) => {
    toast.addEventListener('mouseenter', Swal.stopTimer)
    toast.addEventListener('mouseleave', Swal.resumeTimer)
  }
});

const projects = ref([]);
const syncingProjectId = ref(null);
const syncStatus = ref("idle");
const isReloading = ref(false);

const showConfigModal = ref(false);
const editingProject = ref(null);

const showSpecialModal = ref(false);
const specialProject = ref(null);
const specialFiles = ref([]);
const specialSelected = ref([]);
const specialLoading = ref(false);

const showGitModal = ref(false);
const gitProject = ref(null);

// Export for other composables if needed
export {
  projects, syncingProjectId, syncStatus, isReloading,
  showConfigModal, editingProject,
  showSpecialModal, specialProject, specialFiles, specialSelected, specialLoading,
  showGitModal, gitProject,
  Toast
};

export function useProjects() {
  const { appendGlobalLog, appendLog, projectLogs, activeLogProjectId, isLogExpanded, setupGlobalListener } = useLogs();

  async function fetchGitStatus(project) {
    try {
      appendGlobalLog("GIT", `Checking status for "${project.name}"...`);
      const status = await invoke("get_git_status", { localPath: project.local_path });
      project.git_status = status;
      appendGlobalLog("GIT", `Status for "${project.name}": ${status}`);
      
      try {
        const remoteUrl = await invoke("get_git_remote_url", { localPath: project.local_path });
        if (remoteUrl) {
          project.remote_url = remoteUrl;
        }
      } catch (e) {
        // silently ignore remote url errors
      }
    } catch (err) {
      project.git_status = "Git Error";
      appendGlobalLog("ERROR", `Failed git status for "${project.name}": ${err}`);
    }
  }

  async function loadData(sshHosts, showToast = false) {
    if (isReloading.value) return;
    isReloading.value = true;
    try {
      appendGlobalLog("LOAD", "Initializing workspace and scanning SSH hosts...");
      sshHosts.value = await invoke("get_ssh_hosts");
      appendGlobalLog("LOAD", `Found ${sshHosts.value.length} SSH hosts.`);
      const loaded = await invoke("load_projects");

      for (const p of loaded) {
        if (p.dry_run === undefined) p.dry_run = true;
        if (p.sync_git === undefined) p.sync_git = true;
        p.git_status = "...";
        if (!projectLogs.value[p.id]) projectLogs.value[p.id] = [];
      }
      projects.value = loaded;
      setupGlobalListener();

      appendGlobalLog("LOAD", `Loaded ${loaded.length} projects successfully.`);

      const fetchPromises = projects.value.map(p => fetchGitStatus(p));
      await Promise.all(fetchPromises);

      if (showToast) Toast.fire({ icon: 'success', title: 'Data Reloaded!' });
    } catch (err) {
      appendGlobalLog("ERROR", `Failed to load data: ${err}`);
      if (showToast) Toast.fire({ icon: 'error', title: 'Lỗi tải dữ liệu' });
    } finally {
      isReloading.value = false;
    }
  }

  async function saveProjectsList() {
    try {
      await invoke("save_projects", { projects: projects.value });
    } catch (err) {
      appendGlobalLog("ERROR", `Failed to save projects: ${err}`);
    }
  }

  async function startSync(project, direction, specificPaths = []) {
    if (syncingProjectId.value !== null) return;

    const isDryRun = project.dry_run;
    syncingProjectId.value = project.id;
    activeLogProjectId.value = project.id;
    isLogExpanded.value = true;
    syncStatus.value = "syncing";

    if (!projectLogs.value[project.id]) projectLogs.value[project.id] = [];
    projectLogs.value[project.id] = [];

    let actionName = direction.toUpperCase();
    if (specificPaths.length === 1 && specificPaths[0] === ".git/") actionName = "SYNC GIT";
    else if (specificPaths.length > 0) actionName = "PUSH SPECIAL";

    appendLog(project.id, `>>> START SYNC [${actionName}] - ${project.name}`);
    if (specificPaths.length > 0) {
      appendLog(project.id, `>>> TARGET: Partial Sync on ${specificPaths.length} specific item(s)`);
    }

    const dryText = isDryRun ? " (Dry Run)" : "";
    appendGlobalLog("SYNC", `Started ${actionName} for "${project.name}"${dryText}`);

    try {
      await invoke("run_sync", {
        project: project,
        direction: direction,
        dryRun: isDryRun,
        specificPaths: specificPaths,
        syncGit: project.sync_git
      });
      project.last_sync_action = actionName + (isDryRun ? " (Dry)" : "");
      project.last_sync_time = Math.floor(Date.now() / 1000);
      await saveProjectsList();
      fetchGitStatus(project);

      if (activeLogProjectId.value === project.id) {
        setTimeout(() => {
          isLogExpanded.value = false;
          activeLogProjectId.value = null;
        }, 1500);
      }

      Toast.fire({ icon: 'success', title: 'Đồng bộ hoàn tất!' });
    } catch (err) {
      appendLog(project.id, `\n[ERROR] Sync failed: ${err}`);
      appendGlobalLog("ERROR", `Sync failed for "${project.name}": ${err}`);
      Toast.fire({ icon: 'error', title: 'Đồng bộ thất bại' });
    } finally {
      syncingProjectId.value = null;
      syncStatus.value = "idle";
    }
  }

  function openConfig(project) {
    const p = JSON.parse(JSON.stringify(project));
    if (!p.hooks) p.hooks = { pre_pull_cmd: null, post_pull_cmd: null, pre_push_cmd: null, post_push_cmd: null, run_hooks_on_remote: true };
    if (!p.pull_excludes) p.pull_excludes = [];
    if (!p.push_excludes) p.push_excludes = [];
    if (p.production_url === undefined) p.production_url = "";

    editingProject.value = p;
    showConfigModal.value = true;
  }

  function closeConfig() {
    showConfigModal.value = false;
    editingProject.value = null;
  }

  async function saveConfig() {
    if (!editingProject.value) return;

    if (editingProject.value.production_url) {
      const pUrl = editingProject.value.production_url.trim();
      if (!pUrl.startsWith('http://') && !pUrl.startsWith('https://') && pUrl !== "") {
        editingProject.value.production_url = 'https://' + pUrl;
      } else {
        editingProject.value.production_url = pUrl;
      }
    }

    const index = projects.value.findIndex(p => p.id === editingProject.value.id);

    if (index !== -1) {
      editingProject.value.dry_run = projects.value[index].dry_run;
      editingProject.value.sync_git = projects.value[index].sync_git;
      editingProject.value.git_status = projects.value[index].git_status;
      projects.value[index] = JSON.parse(JSON.stringify(editingProject.value));
    } else {
      editingProject.value.dry_run = true;
      editingProject.value.sync_git = true;
      projects.value.push(JSON.parse(JSON.stringify(editingProject.value)));
    }

    await saveProjectsList();
    const savedProject = projects.value.find(p => p.id === editingProject.value.id);
    if (savedProject) fetchGitStatus(savedProject);
    closeConfig();
  }

  async function createNewProject(sshHosts) {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const selectedPath = await open({
      directory: true,
      multiple: false,
      title: "Select Local Project Folder"
    });

    if (selectedPath) {
      const folderName = selectedPath.split('/').pop() || "New Project";
      const newId = "project-" + Date.now();
      
      let productionUrl = "";
      if (folderName.includes(".")) {
          productionUrl = "https://" + folderName;
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
        sync_git: true,
        dry_run: true,
        git_status: "..."
      };
      openConfig(p);
    }
  }

  function confirmRemove() {
    if (!editingProject.value) return;

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
        const id = editingProject.value.id;
        const projectName = editingProject.value.name;
        projects.value = projects.value.filter(p => p.id !== id);
        if (activeLogProjectId.value === id) activeLogProjectId.value = null;
        saveProjectsList();
        closeConfig();
        appendGlobalLog("REMOVE", `Project "${projectName}" was removed from the local list.`);
      }
    });
  }

  async function openSpecialModal(project) {
    specialProject.value = project;
    showSpecialModal.value = true;
    specialFiles.value = [];
    specialSelected.value = [];
    specialLoading.value = true;

    try {
      specialFiles.value = await invoke("get_project_files", { localPath: project.local_path });
    } catch (err) {
      appendGlobalLog("ERROR", `Failed to load files: ${err}`);
    }

    specialLoading.value = false;
  }

  function toggleSpecialSelection(file) {
    const idx = specialSelected.value.indexOf(file);
    if (idx === -1) specialSelected.value.push(file);
    else specialSelected.value.splice(idx, 1);
  }

  function selectAllSpecial(selected) {
    if (selected) specialSelected.value = [...specialFiles.value];
    else specialSelected.value = [];
  }

  function confirmPushSpecial() {
    if (specialSelected.value.length === 0) return;
    const p = specialProject.value;
    const selected = [...specialSelected.value];
    closeSpecialModal();
    startSync(p, "push", selected);
  }

  function closeSpecialModal() {
    showSpecialModal.value = false;
    specialProject.value = null;
    specialFiles.value = [];
    specialSelected.value = [];
  }

  function openGitModal(project) {
    gitProject.value = project;
    showGitModal.value = true;
  }

  function closeGitModal() {
    showGitModal.value = false;
    gitProject.value = null;
  }

  return {
    projects,
    syncingProjectId,
    syncStatus,
    isReloading,
    showConfigModal,
    editingProject,
    loadData,
    saveProjectsList,
    startSync,
    fetchGitStatus,
    openConfig,
    closeConfig,
    saveConfig,
    createNewProject,
    confirmRemove,
    showSpecialModal,
    specialProject,
    specialFiles,
    specialSelected,
    specialLoading,
    openSpecialModal,
    toggleSpecialSelection,
    selectAllSpecial,
    confirmPushSpecial,
    closeSpecialModal,
    showGitModal,
    gitProject,
    openGitModal,
    closeGitModal,
    Toast
  };
}
