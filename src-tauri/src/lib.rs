mod agent_usage;
mod claude_cleanup;
mod claude_profile;
mod gemini_allowlist;
mod git;
mod global_note;
mod logger;
mod project_notes;
mod projects;
mod pty;
mod remote_shell;
mod ssh;
mod statusline;
mod sync;
mod system;
mod web_server;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            logger::init(app.handle());
            // Remote Control relay (docs/plan/done/remote-control.md §7) — binds the axum server on
            // Tauri's own tokio runtime; never blocks this setup thread (see web_server::init).
            web_server::init(app.handle());
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
                let end = stripped.find(['/', '?']).unwrap_or(stripped.len());
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
            sync::cancel_sync,
            sync::cleanup_legacy_baselines,
            // agent usage
            agent_usage::provision_agent_usage,
            agent_usage::get_agent_usage,
            system::macos_open,
            system::install_ssh_terminal_color,
            system::install_akiclaudedoc,
            system::open_local_terminal,
            system::open_remote_subprocess,
            system::build_remote_ssh_command,
            system::check_ide_availability,
            system::resolve_remote_path,
            system::resolve_report_html,
            system::check_for_updates,
            system::find_in_downloads,
            system::check_project_stack,
            system::read_project_changelog,
            system::count_external_terminals,
            system::count_external_terminals_global,
            system::list_external_terminals,
            // global note
            global_note::read_global_note,
            global_note::write_global_note,
            // per-project tasks & notes, stored in the repo (docs/plan/done/1.22.0-notes-json-ssot.md)
            project_notes::read_project_notes,
            project_notes::read_project_notes_map,
            project_notes::write_project_notes,
            // claude profile switcher
            claude_profile::get_claude_mode,
            claude_profile::set_claude_profile,
            // Claude Code cleanup (docs/plan/done/claudecode-cleanup.md) — host-only by design; both
            // commands are deliberately absent from COMPANION_ALLOWED_COMMANDS.
            claude_cleanup::scan_claude_cleanup,
            claude_cleanup::run_claude_cleanup,
            // statusline customizer
            statusline::apply_statusline_config,
            statusline::check_statusline_status,
            // agy allowlist preload
            gemini_allowlist::apply_gemini_allowlist,
            // logger / debug
            logger::is_debug_mode,
            logger::get_log_path,
            logger::log_frontend,
            // remote control relay (docs/plan/done/remote-control.md §7)
            web_server::start_companion_server,
            web_server::stop_companion_server,
            web_server::get_companion_url,
            web_server::get_companion_status,
            web_server::get_tailscale_https,
            web_server::set_tailscale_https,
            web_server::list_paired_devices,
            web_server::revoke_device,
            web_server::get_project_icons_map,
            web_server::read_text_file,
            // in-app terminal (docs/plan/done/1.20.0-terminal-and-remote-sync.md §4)
            pty::pty_spawn,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_get_scrollback,
            pty::pty_kill,
            pty::pty_restart,
            pty::pty_clear,
            pty::pty_cwd,
            // Multi-tab surface: enumerate the backend's tabs (scrollback replay + host re-adoption
            // after a frontend reload) and close exactly one of them.
            pty::pty_list_tabs,
            pty::pty_close_tab,
        ])
        // `build` + `run(closure)` rather than `run(context)` purely so there is somewhere to hang
        // the exit hook below — the two are otherwise equivalent.
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            // Kill the in-app terminal's whole process tree on quit. Without this, anything the
            // user left running in that shell — most damagingly a live `ssh <host>` — outlives the
            // app as an init-owned orphan and keeps its remote session (and that session's `agy`/
            // `claude`) alive indefinitely. See pty::kill_process_group.
            // Same for a running sync: rsync and the ssh it spawned used to survive the app as
            // init-owned orphans, still writing to a remote nobody is watching any more.
            if let tauri::RunEvent::Exit = event {
                pty::shutdown();
                sync::shutdown();
            }
        });
}
