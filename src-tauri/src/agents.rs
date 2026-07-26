//! Agent registry & detection.
//!
//! ADE is agent-agnostic: it discovers which agent CLIs are installed and lets
//! the user launch, switch, and combine them. Adding a backend is one entry in
//! REGISTRY — nothing else in the app hard-codes "claude".

use std::path::PathBuf;

use serde::Serialize;

use crate::theming::ThemeConfig;
use crate::util::{find_in, search_dirs};

/// Canonical registry ids — the one authoritative spelling of each agent's id
/// (`session.agent.id`). The REGISTRY entries below read from these, as do the
/// other modules that must name an agent (`usage`, `config`), so the vocabulary
/// has a single home and can never drift between them.
pub const ID_CLAUDE: &str = "claude";
pub const ID_CODEX: &str = "codex";
pub const ID_OPENCODE: &str = "opencode";
pub const ID_COPILOT: &str = "copilot";
pub const ID_GROK: &str = "grok";
pub const ID_ANTIGRAVITY: &str = "antigravity";
pub const ID_CURSOR: &str = "cursor";
pub const ID_AIDER: &str = "aider";

struct AgentDefinition {
    id: &'static str,
    label: &'static str,
    /// The executable to look for and run — the name the CLI's own docs use, and
    /// the one ADE stores and shows.
    command: &'static str,
    /// Other executable names the same CLI is installed under. Installers don't
    /// agree: npm, Homebrew and cargo all drop a plain `codex`, but winget's
    /// portable package unpacks the vendor's release binary under its raw
    /// target-triple name and never creates a `codex.exe` at all. Without the
    /// alias, a real install is invisible.
    aliases: &'static [&'static str],
    /// Args that make the CLI answer one prompt non-interactively and exit, with
    /// the prompt appended as the final arg (used for auto-naming). `None` = no
    /// headless mode we can drive.
    oneshot: Option<&'static [&'static str]>,
    /// Args that launch an *interactive* session in the CLI's "skip every
    /// permission prompt" / yolo mode, so ADE drives the agent autonomously —
    /// no per-tool, per-edit approval stops. Empty for a CLI with no such flag.
    /// Distinct from `oneshot` (headless) and from the first-run trust gate,
    /// which these flags do NOT dismiss (ADE accepts that separately — see the
    /// frontend's initial-prompt delivery).
    session_args: &'static [&'static str],
    /// How to force this agent's own UI theme to ADE's scheme via its config
    /// file — the terminal protocol can't carry it through `ConPTY` (see
    /// `theming.rs`). `None` for a CLI with no known theme setting.
    theme_config: Option<ThemeConfig>,
    /// Optional project-local adaptive-theme seed for later launches. Created
    /// only when absent; never merged or overwritten. The registry owns each
    /// agent's native path and syntax.
    project_theme_seed: Option<ProjectThemeSeed>,
    /// The flag that binds a session to a caller-chosen conversation id
    /// (`claude --session-id <uuid>`): it creates a fresh conversation with that
    /// id, and a later spawn with the same id *resumes* that exact one —
    /// non-interactively. That is what lets ADE terminate a specific session and
    /// bring back ITS conversation (e.g. after the project's `.mcp.json` changed),
    /// not merely the most recent. `None` for a CLI with no equivalent.
    session_id_flag: Option<&'static str>,
    /// Environment the CLI needs to render the way ADE embeds it. Empty for most.
    env: &'static [(&'static str, &'static str)],
    /// Whether this CLI paints its own UI (its input composer box) from the
    /// *detected* terminal background and so needs ADE's Windows light-console
    /// workaround. Under `ConPTY` the detected background is the pseudoconsole's
    /// hard-coded BLACK buffer that no env/arg/config/OSC channel can change, so on
    /// a light ADE the box clashes; `pty.rs` prefixes `cmd /c color F0 & …` (a
    /// child-side console write) only for these agents, only on Windows, only when
    /// the scheme is light. `true` for Codex; `false` for CLIs whose box follows a
    /// config/env theme (Claude, aider, cursor) or that inherit the palette.
    needs_light_console_fix: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectThemeSeed {
    pub relative_path: &'static str,
    pub contents: &'static str,
}

