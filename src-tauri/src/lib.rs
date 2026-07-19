mod agents;
mod config;
mod contextmenu;
#[cfg(windows)]
mod copilot;
mod design;
mod discord;
mod ide;
mod members;
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

// Mostly the command registry (`generate_handler!`) + builder wiring — one long
// but flat list, not branching logic, so the line count is expected to grow with
// each new IPC command.
#[allow(clippy::too_many_lines)]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Dev-only: expose WebView2's DevTools protocol (CDP) on a fixed port so tools
    // — chrome-devtools-mcp, a raw CDP client — can drive the live app for UI checks.
    // Set before any window (and thus any WebView2 environment) is created, and
    // compiled out of release builds. `--remote-allow-origins=*` lets modern Chromium
    // accept the WebSocket CDP connection; the port also opens the webview to any local
    // process, which is why this stays gated behind debug_assertions.
    #[cfg(all(debug_assertions, windows))]
    std::env::set_var(
        "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
        "--remote-debugging-port=9222 --remote-allow-origins=*",
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            workspace::migrate_from_ade(); // one-time ade → pade data migration
            let handle = app.handle();
            pty::init(handle);
            runner::init(handle);
            watcher::init(handle);
            app.manage(window::WindowProjects::default());
            app.manage(discord::DiscordState::default());
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
            discord::discord_set_activity,
            discord::discord_clear_activity,
            ide::ide_detect,
            ide::ide_add_editor,
            ide::ide_remove_editor,
            ide::ide_suggest,
            ide::ide_choose_editor,
            ide::ide_kind_options,
            ide::ide_kinds,
            ide::ide_project_kinds,
            ide::ide_open,
            ide::ide_open_file,
            members::members_list,
            os::open_in_explorer,
            os::open_in_terminal,
            os::open_url,
            window::window_create,
            window::window_register_project,
            window::window_focus_project,
            window::window_focus_relative,
            window::window_list,
            window::window_focus_label,
            pty::pty_spawn,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_kill,
            pty::pty_history,
            pty::pty_list,
            runner::runner_start,
            runner::runner_stop,
            runner::runner_list,
            watcher::watch_start,
            watcher::watch_dirs,
            watcher::feed_diff,
            vcs::status::vcs_status,
            vcs::log::vcs_log,
            vcs::status::vcs_diff,
            vcs::branches::vcs_branches,
            vcs::branches::vcs_branch_of,
            vcs::pull::vcs_pull,
            vcs::inspect::vcs_commit,
            vcs::inspect::vcs_commit_diff,
            vcs::remote::vcs_remote_url,
            vcs::clone::vcs_git_installed,
            vcs::clone::vcs_has_ssh_key,
            vcs::clone::vcs_probe_remote,
            vcs::clone::vcs_clone,
            vcs::worktree::vcs_worktree_add,
            vcs::restore::vcs_restore_candidates,
            vcs::restore::vcs_restore_checkout,
            config::config_list,
            config::config_read,
            tasks::tasks_list,
            usage::usage_get,
            usage::usage_account,
            naming::project_autoname,
            naming::session_generate_name,
            workspace::launch_context,
            workspace::settings_get,
            workspace::workspace_add_root,
            workspace::workspace_remove_root,
            workspace::workspace_probe_path,
            workspace::workspace_scan,
            workspace::workspace_open,
            workspace::handoff_doc_delete,
            workspace::workspace_temp,
            workspace::workspace_move,
            workspace::workspace_rename,
            workspace::workspace_set_label,
            workspace::workspace_delete,
            workspace::workspace_prune,
            workspace::workspace_create,
            workspace::workspace_clear_recent,
            workspace::workspace_set_pinned,
            workspace::workspace_remove_recent,
            workspace::workspace_set_pinned_order,
            workspace::workspace_delete_directory,
            workspace::set_default_agent,
            workspace::set_project_agent,
            workspace::set_prefs,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ADE")
        .run(|handle, event| {
            // As the app exits, terminate every agent PTY so no child lingers
            // and the workspace cwd-lock is released — letting the user reopen
            // and pick up cleanly. The frontend has already closed its sessions
            // gracefully by now (App intercepts the window close and waits for
            // each agent's idle prompt before killing); this is the backstop
            // for whatever a force-close or a crashed WebView leaves behind.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(state) = handle.try_state::<pty::PtyState>() {
                    pty::kill_all(state.inner());
                }
            }
        });
}
