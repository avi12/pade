mod agents;
mod platform;
mod terminal;
#[cfg(test)]
mod test_support;
mod update;
mod usage;
mod util;
mod vcs;
mod workspace;

use tauri::Manager;

/// The value an opt-in environment switch is set to when it is on.
#[cfg(windows)]
const SWITCH_ON: &str = "1";

/// Whether the opt-in environment switch `variable` is on.
#[cfg(windows)]
fn is_switch_on(variable: &str) -> bool {
    std::env::var(variable).is_ok_and(|value| value == SWITCH_ON)
}

/// The `WebView2` browser switches and user-data folder, set as environment
/// before any window (and thus any `WebView2` environment) is created.
#[cfg(windows)]
fn configure_webview2_environment() {
    let mut browser_arguments = Vec::new();
    // Dev-only and explicit opt-in: CDP lets any local process control the
    // privileged webview, so a normal debug build must not expose it.
    #[cfg(debug_assertions)]
    if is_switch_on("PADE_ENABLE_CDP") {
        browser_arguments.push("--remote-debugging-port=9222 --remote-allow-origins=*");
    }
    // Opt-in software rendering: when another process saturates the GPU (a
    // game), the webview's composited frames queue behind it and every UI
    // interaction stalls on the commits. `--disable-gpu` renders on the CPU
    // instead, decoupling ADE from the GPU queue entirely - slower frames in
    // absolute terms, but immune to GPU contention.
    let is_software_render_preferred = workspace::load().prefs.passthrough.get("softwareRender")
        == Some(&serde_json::Value::Bool(true));
    if is_software_render_preferred || is_switch_on("PADE_SOFTWARE_RENDER") {
        browser_arguments.push("--disable-gpu");
    }
    if !browser_arguments.is_empty() {
        std::env::set_var(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            browser_arguments.join(" "),
        );
    }
    // Give this instance its own WebView2 user-data folder, keyed by the launch
    // project, so two projects open in parallel run in separate browser + GPU
    // process trees rather than sharing one (the shared default lets one
    // instance's GPU load compound into the other and trip a DWM reset). Unlike
    // the additional-arguments var above (which appends), WEBVIEW2_USER_DATA_FOLDER
    // *replaces* the folder WebView2 would otherwise derive - the supported way
    // to override it, since `--user-data-dir` as a browser arg is ignored.
    if let Some(folder) = workspace::webview_data_directory() {
        std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", folder);
    }
}