/// Known agent backends, in preferred display order. The plain shell is always
/// offered last as a universal fallback.
///
/// **Forcing fullscreen.** ADE's terminal host is tuned for agents that own the
/// terminal's ALTERNATE screen — a full TUI (see the `claude`/`codex` entries and
/// `docs/terminal-rendering.md`). Where a CLI can be pinned to alt-screen *per
/// launch*, ADE does so through whichever field the CLI already exposes — and never
/// a user-global config file, which would leak ADE's choice into the user's other
/// terminals: Claude via `env` (`CLAUDE_CODE_NO_FLICKER=1`); Codex and Copilot via
/// `session_args` (`-c tui.alternate_screen=always` / `--alt-screen on`), each
/// overriding a saved default that could otherwise drop to inline. Grok and
/// Antigravity already render alt-screen by default on a local terminal and expose
/// no safe per-launch override (only a slash command or a user-global settings
/// file), so ADE leaves them as-is; aider is a line-oriented REPL with no fullscreen
/// mode; cursor-agent's rendering is undocumented — none of those three is forced.
/// The TUI configs `OPENCODE_TUI_CONFIG` points opencode at — one per scheme,
/// each selecting the matching PADE-owned plain-string theme (theming.rs
/// materializes both definitions). Plain-string, not adaptive, so the render is
/// truthful regardless of opencode's ConPTY-answered light/dark probe; a flip
/// re-themes by respawn (App's `restartSpawnThemedAgents`).
const OPENCODE_LIGHT_TUI_CONFIG: &str =
    r#"{"$schema":"https://opencode.ai/tui.json","theme":"pade-light"}"#;
const OPENCODE_DARK_TUI_CONFIG: &str =
    r#"{"$schema":"https://opencode.ai/tui.json","theme":"pade-dark"}"#;

