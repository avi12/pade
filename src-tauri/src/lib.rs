mod agents;
mod config;
mod ide;
mod os;
mod pty;
mod util;
mod vcs;
mod watcher;
mod workspace;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle();
            pty::init(handle);
            watcher::init(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            agents::agents_detect,
            ide::ide_detect,
            ide::ide_suggest,
            ide::ide_open,
            os::open_in_explorer,
            os::open_in_terminal,
            pty::pty_spawn,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_kill,
            watcher::watch_start,
            vcs::vcs_status,
            vcs::vcs_log,
            vcs::vcs_diff,
            vcs::vcs_branches,
            vcs::vcs_worktree_add,
            config::config_list,
            config::config_read,
            workspace::launch_context,
            workspace::settings_get,
            workspace::workspace_add_root,
            workspace::workspace_remove_root,
            workspace::workspace_scan,
            workspace::workspace_open,
            workspace::workspace_temp,
            workspace::workspace_move,
            workspace::workspace_rename,
            workspace::workspace_delete,
            workspace::workspace_create,
            workspace::workspace_clone,
            workspace::workspace_clear_recent,
            workspace::set_default_agent,
            workspace::set_project_agent,
            workspace::set_prefs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ADE");
}
