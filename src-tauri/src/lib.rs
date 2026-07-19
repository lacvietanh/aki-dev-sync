mod agent_usage;
mod claude_profile;
mod git;
mod global_note;
mod logger;
mod projects;
mod ssh;
mod statusline;
mod sync;
mod system;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            logger::init(app.handle());
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .register_uri_scheme_protocol("aki-devsync-icon", |_ctx, request| {
            let uri_str = request.uri().to_string();
            let host = if let Some(stripped) = uri_str.strip_prefix("aki-devsync-icon://") {
                let end = stripped.find(|c| c == '/' || c == '?').unwrap_or(stripped.len());
                &stripped[..end]
            } else {
                ""
            };

            let icons = system::get_project_icons().lock().unwrap();
            if let Some(icon) = icons.get(host) {
                tauri::http::Response::builder()
                    .status(200)
                    .header("Content-Type", &icon.mime_type)
                    .body(icon.bytes.clone())
                    .unwrap()
            } else {
                tauri::http::Response::builder()
                    .status(404)
                    .header("Content-Type", "text/plain")
                    .body(Vec::new())
                    .unwrap()
            }
        })
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
            git::run_git_command,
            git::get_file_conflict_info,
            // sync
            sync::run_sync,
            sync::check_sync_status,
            sync::get_sync_delete_preview,
            sync::cleanup_legacy_baselines,
            // agent usage
            agent_usage::provision_agent_usage,
            agent_usage::force_sync_agent_usage,
            agent_usage::get_agent_usage,
            agent_usage::logout_antigravity,
            system::macos_open,
            system::install_ssh_terminal_color,
            system::install_akiclaudedoc,
            system::open_local_terminal,
            system::open_remote_subprocess,
            system::check_ide_availability,
            system::resolve_remote_path,
            system::resolve_report_html,
            system::check_for_updates,
            system::find_in_downloads,
            system::check_project_stack,
            system::run_project_command,
            system::read_project_changelog,
            // global note
            global_note::read_global_note,
            global_note::write_global_note,
            // claude profile switcher
            claude_profile::get_claude_mode,
            claude_profile::set_claude_profile,
            // statusline customizer
            statusline::get_default_statusline_config,
            statusline::apply_statusline_config,
            statusline::check_statusline_status,
            // logger / debug
            logger::is_debug_mode,
            logger::get_log_path,
            logger::log_frontend,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
