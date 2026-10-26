//! Launching a session: resolving the program a spawn asks for, the arguments
//! and environment that agent needs (theme, model report, conversation id,
//! terminal capabilities), and opening the PTY it runs in.

use std::io::{Read, Write};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};

use crate::agents::running_model::ModelChoice;
use crate::agents::DEFAULT_SHELL;
use crate::terminal::theming::Scheme;

use super::registry::SessionError;

/// Overrides the program a session runs when the spawn names none.
const AGENT_COMMAND_OVERRIDE_VARIABLE: &str = "ADE_AGENT_CMD";
/// The login shell outside Windows, when the spawn names no command.
const UNIX_SHELL_VARIABLE: &str = "SHELL";

/// Everything a session is started from - what the app's `pty_spawn` asks for,
/// carried whole to the PTY host. The owner and id travel beside it.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionLaunch {
    /// The program to run (the agent's canonical name, e.g. `claude`); `None`
    /// falls back to `ADE_AGENT_CMD`, then the platform shell.
    pub command: Option<String>,
    /// The directory to run in (a project, a per-branch worktree).
    pub cwd: Option<String>,
    pub columns: u16,
    pub rows: u16,
    /// Extra arguments for `command` (e.g. the project path for a terminal editor).
    pub arguments: Vec<String>,
    /// ADE's resolved appearance at spawn time, for themed CLIs (theming.rs).
    pub scheme: Option<Scheme>,
    /// A stable conversation id to pin/resume this session to (`claude --session-id`).
    pub conversation_id: Option<String>,
    /// A model and effort picked for this launch alone (`running_model.rs`).
    pub model_choice: Option<ModelChoice>,
    /// The environment the session starts from: the app's own, so an agent sees
    /// what it saw when the app spawned it directly - whichever way the PTY host
    /// itself was started, and however long ago. Empty keeps the host's.
    pub environment: Vec<(String, String)>,
}

/// A spawned session's process and PTY halves, not yet published anywhere.
pub(super) struct LaunchedProcess {
    pub(super) master: Box<dyn MasterPty + Send>,
    pub(super) writer: Box<dyn Write + Send>,
    pub(super) reader: Box<dyn Read + Send>,
    pub(super) child: Box<dyn Child + Send + Sync>,
    /// The directory the process actually started in.
    pub(super) cwd: Option<String>,
}

/// Open a PTY and spawn the session's program in it. Blocking: opening a PTY,
/// writing theme files and starting an agent all wait on the operating system.
pub(super) fn launch_process(launch: &SessionLaunch) -> Result<LaunchedProcess, SessionError> {
    let pair = native_pty_system()
        .openpty(pty_size(launch.rows, launch.columns))
        .map_err(SessionError::failed)?;

    // An explicit cwd (e.g. a per-branch worktree) overrides the process dir.
    let directory = launch
        .cwd
        .as_ref()
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok());
    let spawn_cwd = directory
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned());

    let mut process = build_command(launch);
    process.args(&launch.arguments);
    if let Some(directory) = directory {
        process.cwd(directory);
    }

    let child = pair
        .slave
        .spawn_command(process)
        .map_err(SessionError::failed)?;
    drop(pair.slave);

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(SessionError::failed)?;
    let writer = pair.master.take_writer().map_err(SessionError::failed)?;
    Ok(LaunchedProcess {
        master: pair.master,
        writer,
        reader,
        child,
        cwd: spawn_cwd,
    })
}

