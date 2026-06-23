mod agent_usage;
mod git;
mod projects;
mod ssh;
mod sync;
mod system;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            // projects
            projects::load_projects,
            projects::save_projects,
            // ssh
            ssh::get_ssh_hosts,
            ssh::read_ssh_config,
            ssh::save_ssh_config,
            ssh::undo_ssh_config,
            ssh::redo_ssh_config,
            ssh::get_ssh_history_status,
            // git
            git::get_git_info,
            git::get_project_files,
            // sync
            sync::run_sync,
            sync::check_sync_status,
            // agent usage
            agent_usage::provision_agent_usage,
            agent_usage::force_sync_agent_usage,
            agent_usage::get_agent_usage,
            // system / OS integration
            system::macos_open,
            system::open_remote_subprocess,
            system::get_project_icon_base64,
            system::check_ide_availability,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