const REGISTRY: &[AgentDefinition] = &[
    AgentDefinition {
        id: ID_CLAUDE,
        label: "Claude Code",
        command: "claude",
        aliases: &[],
        oneshot: Some(&["-p"]),
        // Bypass every per-tool/edit approval. The project-theme seed below is
        // created before this process launches, so even a brand-new project has
        // Claude's adaptive mode active on its first frame. The trust gate remains
        // frontend-owned.
        session_args: &["--dangerously-skip-permissions"],
        // Claude's forced `auto` theme reads `$COLORFGBG` before its OSC 11
        // background probe (which ConPTY swallows), so this spawn-time pair is
        // the reliable initial scheme signal. The live ?997 relay handles flips.
        theme_config: Some(ThemeConfig::SpawnEnv {
            light: &[("COLORFGBG", "0;15")],
            dark: &[("COLORFGBG", "15;0")],
        }),
        project_theme_seed: Some(ProjectThemeSeed {
            relative_path: ".claude/settings.local.json",
            contents: "{\n  \"theme\": \"auto\"\n}\n",
        }),
        // `--session-id <uuid>` creates-or-continues the conversation with that
        // id (non-interactive), so ADE can restart a specific session and land
        // back in its own conversation — see `session_id_flag`.
        session_id_flag: Some("--session-id"),
        // Claude Code's fullscreen renderer: it takes over the terminal's ALTERNATE
        // screen and owns every row of it, which is what buys the polished TUI —
        // flicker-free output, mouse support, selection that copies itself. ADE wants
        // that, and `Terminal.svelte` knows how to host it (a resize there waits for
        // the drag to settle and then moves the grid and the agent together; see
        // docs/terminal-rendering.md).
        //
        // Forced per invocation, so the user's own Claude config stays untouched.
        env: &[("CLAUDE_CODE_NO_FLICKER", "1")],
        // Claude's box follows its `theme` settings key, not the terminal probe.
        needs_light_console_fix: false,
    },
    AgentDefinition {
        id: ID_CODEX,
        label: "Codex",
        command: "codex",
        // OpenAI publishes Codex as bare release binaries named by target triple,
        // and winget's package installs one verbatim — so on a winget machine the
        // only `codex` there is answers to `codex-x86_64-pc-windows-msvc`.
        aliases: &[
            "codex-x86_64-pc-windows-msvc",
            "codex-aarch64-pc-windows-msvc",
            "codex-aarch64-apple-darwin",
            "codex-x86_64-apple-darwin",
            "codex-aarch64-unknown-linux-musl",
            "codex-x86_64-unknown-linux-musl",
        ],
        oneshot: Some(&["exec"]),
        // `--yolo` is the alias; the explicit form states what it waives. It also
        // drops the sandbox — the price of a fully autonomous run.
        //
        // `-c tui.alternate_screen=always` forces Codex fullscreen on the terminal's
        // ALTERNATE screen, like Claude — which is what `Terminal.svelte` is tuned to
        // host (its wheel handler forwards a wheel tick as PageUp/PageDown for a
        // fullscreen agent with no xterm scrollback, and Codex scrolls its transcript
        // on PageUp/PageDown). Codex's default `auto` mode drops to inline rendering
        // under some environments (e.g. Zellij) to preserve scrollback, leaving it
        // NOT fullscreen and unscrollable in ADE; `always` overrides that heuristic
        // (see openai/codex `AltScreenMode`). The `-c` override applies to THIS spawn
        // only and never rewrites the user's `~/.codex/config.toml`. This is not
        // scheme-dependent, so it's a constant launch arg here — not in `theme_config`
        // (which is scheme-keyed) and not on the `codex exec` oneshot (which renders
        // no TUI); `session_args` is interactive-session-only, so this is its home.
        session_args: &[
            "--dangerously-bypass-approvals-and-sandbox",
            "-c",
            "tui.alternate_screen=always",
        ],
        // `[tui] theme` selects one of Codex's bundled *syntax-highlight* themes
        // (syntect/two_face) — code spans, diffs, markdown. Codex reads it once at
        // startup and has no live light/dark toggle, so ADE forces it per launch
        // with the global `-c tui.theme=<name>` override: it wins over
        // `~/.codex/config.toml` for THIS spawn only, never rewriting the user's
        // chosen theme on disk. The two names are Codex's own adaptive defaults —
        // what it auto-selects for a light vs dark terminal background (see
        // openai/codex `highlight.rs` `adaptive_default_theme_selection`) — so
        // ADE's scheme wins with Codex's own neutral picks.
        //
        // What `tui.theme` does NOT control: the input composer / user-message box
        // background. Codex fills that from `style::user_message_bg(default_bg())`
        // — a tint blended over the *detected terminal background*, not the syntax
        // theme (openai/codex `style.rs`; verified: a user's own light
        // `inspired-github` global theme still yields a dark composer box on ADE).
        // `default_bg()` comes from a terminal probe (`terminal_probe.rs`): an
        // OSC 10/11 query, then a `GetConsoleScreenBufferInfoEx` fallback. Under
        // Windows ConPTY both resolve to the pseudoconsole's hard-coded BLACK
        // buffer regardless of ADE's scheme — the OSC query is answered by ConPTY
        // itself (never reaching ADE's xterm, matching theming.rs's live finding),
        // and the console buffer's background is Black (empirically confirmed via
        // portable-pty: `GetConsoleScreenBufferInfoEx` reports index-0 Black, and
        // it is unaffected by an OSC 11 SET written to the pty master). So on a
        // light ADE the composer box would stay dark, and no env/args/config/OSC
        // channel can feed Codex a light `default_bg` on Windows — only a
        // child-side console write can. ADE does exactly that via the
        // `needs_light_console_fix` flag below: `pty.rs` prefixes the Windows
        // light-scheme launch with `cmd /c color F0 & …`, which flips the console
        // buffer light *before* Codex probes it (verified end-to-end: the composer
        // fill goes from `48;2;41;41;41` to `48;2;232;232;232`). `tui.theme` is a
        // separate concern and stays forced because it matches syntax highlighting
        // to ADE's scheme.
        // Like SpawnEnv, everything above is read once at startup. A running
        // session keeps its syntax choice until it naturally exits; ADE still
        // re-themes the terminal palette in place so it never destroys a live
        // conversation just to follow an app appearance change.
        theme_config: Some(ThemeConfig::SpawnArgs {
            light: &["-c", "tui.theme=catppuccin-latte"],
            dark: &["-c", "tui.theme=catppuccin-mocha"],
        }),
        project_theme_seed: None,
        session_id_flag: None,
        env: &[],
        // Codex paints its input composer box from the detected terminal
        // background (`default_bg()`), which under Windows ConPTY is the
        // pseudoconsole's hard-coded BLACK buffer — so on a light ADE the box
        // clashes and no clean channel can fix it. `pty.rs` forces the console
        // buffer light before Codex probes it (Windows + light scheme only).
        needs_light_console_fix: true,
    },
    AgentDefinition {
        id: ID_OPENCODE,
        label: "OpenCode",
        command: "opencode",
        aliases: &[],
        // `opencode run <message>` answers one prompt headlessly and exits, with
        // the prompt appended as the final arg — the shape auto-naming drives.
        oneshot: Some(&["run"]),
        // `--auto` auto-approves every permission not explicitly denied —
        // opencode's yolo switch. Its TUI owns the alternate screen by default,
        // so no fullscreen pin is needed.
        session_args: &["--auto"],
        // opencode exposes no per-launch theme flag or env var, and it decides
        // light vs dark ONLY from OSC 10/11 query replies — which under Windows
        // ConPTY are answered by the console layer's hard-coded black buffer, so
        // its mode probe always reads dark and no env var, config key, or later
        // `?997` report flips it (verified: on a light ADE an adaptive theme
        // still rendered its dark variant). The fix sidesteps mode detection:
        // `OPENCODE_TUI_CONFIG` points opencode at a PADE-owned tui config that
        // merges OVER the user's own ~/.config/opencode/tui.json (user file
        // order=1, env file order=2 — keybinds survive, only `theme` is
        // overridden), selecting a plain-string PADE theme for THIS spawn's
        // scheme (`pade-light` or `pade-dark`; theming.rs materializes both).
        // Plain strings render verbatim, so the broken probe can't spoil them. A
        // mid-session flip re-themes by respawn (App's restartSpawnThemedAgents),
        // since opencode has no live theme channel.
        theme_config: Some(ThemeConfig::SpawnTuiConfig {
            variable: "OPENCODE_TUI_CONFIG",
            light: Some(OPENCODE_LIGHT_TUI_CONFIG),
            dark: Some(OPENCODE_DARK_TUI_CONFIG),
        }),
        project_theme_seed: None,
        // `--session <id>` only *continues* an existing session; it cannot create
        // one with a caller-chosen id, so restart-to-resume has no handle here.
        session_id_flag: None,
        env: &[],
        // The `pade-light` theme fixes the colors, but ConPTY resize refills
        // still paint from the console buffer — so a light ADE keeps the
        // console-buffer flip (like Codex) to stop dark flashes on reflow.
        needs_light_console_fix: true,
    },
    AgentDefinition {
        id: ID_COPILOT,
        label: "Copilot CLI",
        // GitHub's standalone Copilot CLI (`npm i -g @github/copilot`) installs a
        // plain `copilot` binary. This is not the older `gh copilot` extension,
        // which is a subcommand of `gh` and has no `copilot` executable of its own.
        command: "copilot",
        aliases: &[],
        // No headless one-shot wired: the CLI's programmatic mode gates on tool
        // approvals, so a naming run could stall. Auto-naming falls back to the
        // label-based heuristic (see naming.rs) until a safe invocation is known.
        oneshot: None,
        // Auto-approve every tool. (`--allow-all` also waives path/URL prompts but
        // has been flaky; tool approval is the friction that matters for a run.)
        //
        // `--alt-screen on` forces Copilot fullscreen on the terminal's ALTERNATE
        // screen, like Claude and Codex — the mode `Terminal.svelte` is tuned to host.
        // Alt-screen is Copilot's default for interactive sessions, but the CLI
        // persists display preferences to the user-level `~/.copilot/settings.json`
        // (a user can turn alt-screen off and it sticks), so `on` pins it for THIS
        // spawn and can't be undone by the saved config — the same reasoning as
        // Codex forcing `tui.alternate_screen=always` over its `auto` default. The
        // flag is documented in Copilot CLI's command reference (default `on`); it
        // rides `session_args` after the tool-approval flag, its authoritative home.
        session_args: &["--allow-all-tools", "--alt-screen", "on"],
        // Copilot CLI auto-follows the system light/dark mode since v1.0.62;
        // forcing it would mean writing the USER-level ~/.copilot/settings.json,
        // which would leak ADE's scheme into every other terminal. Left alone.
        theme_config: None,
        project_theme_seed: None,
        session_id_flag: None,
        env: &[],
        needs_light_console_fix: false,
    },
    AgentDefinition {
        id: ID_GROK,
        label: "Grok CLI",
        command: "grok",
        aliases: &[],
        // xAI's Grok Build answers a single prompt with `-p <PROMPT>`, the same
        // shape as Claude. `--no-auto-update` goes first because a one-shot naming
        // run is exactly the headless, automated case the CLI's own docs say to
        // pass it for — it skips the background update check that would otherwise
        // risk blowing NAME_TIMEOUT before the name comes back.
        oneshot: Some(&["--no-auto-update", "-p"]),
        // xAI Grok Build's "auto-approve all tool executions" (alias `--yolo`).
        session_args: &["--always-approve"],
        // The settings reference lists no theme/color/appearance key at all;
        // its TUI inherits the terminal palette.
        theme_config: None,
        project_theme_seed: None,
        session_id_flag: None,
        env: &[],
        needs_light_console_fix: false,
    },
    AgentDefinition {
        id: ID_ANTIGRAVITY,
        label: "Antigravity CLI",
        command: "antigravity",
        aliases: &[],
        oneshot: None,
        // No verified bypass flag for this CLI yet — left off rather than guess a
        // wrong flag (an unknown flag makes the whole session fail to launch).
        session_args: &[],
        // Its `colorScheme` lives in the USER-level settings file, needs a
        // restart to apply, and only "dark" is documented verbatim — not enough
        // verified surface to force safely.
        theme_config: None,
        project_theme_seed: None,
        session_id_flag: None,
        env: &[],
        needs_light_console_fix: false,
    },
    AgentDefinition {
        id: ID_CURSOR,
        label: "Cursor CLI",
        command: "cursor-agent",
        aliases: &[],
        oneshot: None,
        // Cursor's own permissions docs name `--force` as the run-without-prompts
        // switch; deny rules still take precedence.
        session_args: &["--force"],
        // No documented theme setting; Cursor staff recommend TERM_THEME=light
        // on light-background terminals (fixes an invisible input cursor). The
        // dark side needs no help — its palette assumes a dark terminal.
        theme_config: Some(ThemeConfig::SpawnEnv {
            light: &[("TERM_THEME", "light")],
            dark: &[],
        }),
        project_theme_seed: None,
        session_id_flag: None,
        env: &[],
        needs_light_console_fix: false,
    },
    AgentDefinition {
        id: ID_AIDER,
        label: "aider",
        command: "aider",
        aliases: &[],
        oneshot: None,
        // aider's "always say yes to every confirmation".
        session_args: &["--yes-always"],
        // aider reads its theme once at startup; the documented env pair beats
        // writing a `.aider.conf.yml` into the repo root (which would dirty the
        // user's git status).
        theme_config: Some(ThemeConfig::SpawnEnv {
            light: &[("AIDER_LIGHT_MODE", "true")],
            dark: &[("AIDER_DARK_MODE", "true")],
        }),
        project_theme_seed: None,
        session_id_flag: None,
        env: &[],
        needs_light_console_fix: false,
    },
];