/// A character-cell size; the pixel size is unused by every backend ADE runs.
pub(super) fn pty_size(rows: u16, columns: u16) -> PtySize {
    PtySize {
        rows,
        cols: columns,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Resolve the program to run: the explicit command, else `ADE_AGENT_CMD`, else
/// the platform shell (so a session is always launchable), and apply whatever
/// environment that agent needs (the registry knows; this module doesn't).
///
/// The command is exec'd by absolute path when we can find one. Left as a bare
/// name it would be resolved by the PTY against *our* inherited PATH - the same
/// stale copy that hides a freshly-installed agent from detection - so a CLI ADE
/// had just listed as available could fail to start. Unresolvable commands (a task
/// runner, a shell) pass through untouched, for the PTY to resolve as before.
fn build_command(launch: &SessionLaunch) -> CommandBuilder {
    let program = launch
        .command
        .clone()
        .or_else(|| std::env::var(AGENT_COMMAND_OVERRIDE_VARIABLE).ok())
        .unwrap_or_else(default_shell);
    let executable = crate::agents::program(&program).map_or_else(
        || program.clone(),
        |path| path.to_string_lossy().into_owned(),
    );
    let scheme = launch.scheme;
    let conversation_id = launch.conversation_id.as_deref();

    // The agent's leading arguments, in the order they must appear; caller-supplied
    // args (e.g. a terminal editor's project path) are appended by
    // `launch_process` after this. Assembled up front so the same list feeds
    // either a direct spawn or the Windows light-console wrapper.
    let mut leading_arguments = agent_launch_arguments(&program, scheme);
    // The agent reports its model and effort out of band (hooks, a plugin) -
    // see running_model.rs. Its arguments join the list (merging into a
    // `--settings` layer the theme arguments may already carry); its environment
    // is applied with the rest.
    let model_report_environment = crate::agents::running_model::attach(
        &program,
        conversation_id,
        launch.model_choice.as_ref(),
        &mut leading_arguments,
    );
    // Bind the session to ADE's stable conversation id (`--session-id <uuid>`), so a
    // later spawn with the same id resumes THIS conversation rather than the most
    // recent - how ADE restarts one specific session (e.g. after `.mcp.json`
    // changed). Only for an agent whose CLI supports it; a shell/editor ignores it.
    if let Some(arguments) = conversation_id
        .and_then(|conversation_id| crate::agents::session_id_arguments(&program, conversation_id))
    {
        leading_arguments.extend(arguments);
    }

    let mut process = launcher(&program, &executable, scheme, &leading_arguments);
    apply_base_environment(&mut process, &launch.environment);
    apply_agent_environment(&mut process, &program, scheme, model_report_environment);
    apply_terminal_capabilities(&mut process);
    process
}

/// Start the process from `environment` - the app's own - instead of the
/// environment the PTY host happened to be started with. Applied before the
/// agent's own variables, which override it.
fn apply_base_environment(process: &mut CommandBuilder, environment: &[(String, String)]) {
    if environment.is_empty() {
        return;
    }
    process.env_clear();
    for (key, value) in environment {
        process.env(key, value);
    }
}

/// The platform shell a session runs when nothing names a program: the login
/// `$SHELL` outside Windows, else the registry's default shell.
fn default_shell() -> String {
    if cfg!(windows) {
        return DEFAULT_SHELL.into();
    }
    std::env::var(UNIX_SHELL_VARIABLE).unwrap_or_else(|_| DEFAULT_SHELL.into())
}

/// The arguments every interactive launch of `program` leads with: its
/// skip-every-permission ("yolo") mode, so ADE drives it autonomously with no
/// per-tool/edit approval stops, then the per-scheme theme arguments of an
/// arg-themed CLI (codex's global `-c tui.theme=…`) so its TUI starts on ADE's
/// current appearance (see theming.rs). Interactive spawn path only; the oneshot
/// `codex exec` renders no TUI. Empty for a shell or an unknown command.
fn agent_launch_arguments(program: &str, scheme: Option<Scheme>) -> Vec<String> {
    let theme_arguments = scheme.map_or(&[][..], |scheme| {
        crate::terminal::theming::spawn_arguments(program, scheme)
    });
    crate::agents::session_arguments(program)
        .iter()
        .chain(theme_arguments)
        .map(ToString::to_string)
        .collect()
}

/// The process to spawn for `executable` + `arguments`.
///
/// Windows + light scheme only: some agents (Codex) paint their input composer
/// box from the *detected* terminal background, which under `ConPTY` is the
/// pseudoconsole's hard-coded BLACK buffer no env/arg/config/OSC channel can
/// change (see agents.rs). The one lever is a child-side console write, so ADE
/// prefixes `cmd /c color F0 & …` to flip the buffer light before the agent
/// probes it. Applied at spawn: a later scheme flip won't recolor an
/// already-running box (acceptable - every spawn-time theme signal works this
/// way). Every other case (dark scheme, non-Windows, agents that don't need it)
/// spawns the agent unchanged.
fn launcher(
    program: &str,
    executable: &str,
    scheme: Option<Scheme>,
    arguments: &[String],
) -> CommandBuilder {
    let is_light_console_forced = cfg!(target_os = "windows")
        && scheme == Some(Scheme::Light)
        && crate::agents::needs_light_console_fix(program);
    if is_light_console_forced {
        return light_console_command(executable, arguments);
    }
    let mut process = CommandBuilder::new(executable);
    process.args(arguments);
    process
}

/// The environment `program` needs, applied to whichever process ADE spawns; a
/// `cmd.exe` wrapper passes it through to the agent it launches. Keyed by the
/// command ADE knows (`codex`), not the file it resolved to
/// (`…\codex-x86_64-pc-windows-msvc.exe`).
fn apply_agent_environment(
    process: &mut CommandBuilder,
    program: &str,
    scheme: Option<Scheme>,
    model_report_environment: Vec<(String, String)>,
) {
    for (key, value) in crate::agents::spawn_environment(program) {
        process.env(key, value);
    }
    for (key, value) in model_report_environment {
        process.env(key, value);
    }
    let Some(scheme) = scheme else {
        return;
    };
    // Per-scheme theme environment for env-themed CLIs (aider, cursor-agent), so the
    // agent starts matching ADE's current appearance - see theming.rs.
    for (key, value) in crate::terminal::theming::spawn_environment(program, scheme) {
        process.env(key, value);
    }
    // …and the tui-config file pair for file-themed CLIs (opencode):
    // theming.rs materializes the config on disk and returns the env pair
    // pointing at it. Filesystem I/O happens here, before any session lock.
    for (key, value) in crate::terminal::theming::spawn_tui_config_environment(program, scheme) {
        process.env(key, value);
    }
    // …and the live theme definition the launch args are about to select
    // (Claude), written before the process starts so its first frame already
    // paints in ADE's scheme. A flip later rewrites this same file, which is
    // what re-themes the session without restarting it.
    if let Err(error) = crate::terminal::theming::publish_live_themes(scheme) {
        eprintln!("theming: could not publish the live theme ({error}); {program} starts on its own default");
    }
}

/// ADE's terminal renders the full 16-colour ANSI palette (see terminal-theme.ts),
/// but a CLI can't tell from inside a bare `ConPTY`: Ink/chalk/supports-color probe
/// for a colour-capable terminal (TERM, COLORTERM, an isatty check) and, finding a
/// Windows pseudoconsole that advertises none of them, fall back to MONOCHROME
/// output - Claude Code's banner and every agent's colour go grey. These two are
/// the escape hatch supports-color honours: `FORCE_COLOR` pins the level
/// regardless of the isatty result, `COLORTERM` names truecolor so 24-bit SGR is
/// emitted.
const COLOR_ENVIRONMENT: [(&str, &str); 2] = [("FORCE_COLOR", "3"), ("COLORTERM", "truecolor")];
/// ADE's terminal renders OSC 8 hyperlinks (xterm + linkHandler), but a CLI can't
/// tell from inside a bare `ConPTY`: Ink/terminal-link probe the environment
/// (`TERM_PROGRAM`, VTE version) and silently fall back to plain text when
/// nothing matches. This is the escape hatch those probes honor.
const HYPERLINK_ENVIRONMENT: (&str, &str) = ("FORCE_HYPERLINK", "1");
/// Claude Code stamps every subprocess it spawns with this marker, and a bare
/// `ConPTY` forwards it verbatim - so when ADE is itself launched from inside a
/// Claude Code session, the agent ADE spawns inherits the marker, reads itself as
/// a nested child, and disables transcript saving (no --resume, no history). An
/// agent ADE launches is always its own top-level session, so ADE clears it.
const CHILD_SESSION_MARKER_VARIABLE: &str = "CLAUDE_CODE_CHILD_SESSION";

/// Tell the CLI what ADE's terminal can render, which it cannot probe through a
/// bare `ConPTY`, and clear the nested-session marker it must not inherit.
fn apply_terminal_capabilities(process: &mut CommandBuilder) {
    for (key, value) in COLOR_ENVIRONMENT {
        process.env(key, value);
    }
    process.env_remove(CHILD_SESSION_MARKER_VARIABLE);
    let (key, value) = HYPERLINK_ENVIRONMENT;
    process.env(key, value);
}

/// Wrap `executable` + `arguments` as `cmd /c color F0 & <executable> <arguments…>` so the `ConPTY` console
/// buffer reads as a light background *before* the launched agent probes it - ADE's
/// Windows fix for Codex's terminal-background-derived composer box (see
/// `build_command` and `agents::needs_light_console_fix`). `color F0` sets the
/// buffer to black-on-white, which `GetConsoleScreenBufferInfoEx` (the probe's
/// Windows fallback) then reports as light.
///
/// Each token is a *separate* argv entry on purpose: portable-pty quotes each with
/// standard Windows (`CommandLineToArgvW`) rules, which for tokens without embedded
/// quotes is exactly what `cmd.exe` parses - a spaced exe path becomes one `"…"`
/// token, and the bare `&` stays cmd's command separator. Building one pre-quoted
/// shell string instead would be double-escaped by portable-pty and mis-parsed by
/// cmd.
fn light_console_command(executable: &str, arguments: &[String]) -> CommandBuilder {
    let mut command = CommandBuilder::new(CONSOLE_HOST);
    command.args(LIGHT_CONSOLE_PREFIX);
    command.arg(executable);
    command.args(arguments);
    command
}

const CONSOLE_HOST: &str = "cmd.exe";
/// `cmd /c color F0 &` - black-on-white, then whatever follows the separator.
const LIGHT_CONSOLE_PREFIX: [&str; 4] = ["/c", "color", "F0", "&"];

#[cfg(test)]
mod tests {
    use super::*;

    /// The Windows light-console wrapper must lead with `cmd /c color F0 &`, keep
    /// the agent's absolute path as ONE argv token even with spaces, and preserve
    /// every following arg verbatim - this is the quoting-risk surface the fix
    /// rides on.
    #[test]
    fn light_console_command_round_trips_the_path_and_arguments() {
        let executable = r"C:\Program Files\codex dir\codex.exe";
        let arguments = vec![
            "--dangerously-bypass-approvals-and-sandbox".to_string(),
            "-c".to_string(),
            "tui.theme=catppuccin-latte".to_string(),
        ];
        let built = light_console_command(executable, &arguments);
        let assembled: Vec<String> = built
            .get_argv()
            .iter()
            .map(|token| token.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            assembled,
            vec![
                "cmd.exe".to_string(),
                "/c".to_string(),
                "color".to_string(),
                "F0".to_string(),
                "&".to_string(),
                executable.to_string(),
                "--dangerously-bypass-approvals-and-sandbox".to_string(),
                "-c".to_string(),
                "tui.theme=catppuccin-latte".to_string(),
            ]
        );
        // The spaced path is a single token (portable-pty quotes it at spawn), not
        // split - the whole point of separate argv entries.
        assert!(assembled.contains(&executable.to_string()));
    }

    /// Only Codex opts into the console workaround; agents themed by file/env and
    /// unknown commands do not.
    #[test]
    fn only_codex_needs_the_light_console_fix() {
        assert!(crate::agents::needs_light_console_fix("codex"));
        assert!(!crate::agents::needs_light_console_fix("claude"));
        assert!(!crate::agents::needs_light_console_fix("aider"));
        assert!(!crate::agents::needs_light_console_fix(DEFAULT_SHELL));
    }

    #[test]
    fn the_app_environment_replaces_the_hosts() {
        let mut process = CommandBuilder::new("program");
        process.env("HOST_ONLY", "1");
        apply_base_environment(&mut process, &[("APP_ONLY".to_string(), "yes".to_string())]);
        assert_eq!(process.get_env("HOST_ONLY"), None);
        assert_eq!(
            process.get_env("APP_ONLY").and_then(|value| value.to_str()),
            Some("yes")
        );
    }

    #[test]
    fn an_empty_environment_keeps_the_hosts() {
        let mut process = CommandBuilder::new("program");
        process.env("HOST_ONLY", "1");
        apply_base_environment(&mut process, &[]);
        assert!(process.get_env("HOST_ONLY").is_some());
    }

    #[test]
    fn a_launch_round_trips_through_its_wire_shape() {
        let launch = SessionLaunch {
            command: Some("claude".into()),
            cwd: Some(r"C:\work".into()),
            columns: 120,
            rows: 40,
            arguments: vec!["--flag".into()],
            scheme: Some(Scheme::Dark),
            conversation_id: Some("abc".into()),
            model_choice: None,
            environment: vec![("PATH".into(), r"C:\bin".into())],
        };
        let text = serde_json::to_string(&launch).expect("serializable");
        assert!(text.contains("\"conversationId\":\"abc\""));
        let parsed: SessionLaunch = serde_json::from_str(&text).expect("parsable");
        assert_eq!(parsed, launch);
    }
}