// Mostly the command registry (`generate_handler!`) + builder wiring - one long
// but flat list, not branching logic, so the line count is expected to grow with
// each new IPC command.
#[allow(clippy::too_many_lines)]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Launched by an agent's hook to record a session's model and effort: do that
    // and exit before anything of the app (a window, a lowered priority) starts.
    if agents::running_model::run_hook_sink_if_requested() {
        return;
    }

    #[cfg(windows)]
    {
        // Agents can launch CPU-heavy compilers and dev servers. Lower this
        // process before creating WebView2 or PTY children so the whole tree
        // yields scheduler time to games and other foreground applications.
        if let Err(error) = process_priority::lower_current_process() {
            eprintln!("could not lower PADE process priority: {error}");
        }
        configure_webview2_environment();
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|window, event| {
            // WebView2 doesn't reliably fire the frontend's prefers-color-scheme
            // change for a window that stays focused; the native ThemeChanged does.
            match event {
                tauri::WindowEvent::ThemeChanged(theme) => {
                    platform::window::on_theme_changed(window, *theme);
                }
                tauri::WindowEvent::Destroyed => platform::window::on_destroyed(window),
                _ => {}
            }
        })
        .setup(|app| {
            workspace::migrate_from_ade(); // one-time ade → pade data migration
            let handle = app.handle();
            workspace::init(handle);
            terminal::pty::init(handle);
            platform::window_handoff::init(handle);
            terminal::runner::init(handle);
            workspace::watcher::init(handle);
            agents::running_model::init(handle);
            agents::conversation_watch::init(handle);
            update::init(handle);
            app.manage(platform::window::WindowProjects::default());
            app.manage(platform::discord::DiscordState::default());
            // Paint the main window in-theme before showing it, so a dark desktop
            // doesn't flash a white webview before the UI first renders. The window
            // is created hidden (see tauri.conf.json) and shown here once themed.
            if let Some(main) = app.get_webview_window(platform::window::MAIN_WINDOW_LABEL) {
                platform::window::paint_surface(&main);
                let _ = main.show();
                // Crash recovery arms from the FRONTEND (`recovery_arm` with
                // its live location.href) - a backend capture here raced
                // navigation and rebuilt a crashed window onto about:blank.
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            agents::agents_detect,
            platform::clipboard::clipboard_image_save,
            platform::contextmenu::context_menu_register,
            platform::contextmenu::context_menu_unregister,
            platform::contextmenu::context_menu_status,
            platform::design::design_tools,
            platform::design::design_open,
            platform::discord::discord_set_activity,
            platform::discord::discord_clear_activity,
            workspace::ide::ide_detect,
            workspace::ide::ide_add_editor,
            workspace::ide::ide_remove_editor,
            workspace::ide::ide_suggest,
            workspace::ide::ide_choose_editor,
            workspace::ide::ide_kind_options,
            workspace::ide::ide_kinds,
            workspace::ide::ide_project_kinds,
            workspace::ide::ide_project_languages,
            workspace::ide::ide_open,
            workspace::ide::ide_open_file,
            workspace::members::members_list,
            platform::os::open_in_explorer,
            platform::os::reveal_in_explorer,
            platform::os::open_in_terminal,
            platform::os::open_url,
            platform::os::restart,
            platform::window::window_create,
            platform::window::window_register_project,
            platform::window::window_focus_project,
            platform::window::window_focus_relative,
            platform::window::window_list,
            platform::window::window_reorder,
            platform::window::window_focus_label,
            platform::window::window_theme,
            platform::window_handoff::window_hand_off,
            platform::window_handoff::window_take_handoffs,
            terminal::pty::pty_spawn,
            terminal::pty::pty_write,
            terminal::pty::pty_resize,
            terminal::pty::pty_kill,
            terminal::pty::session_task_stop,
            terminal::pty::session_task_running,
            terminal::pty::pty_history,
            terminal::pty::pty_list,
            platform::recovery::recovery_arm,
            terminal::theming::agent_theme_publish,
            agents::resume::agent_resume_args,
            agents::conversations::agent_conversations,
            agents::session_context::agent_session_context,
            agents::running_model::agent_running_model,
            agents::running_model::agent_model_options,
            agents::running_model::agent_apply_model,
            terminal::runner::runner_start,
            terminal::runner::runner_stop,
            terminal::runner::runner_list,
            workspace::watcher::watch_projects,
            workspace::watcher::watch_status,
            workspace::watcher::watch_dirs,
            workspace::watcher::watch_languages,
            workspace::watcher::feed_diff,
            workspace::watcher::feed_image,
            workspace::watcher::feed_text,
            workspace::watcher::feed_video,
            workspace::watcher::feed_ignored,
            vcs::status::vcs_status,
            vcs::log::vcs_log,
            vcs::status::vcs_diff,
            vcs::branches::vcs_branches,
            vcs::branches::vcs_branch_of,
            vcs::branches::vcs_remote_branches,
            vcs::branches::vcs_switch_branch,
            vcs::pull::vcs_pull,
            vcs::inspect::vcs_commit,
            vcs::inspect::vcs_commit_diff,
            vcs::remote::vcs_default_branch,
            vcs::remote::vcs_remote_url,
            vcs::clone::vcs_git_installed,
            vcs::clone::vcs_has_ssh_key,
            vcs::clone::vcs_probe_remote,
            vcs::clone::vcs_clone,
            vcs::worktree::vcs_worktree_add,
            vcs::restore::vcs_restore_candidates,
            vcs::restore::vcs_restore_checkout,
            workspace::config::config_list,
            workspace::config::config_read,
            terminal::tasks::tasks_descriptors,
            terminal::tasks::tasks_list,
            terminal::schemes::terminal_schemes,
            usage::usage_get,
            usage::usage_account_agent,
            usage::limit_reset::limit_reset_offer,
            usage::limit_reset::limit_reset_claim,
            agents::naming::project_autoname,
            agents::naming::session_generate_name,
            agents::summary::session_summarize,
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
            workspace::workspace_rename_folder,
            workspace::workspace_save_migrate,
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
            update::update_status,
            update::update_check,
            update::update_restart,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ADE")
        .run(|handle, event| {
            // As the app exits, terminate every agent PTY so no child lingers
            // and the workspace cwd-lock is released - letting the user reopen
            // and pick up cleanly. The frontend has already closed its sessions
            // gracefully by now (App intercepts the window close and waits for
            // each agent's idle prompt before killing); this is the backstop
            // for whatever a force-close or a crashed WebView leaves behind.
            //
            // On the way out, a downloaded release is swapped in, so the next
            // start is the new version.
            if matches!(event, tauri::RunEvent::Exit) {
                if let Err(error) = update::apply_pending(handle) {
                    eprintln!("could not install the update: {error}");
                }
                return;
            }
            let tauri::RunEvent::ExitRequested { api, code, .. } = &event else {
                return;
            };
            // The last window closing mid crash-recovery is not an exit:
            // its replacement (same label, same URL) is about to be built,
            // and killing the PTYs now would defeat the recovery. An
            // explicit exit (`code` present) is always honored.
            if code.is_none() && platform::recovery::is_recreate_pending() {
                api.prevent_exit();
                return;
            }
            if let Some(state) = handle.try_state::<terminal::pty::PtyState>() {
                terminal::pty::kill_all(state.inner());
            }
        });
}