/// The registry entry for an executable, if ADE knows it. One lookup (DRY) behind
/// every per-agent question.
fn definition(command: &str) -> Option<&'static AgentDefinition> {
    REGISTRY.iter().find(|agent| agent.command == command)
}

/// Every executable name `command` could be installed under: its own name first,
/// then the registry's aliases (just the name itself for a shell or a command ADE
/// doesn't know, e.g. a task runner).
fn installed_names(command: &str) -> Vec<&str> {
    let mut names = vec![command];
    if let Some(definition) = definition(command) {
        names.extend(definition.aliases);
    }
    names
}

/// The executable to actually run for `command`, or `None` if it isn't installed.
///
/// Everything that needs a real program goes through here — detection, spawning a
/// session, running a headless one-shot — so an agent is found and launched by the
/// same rules, and an install ADE can *see* is always one it can *run*. Resolving
/// to an absolute path (rather than handing a bare name to the child process) is
/// what lets a session start from a directory that only the live PATH knows about.
pub fn program(command: &str) -> Option<PathBuf> {
    find_in(&search_dirs(), &installed_names(command))
}

/// How to invoke `command` headlessly for a one-shot prompt (auto-naming), if we
/// know a way. Keeps the registry the single source of truth (DRY).
pub fn oneshot_invocation(command: &str) -> Option<&'static [&'static str]> {
    definition(command).and_then(|agent| agent.oneshot)
}

