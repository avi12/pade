mod agents;
mod config;
mod contextmenu;
#[cfg(windows)]
mod copilot;
mod design;
mod ide;
mod naming;
mod os;
mod pty;
mod refs;
mod runner;
mod tasks;
mod usage;
mod util;
mod vcs;
mod watcher;
mod window;
mod workspace;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            workspace::migrate_from_ade(); // one-time ade → pade data migration
            let handle = app.handle();
            pty::init(handle);
            runner::init(handle);
            watcher::init(handle);
            app.manage(window::WindowProjects::default());
            // Paint the main window in-theme before showing it, so a dark desktop
            // doesn't flash a white webview before the UI first renders. The window
            // is created hidden (see tauri.conf.json) and shown here once themed.
            if let Some(main) = app.get_webview_window("main") {
                window::paint_surface(&main);
                let _ = main.show();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            agents::agents_detect,
            contextmenu::context_menu_register,
            contextmenu::context_menu_unregister,
            contextmenu::context_menu_status,
            design::design_tools,
            design::design_open,
            ide::ide_detect,
            ide::ide_add_editor,
            ide::ide_suggest,
            ide::ide_open,
            ide::ide_open_file,
            ide::ide_project_kind,
            os::open_in_explorer,
            os::open_in_terminal,
            os::open_url,
            window::window_create,
            window::window_register_project,
            window::window_focus_project,
            pty::pty_spawn,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_kill,
            pty::pty_history,
            pty::session_transcript,
            runner::runner_start,
            runner::runner_stop,
            runner::runner_list,
            watcher::watch_start,
            vcs::status::vcs_status,
            vcs::log::vcs_log,
            vcs::status::vcs_diff,
            vcs::branches::vcs_branches,
            vcs::inspect::vcs_commit,
            vcs::inspect::vcs_commit_diff,
            vcs::remote::vcs_remote_url,
            vcs::branches::vcs_current_branch,
            vcs::worktree::vcs_worktree_add,
            vcs::restore::vcs_restore_candidates,
            vcs::restore::vcs_restore_checkout,
            config::config_list,
            config::config_read,
            tasks::tasks_list,
            usage::usage_get,
            usage::usage_session,
            usage::usage_account,
            naming::project_autoname,
            naming::session_generate_name,
            workspace::launch_context,
            workspace::settings_get,
            workspace::workspace_add_root,
            workspace::workspace_remove_root,
            workspace::workspace_scan,
            workspace::workspace_open,
            workspace::workspace_temp,
            workspace::workspace_move,
            workspace::workspace_rename,
            workspace::workspace_set_label,
            workspace::workspace_delete,
            workspace::workspace_create,
            workspace::workspace_clone,
            workspace::workspace_clear_recent,
            workspace::set_default_agent,
            workspace::set_project_agent,
            workspace::set_prefs,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ADE")
        .run(|handle, event| {
            // As the app exits (window closed), terminate every agent PTY so no
            // child lingers and the workspace cwd-lock is released — letting the
            // user reopen and pick up cleanly.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(state) = handle.try_state::<pty::PtyState>() {
                    pty::kill_all(state.inner());
                }
            }
        });
}
