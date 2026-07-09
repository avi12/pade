mod agents;
mod config;
mod pty;
mod vcs;
mod watcher;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle();
            pty::init(handle);
            watcher::init(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            agents::agents_detect,
            pty::pty_spawn,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_kill,
            watcher::watch_start,
            vcs::vcs_status,
            vcs::vcs_log,
            vcs::vcs_diff,
            config::config_list,
            config::config_read,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ADE");
}