/// Environment variables to set when spawning `command` in a PTY. Empty for an
/// unknown command or a plain shell, so `pty.rs` stays agent-agnostic.
pub fn spawn_env(command: &str) -> &'static [(&'static str, &'static str)] {
    definition(command).map_or(&[], |agent| agent.env)
}

/// Args to launch an interactive session of `command` with — the CLI's
/// skip-every-permission ("yolo") flag(s), so ADE runs it autonomously. Empty for
/// an unknown command or a plain shell (which has nothing to bypass), so `pty.rs`
/// stays agent-agnostic.
pub fn session_args(command: &str) -> &'static [&'static str] {
    definition(command).map_or(&[], |agent| agent.session_args)
}

/// The flag that pins `command`'s session to a caller-chosen conversation id
/// (and resumes that exact conversation on a later spawn with the same id) —
/// e.g. `--session-id` for Claude. `None` for a CLI with no equivalent, in which
/// case ADE can't target a specific conversation and won't try. See
/// `session_id_flag` on the registry entry.
pub fn session_id_flag(command: &str) -> Option<&'static str> {
    definition(command).and_then(|agent| agent.session_id_flag)
}

/// How `command`'s own UI theme is forced to ADE's scheme, if the registry
/// knows a mechanism (`theming.rs` interprets it). `None` for an unknown
/// command or a CLI with no theme setting.
pub fn theme_config(command: &str) -> Option<&'static ThemeConfig> {
    definition(command).and_then(|agent| agent.theme_config.as_ref())
}

/// Project-local adaptive-theme seed for `command`, when its CLI exposes a
/// documented project setting safe to create without choosing a fixed palette.
/// The theming layer owns the generic create-only file operation.
pub fn project_theme_seed(command: &str) -> Option<ProjectThemeSeed> {
    definition(command).and_then(|agent| agent.project_theme_seed)
}

/// Whether `command`'s input composer box follows the *detected* terminal
/// background (rather than a config/env theme), and so needs ADE's Windows
/// light-console workaround. `pty.rs` gates the `cmd /c color F0 & …` prefix on
/// this plus `cfg!(windows)` and a light scheme. `false` for an unknown command
/// or a CLI that themes its box some other way.
pub fn needs_light_console_fix(command: &str) -> bool {
    definition(command).is_some_and(|agent| agent.needs_light_console_fix)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    id: String,
    label: String,
    command: String,
}

/// Every installed agent backend. The shell fallback is appended so the list is
/// never empty (there is always something to launch).
///
/// Async + `spawn_blocking`: detection reads the live PATH and stats a few hundred
/// candidate files, which is slow enough that running it synchronously would block
/// Tauri's main thread — and the main thread also drives Windows' window move loop,
/// so a sync detect fired on window-focus stalls dragging. Off-thread it can't.
#[tauri::command]
pub async fn agents_detect() -> Vec<Agent> {
    tauri::async_runtime::spawn_blocking(detect_installed)
        .await
        .unwrap_or_default()
}

fn detect_installed() -> Vec<Agent> {
    // One search path for the whole sweep — it costs a registry read to build.
    let directories = search_dirs();
    let mut found: Vec<Agent> = REGISTRY
        .iter()
        .filter(|agent| find_in(&directories, &installed_names(agent.command)).is_some())
        .map(|agent| Agent {
            id: agent.id.into(),
            label: agent.label.into(),
            command: agent.command.into(),
        })
        .collect();

    let shell = if cfg!(windows) {
        "powershell.exe"
    } else {
        "bash"
    };
    found.push(Agent {
        id: "shell".into(),
        label: "Terminal (shell)".into(),
        command: shell.into(),
    });
    found
}

#[cfg(test)]
mod tests {
    use super::{
        installed_names, needs_light_console_fix, oneshot_invocation, session_args,
        session_id_flag, spawn_env, theme_config,
    };
    use crate::theming::ThemeConfig;

    #[test]
    fn installed_names_lead_with_the_canonical_command() {
        let names = installed_names("codex");
        assert_eq!(names.first(), Some(&"codex"));
        assert!(names.contains(&"codex-x86_64-pc-windows-msvc"));
    }

    /// A command ADE has no entry for — a task runner, an editor — resolves under
    /// its own name and carries no agent baggage.
    #[test]
    fn an_unknown_command_is_only_ever_itself() {
        assert_eq!(installed_names("pnpm"), vec!["pnpm"]);
        assert!(spawn_env("pnpm").is_empty());
        assert!(oneshot_invocation("pnpm").is_none());
        assert!(session_args("pnpm").is_empty());
        assert!(theme_config("pnpm").is_none());
        assert!(session_id_flag("pnpm").is_none());
    }

    /// Claude can pin/resume a conversation by id; agents without the flag can't
    /// be targeted, so ADE leaves them out of the restart-to-resume flow.
    #[test]
    fn only_agents_with_a_session_id_flag_can_be_resumed_by_id() {
        assert_eq!(session_id_flag("claude"), Some("--session-id"));
        assert!(session_id_flag("codex").is_none());
        assert!(session_id_flag("powershell.exe").is_none());
    }

    /// Claude and the other env-themed CLIs declare spawn pairs; Codex declares
    /// launch args.
    #[test]
    fn theme_mechanisms_match_each_agent() {
        assert!(matches!(
            theme_config("claude"),
            Some(ThemeConfig::SpawnEnv { .. })
        ));
        assert!(matches!(
            theme_config("aider"),
            Some(ThemeConfig::SpawnEnv { .. })
        ));
        // Codex themes its whole TUI from a named `[tui] theme`; ADE forces
        // Codex's own light/dark defaults per launch with `-c tui.theme=…`.
        assert!(matches!(
            theme_config("codex"),
            Some(ThemeConfig::SpawnArgs {
                light: &["-c", "tui.theme=catppuccin-latte"],
                dark: &["-c", "tui.theme=catppuccin-mocha"],
            })
        ));
        // No verified mechanism → deliberately unforced.
        assert!(theme_config("grok").is_none());
    }

    /// Interactive sessions launch in the agent's skip-permissions mode; an
    /// unknown command (a shell, a task runner) has nothing to bypass.
    #[test]
    fn session_args_carry_the_skip_permissions_flag() {
        assert_eq!(session_args("claude"), &["--dangerously-skip-permissions"]);
        // Codex also gets forced onto the alternate screen so it renders fullscreen
        // like Claude (see the codex AgentDef); the `-c` pair rides `session_args`.
        assert_eq!(
            session_args("codex"),
            &[
                "--dangerously-bypass-approvals-and-sandbox",
                "-c",
                "tui.alternate_screen=always"
            ]
        );
        // Copilot is likewise pinned to the alternate screen so it renders
        // fullscreen; `--alt-screen on` rides its session args after the
        // tool-approval flag (see the copilot AgentDef).
        assert_eq!(
            session_args("copilot"),
            &["--allow-all-tools", "--alt-screen", "on"]
        );
        assert!(session_args("powershell.exe").is_empty());
        // Keyed by the canonical command, not the file an installer laid down.
        assert!(session_args("codex-x86_64-pc-windows-msvc").is_empty());
    }

    /// Per-agent knowledge stays keyed by the canonical command, never by the
    /// executable an installer happened to lay down.
    #[test]
    fn agent_knowledge_is_keyed_by_the_canonical_command() {
        assert_eq!(oneshot_invocation("codex"), Some(&["exec"][..]));
        assert!(oneshot_invocation("codex-x86_64-pc-windows-msvc").is_none());
        assert_eq!(
            oneshot_invocation("grok"),
            Some(&["--no-auto-update", "-p"][..])
        );
        assert_eq!(spawn_env("claude"), &[("CLAUDE_CODE_NO_FLICKER", "1")]);
    }

    /// opencode's whole registry surface: `run` is its headless one-shot,
    /// `--auto` its skip-permissions mode, and its `system` theme follows the
    /// terminal — but `ConPTY` poisons the OSC 10/11 probe its light/dark
    /// detection rides, so a light ADE forces the mode-independent `pade-light`
    /// theme via a per-spawn `OPENCODE_TUI_CONFIG` file while dark needs no
    /// override. No session-id handle.
    #[test]
    fn opencode_runs_headless_via_run_and_autonomous_via_auto() {
        assert_eq!(oneshot_invocation("opencode"), Some(&["run"][..]));
        assert_eq!(session_args("opencode"), &["--auto"]);
        let Some(ThemeConfig::SpawnTuiConfig {
            variable,
            light,
            dark,
        }) = theme_config("opencode")
        else {
            panic!("opencode should force its theme via SpawnTuiConfig");
        };
        assert_eq!(*variable, "OPENCODE_TUI_CONFIG");
        assert_eq!(
            *light,
            Some(r#"{"$schema":"https://opencode.ai/tui.json","theme":"pade-light"}"#)
        );
        assert!(dark.is_none());
        // `--session <id>` only continues an existing session, so opencode has
        // no restart-to-resume handle.
        assert!(session_id_flag("opencode").is_none());
        assert!(spawn_env("opencode").is_empty());
        // ConPTY resize refills paint from the console buffer, so a light ADE
        // keeps the console-buffer flip (like Codex).
        assert!(needs_light_console_fix("opencode"));
        // Installed under its own name only — no installer aliases.
        assert_eq!(installed_names("opencode"), vec!["opencode"]);
    }
}
