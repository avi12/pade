//! What ADE knows about each agent CLI: the `AgentDefinition` shape and the
//! `REGISTRY` table of entries (command, aliases, one-shot and session
//! arguments, theme mechanism, session-id flag), plus the PADE-owned theme
//! definitions those entries select. Adding a backend is one entry here.

use super::{
    SessionIdFlag, ID_AIDER, ID_ANTIGRAVITY, ID_CLAUDE, ID_CODEX, ID_COPILOT, ID_CURSOR, ID_GROK,
    ID_MUSE, ID_OPENCODE, ID_QWEN,
};
use crate::terminal::theming::ThemeConfig;

pub(super) struct AgentDefinition {
    pub(super) id: &'static str,
    pub(super) label: &'static str,
    /// The executable to look for and run - the name the CLI's own docs use, and
    /// the one ADE stores and shows.
    pub(super) command: &'static str,
    /// Other executable names the same CLI is installed under. Installers don't
    /// agree: npm, Homebrew and cargo all drop a plain `codex`, but winget's
    /// portable package unpacks the vendor's release binary under its raw
    /// target-triple name and never creates a `codex.exe` at all. Without the
    /// alias, a real install is invisible.
    pub(super) aliases: &'static [&'static str],
    /// Args that make the CLI answer one prompt non-interactively and exit, with
    /// the prompt appended as the final arg (used for auto-naming). `None` = no
    /// headless mode we can drive.
    pub(super) oneshot: Option<&'static [&'static str]>,
    /// Args that launch an *interactive* session in the CLI's "skip every
    /// permission prompt" / yolo mode, so ADE drives the agent autonomously -
    /// no per-tool, per-edit approval stops. Empty for a CLI with no such flag.
    /// Distinct from `oneshot` (headless) and from the first-run trust gate,
    /// which these flags do NOT dismiss (ADE accepts that separately - see the
    /// frontend's initial-prompt delivery).
    pub(super) session_arguments: &'static [&'static str],
    /// How to force this agent's own UI theme to ADE's scheme - the terminal
    /// protocol can't carry it through `ConPTY` (see `theming.rs`). `None` for a
    /// CLI with no known theme setting.
    pub(super) theme_config: Option<ThemeConfig>,
    /// The flag that binds a session to a caller-chosen conversation id
    /// (`claude --session-id <uuid>`): it creates a fresh conversation with that
    /// id, and a later spawn with the same id *resumes* that exact one -
    /// non-interactively. That is what lets ADE terminate a specific session and
    /// bring back ITS conversation (e.g. after the project's `.mcp.json` changed),
    /// not merely the most recent. `None` for a CLI with no equivalent.
    pub(super) session_id_flag: Option<SessionIdFlag>,
    /// Environment the CLI needs to render the way ADE embeds it. Empty for most.
    pub(super) environment: &'static [(&'static str, &'static str)],
    /// Whether this CLI paints its own UI (its input composer box) from the
    /// *detected* terminal background and so needs ADE's Windows light-console
    /// workaround. Under `ConPTY` the detected background is the pseudoconsole's
    /// hard-coded BLACK buffer that no env/arg/config/OSC channel can change, so on
    /// a light ADE the box clashes; `pty.rs` prefixes `cmd /c color F0 & …` (a
    /// child-side console write) only for these agents, only on Windows, only when
    /// the scheme is light. `true` for Codex; `false` for CLIs whose box follows a
    /// config/env theme (Claude, aider, cursor) or that inherit the palette.
    pub(super) needs_light_console_fix: bool,
    /// Whether this CLI runs the Unicode bidirectional algorithm over its OWN
    /// output, so the Arabic and Hebrew that reaches the terminal is already in
    /// **visual** order rather than the logical order a program normally writes.
    ///
    /// It inverts what the terminal must do. ADE's right-to-left overlay
    /// (`lib/terminal/rtl/terminal-rtl`) hands a run to the browser and lets it reorder -
    /// correct for every program that writes logical order, and exactly wrong
    /// here, because reordering already-reordered text puts it back the way it
    /// started and the reader sees the sentence backwards. So the overlay is not
    /// drawn for these agents: their output is already right, and the terminal's
    /// job is to paint it cell for cell.
    ///
    /// Measured, never assumed - the running app's buffer read against the
    /// agent's own transcript (see `docs/terminal-rendering.md`).
    pub(super) reorders_bidi: bool,
}

/// The PADE-owned Claude theme. Claude Code watches its user theme directory and
/// re-renders when a definition in use changes, so ONE file PADE rewrites is a
/// live channel into every running session. The definition is inert until a
/// session selects it, and only PADE's own sessions do - through `--settings`,
/// per launch, so no file of the user's names it and their own terminals are
/// untouched. The `custom:` selector, the file stem and the `name` inside the
/// file must agree; `theming::tests` asserts they do.
const PADE_CLAUDE_THEME_PATH: &str = ".claude/themes/pade.json";
const PADE_CLAUDE_THEME_SETTINGS: &str = r#"{"theme":"custom:pade"}"#;
/// The base is the **ANSI** one on purpose. Claude renders an `-ansi` theme in
/// the terminal's own sixteen colours rather than in truecolor of its own, and
/// those sixteen are exactly what PADE paints (`--terminal-*`: the app palette,
/// or the Windows Terminal scheme the user chose for it). So the agent tracks
/// the terminal's colours continuously - a scheme swap repaints it on the next
/// frame with nothing published at all, because the bytes it already wrote name
/// colour *slots*, and the slots moved. Only light-vs-dark still has to be
/// written here, and that is what this file carries.
const PADE_CLAUDE_LIGHT_THEME: &str = r#"{"name":"pade","base":"light-ansi","overrides":{}}"#;
const PADE_CLAUDE_DARK_THEME: &str = r#"{"name":"pade","base":"dark-ansi","overrides":{}}"#;

/// The TUI configs `OPENCODE_TUI_CONFIG` points opencode at - one per scheme,
/// each selecting the matching PADE-owned plain-string theme (theming.rs
/// materializes both definitions). Plain-string, not adaptive, so the render is
/// truthful regardless of opencode's ConPTY-answered light/dark probe; a flip
/// re-themes by respawn (App's `restartSpawnThemedAgents`).
const OPENCODE_LIGHT_TUI_CONFIG: &str =
    r#"{"$schema":"https://opencode.ai/tui.json","theme":"pade-light"}"#;
const OPENCODE_DARK_TUI_CONFIG: &str =
    r#"{"$schema":"https://opencode.ai/tui.json","theme":"pade-dark"}"#;

/// Known agent backends, in preferred display order. The plain shell is always
/// offered last as a universal fallback.
///
/// **Forcing fullscreen.** ADE's terminal host runs both screens (see
/// `docs/terminal-rendering.md`). Where a CLI can be pinned to alt-screen *per
/// launch*, ADE does so through whichever field the CLI already exposes - and never
/// a user-global config file, which would leak ADE's choice into the user's other
/// terminals: Codex and Copilot via `session_arguments`
/// (`-c tui.alternate_screen=always` / `--alt-screen on`), each overriding a saved
/// default that could otherwise drop to inline. **Claude is deliberately not
/// forced** - it is the one agent whose renderer decides who owns the *scrollback*,
/// and taking that from the terminal is what makes scrolling cost a round trip
/// through the agent (see its entry below). Grok and
/// Antigravity already render alt-screen by default on a local terminal and expose
/// no safe per-launch override (only a slash command or a user-global settings
/// file), so ADE leaves them as-is; aider is a line-oriented REPL with no fullscreen
/// mode; cursor-agent's rendering is undocumented - none of those three is forced.
pub(super) const REGISTRY: &[AgentDefinition] = &[
    AgentDefinition {
        id: ID_CLAUDE,
        label: "Claude Code",
        command: "claude",
        aliases: &[],
        oneshot: Some(&["-p"]),
        // Bypass every per-tool/edit approval. The trust gate remains
        // frontend-owned.
        session_arguments: &["--dangerously-skip-permissions"],
        // The one agent ADE can re-theme WITHOUT relaunching it: the session
        // launches on ADE's own theme definition, and rewriting that definition
        // repaints every live session. Measured against 2.1.227 under a real
        // ConPTY - a rewrite repaints an idle session within a second, where the
        // `theme` *setting* is read once at mount and the OSC 11 answer its
        // `?997` doorbell asks for never escapes ConPTY (docs/terminal-rendering.md).
        theme_config: Some(ThemeConfig::SpawnSelectedLiveTheme {
            arguments: &["--settings", PADE_CLAUDE_THEME_SETTINGS],
            relative_path: PADE_CLAUDE_THEME_PATH,
            light: PADE_CLAUDE_LIGHT_THEME,
            dark: PADE_CLAUDE_DARK_THEME,
        }),
        // `--session-id <uuid>` creates the conversation with that id and
        // `--resume <uuid>` brings it back (non-interactive), so ADE can restart
        // a specific session and land back in its own conversation - see
        // `session_id_flag`.
        session_id_flag: Some(SessionIdFlag::CreateOrResume {
            create: "--session-id",
            resume: "--resume",
        }),
        // ADE does NOT force Claude's renderer - this is the one agent where the
        // choice is the user's, because it decides who owns the scrollback.
        //
        // Claude's `tui` setting picks between two renderers: `fullscreen` (the
        // alt-screen one, equivalent to `CLAUDE_CODE_NO_FLICKER=1`, with its own
        // *virtualized* scrollback) and `default` (the classic main-screen one).
        // ADE used to force fullscreen by env precisely so the user's setting could
        // not undo it. That is not ADE's call: on the alternate screen the
        // conversation lives inside Claude, so the terminal holds nothing to scroll
        // and every wheel tick has to become a request. Measured in the live app,
        // per notch: **31ms** round-tripping through IPC, the PTY, a whole agent
        // repaint and back, against **9ms** on the main screen, where xterm owns the
        // document and the tick is a local viewport scroll.
        //
        // `CLAUDE_CODE_NO_FLICKER` outranks the `tui` setting, and a user's
        // `settings.json` `env` block sets it for every Claude everywhere - so a
        // config reading `tui: "default"` can still be fullscreen in every terminal.
        // Check the `env` block before blaming ADE for a session's renderer.
        //
        // So: set nothing, exactly as Windows Terminal sets nothing, and let Claude
        // resolve its own `tui`. A user who wants the polished fullscreen TUI sets
        // `tui: "fullscreen"` and gets it (the alt-screen host in `Terminal.svelte`
        // is unchanged and still runs for Codex, pagers, and that choice).
        environment: &[],
        // Claude's box follows its `theme` settings key, not the terminal probe.
        needs_light_console_fix: false,
        // Claude Code reorders its own Arabic and Hebrew before it writes them:
        // it splits each row into cells, runs bidi-js over the row with an `auto`
        // paragraph direction, and reverses the cells by embedding level. Measured
        // on 2.1.227 - unconditional, and the gate it consults has no path that
        // answers no, so nothing ADE can pass turns it off.
        reorders_bidi: true,
    },
    AgentDefinition {
        id: ID_CODEX,
        label: "Codex",
        command: "codex",
        // OpenAI publishes Codex as bare release binaries named by target triple,
        // and winget's package installs one verbatim - so on a winget machine the
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
        // drops the sandbox - the price of a fully autonomous run.
        //
        // `-c tui.alternate_screen=always` forces Codex fullscreen on the terminal's
        // ALTERNATE screen, like Claude - which is what `Terminal.svelte` is tuned to
        // host (its wheel handler forwards a wheel tick as PageUp/PageDown for a
        // fullscreen agent with no xterm scrollback, and Codex scrolls its transcript
        // on PageUp/PageDown). Codex's default `auto` mode drops to inline rendering
        // under some environments (e.g. Zellij) to preserve scrollback, leaving it
        // NOT fullscreen and unscrollable in ADE; `always` overrides that heuristic
        // (see openai/codex `AltScreenMode`). The `-c` override applies to THIS spawn
        // only and never rewrites the user's `~/.codex/config.toml`. This is not
        // scheme-dependent, so it's a constant launch arg here - not in `theme_config`
        // (which is scheme-keyed) and not on the `codex exec` oneshot (which renders
        // no TUI); `session_arguments` is interactive-session-only, so this is its home.
        session_arguments: &[
            "--dangerously-bypass-approvals-and-sandbox",
            "-c",
            "tui.alternate_screen=always",
        ],
        // `[tui] theme` selects one of Codex's bundled *syntax-highlight* themes
        // (syntect/two_face) - code spans, diffs, markdown. Codex reads it once at
        // startup and has no live light/dark toggle, so ADE forces it per launch
        // with the global `-c tui.theme=<name>` override: it wins over
        // `~/.codex/config.toml` for THIS spawn only, never rewriting the user's
        // chosen theme on disk. The two names are Codex's own adaptive defaults -
        // what it auto-selects for a light vs dark terminal background (see
        // openai/codex `highlight.rs` `adaptive_default_theme_selection`) - so
        // ADE's scheme wins with Codex's own neutral picks.
        //
        // What `tui.theme` does NOT control: the input composer / user-message box
        // background. Codex fills that from `style::user_message_bg(default_bg())`
        // - a tint blended over the *detected terminal background*, not the syntax
        // theme (openai/codex `style.rs`; verified: a user's own light
        // `inspired-github` global theme still yields a dark composer box on ADE).
        // `default_bg()` comes from a terminal probe (`terminal_probe.rs`): an
        // OSC 10/11 query, then a `GetConsoleScreenBufferInfoEx` fallback. Under
        // Windows ConPTY both resolve to the pseudoconsole's hard-coded BLACK
        // buffer regardless of ADE's scheme - the OSC query is answered by ConPTY
        // itself (never reaching ADE's xterm, matching theming.rs's live finding),
        // and the console buffer's background is Black (empirically confirmed via
        // portable-pty: `GetConsoleScreenBufferInfoEx` reports index-0 Black, and
        // it is unaffected by an OSC 11 SET written to the pty master). So on a
        // light ADE the composer box would stay dark, and no env/args/config/OSC
        // channel can feed Codex a light `default_bg` on Windows - only a
        // child-side console write can. ADE does exactly that via the
        // `needs_light_console_fix` flag below: `pty.rs` prefixes the Windows
        // light-scheme launch with `cmd /c color F0 & …`, which flips the console
        // buffer light *before* Codex probes it (verified end-to-end: the composer
        // fill goes from `48;2;41;41;41` to `48;2;232;232;232`). `tui.theme` is a
        // separate concern and stays forced because it matches syntax highlighting
        // to ADE's scheme.
        // Like SpawnEnvironment, everything above is read once at startup. A running
        // session keeps its syntax choice until it naturally exits; ADE still
        // re-themes the terminal palette in place so it never destroys a live
        // conversation just to follow an app appearance change.
        theme_config: Some(ThemeConfig::SpawnArguments {
            light: &["-c", "tui.theme=catppuccin-latte"],
            dark: &["-c", "tui.theme=catppuccin-mocha"],
        }),
        session_id_flag: None,
        environment: &[],
        // Codex paints its input composer box from the detected terminal
        // background (`default_bg()`), which under Windows ConPTY is the
        // pseudoconsole's hard-coded BLACK buffer - so on a light ADE the box
        // clashes and no clean channel can fix it. `pty.rs` forces the console
        // buffer light before Codex probes it (Windows + light scheme only).
        needs_light_console_fix: true,
        reorders_bidi: false,
    },
    AgentDefinition {
        id: ID_OPENCODE,
        label: "OpenCode",
        command: "opencode",
        aliases: &[],
        // `opencode run <message>` answers one prompt headlessly and exits, with
        // the prompt appended as the final arg - the shape auto-naming drives.
        oneshot: Some(&["run"]),
        // `--auto` auto-approves every permission not explicitly denied -
        // opencode's yolo switch. Its TUI owns the alternate screen by default,
        // so no fullscreen pin is needed.
        session_arguments: &["--auto"],
        // opencode exposes no per-launch theme flag or env var, and it decides
        // light vs dark ONLY from OSC 10/11 query replies - which under Windows
        // ConPTY are answered by the console layer's hard-coded black buffer, so
        // its mode probe always reads dark and no env var, config key, or later
        // `?997` report flips it (verified: on a light ADE an adaptive theme
        // still rendered its dark variant). The fix sidesteps mode detection:
        // `OPENCODE_TUI_CONFIG` points opencode at a PADE-owned tui config that
        // merges OVER the user's own ~/.config/opencode/tui.json (user file
        // order=1, env file order=2 - keybinds survive, only `theme` is
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
        // `--session <id>` only *continues* an existing session; it cannot create
        // one with a caller-chosen id, so restart-to-resume has no handle here.
        session_id_flag: None,
        environment: &[],
        // The `pade-light` theme fixes the colors, but ConPTY resize refills
        // still paint from the console buffer - so a light ADE keeps the
        // console-buffer flip (like Codex) to stop dark flashes on reflow.
        needs_light_console_fix: true,
        reorders_bidi: false,
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
        // screen, like Claude and Codex - the mode `Terminal.svelte` is tuned to host.
        // Alt-screen is Copilot's default for interactive sessions, but the CLI
        // persists display preferences to the user-level `~/.copilot/settings.json`
        // (a user can turn alt-screen off and it sticks), so `on` pins it for THIS
        // spawn and can't be undone by the saved config - the same reasoning as
        // Codex forcing `tui.alternate_screen=always` over its `auto` default. The
        // flag is documented in Copilot CLI's command reference (default `on`); it
        // rides `session_arguments` after the tool-approval flag, its authoritative home.
        session_arguments: &["--allow-all-tools", "--alt-screen", "on"],
        // Copilot CLI auto-follows the system light/dark mode since v1.0.62;
        // forcing it would mean writing the USER-level ~/.copilot/settings.json,
        // which would leak ADE's scheme into every other terminal. Left alone.
        theme_config: None,
        // `--resume=<uuid>` starts a NEW session under that id when none exists
        // and resumes it on a later spawn (verified on 1.0.34) - which is also
        // how `running_model` finds this session's event log by name. Joined,
        // because `--resume` alone opens the session picker.
        session_id_flag: Some(SessionIdFlag::Joined("--resume")),
        environment: &[],
        needs_light_console_fix: false,
        reorders_bidi: false,
    },
    AgentDefinition {
        id: ID_GROK,
        label: "Grok CLI",
        command: "grok",
        aliases: &[],
        // xAI's Grok Build answers a single prompt with `-p <PROMPT>`, the same
        // shape as Claude. `--no-auto-update` goes first because a one-shot naming
        // run is exactly the headless, automated case the CLI's own docs say to
        // pass it for - it skips the background update check that would otherwise
        // risk blowing NAME_TIMEOUT before the name comes back.
        oneshot: Some(&["--no-auto-update", "-p"]),
        // xAI Grok Build's "auto-approve all tool executions" (alias `--yolo`).
        session_arguments: &["--always-approve"],
        // The settings reference lists no theme/color/appearance key at all;
        // its TUI inherits the terminal palette.
        theme_config: None,
        session_id_flag: None,
        environment: &[],
        needs_light_console_fix: false,
        reorders_bidi: false,
    },
    AgentDefinition {
        id: ID_MUSE,
        label: "Muse Code",
        // Meta's terminal coding agent on Muse Spark; its installers (`install.sh`,
        // `install.ps1` from dev.meta.ai) drop a plain `muse`.
        command: "muse",
        aliases: &[],
        // `muse exec "<prompt>"` runs one prompt to completion - Meta's documented
        // headless mode for scripts and CI.
        oneshot: Some(&["exec"]),
        // Meta's permissions page: `--yolo` disables approval and the sandbox and
        // trusts the workspace - the Unrestricted profile.
        session_arguments: &["--yolo"],
        // No documented theme setting or variable; its TUI inherits the palette.
        theme_config: None,
        // Only `muse resume --last` is documented - no caller-chosen id.
        session_id_flag: None,
        environment: &[],
        needs_light_console_fix: false,
        reorders_bidi: false,
    },
    AgentDefinition {
        id: ID_QWEN,
        label: "Qwen Code",
        // Alibaba's Qwen Code (`npm i -g @qwen-code/qwen-code`), a fork of Gemini
        // CLI; npm lays down `qwen` / `qwen.cmd`.
        command: "qwen",
        aliases: &[],
        // A positional prompt is one-shot by default (`-p` is deprecated in its
        // favour, verified on 0.15.1), so the prompt alone, appended last, is the
        // whole headless invocation.
        oneshot: Some(&[]),
        // "Automatically accept all actions (aka YOLO mode)".
        session_arguments: &["--yolo"],
        // Its `ui.theme` lives in the user-level settings.json only - forcing it
        // would leak ADE's scheme into the user's other terminals.
        theme_config: None,
        // `--session-id` names a NEW session; resuming one takes `--resume <id>`,
        // a different flag - so one id can't both create and resume, which the
        // restart-to-resume flow needs.
        session_id_flag: None,
        environment: &[],
        needs_light_console_fix: false,
        reorders_bidi: false,
    },
    AgentDefinition {
        id: ID_ANTIGRAVITY,
        label: "Antigravity CLI",
        command: "antigravity",
        aliases: &[],
        oneshot: None,
        // No verified bypass flag for this CLI yet - left off rather than guess a
        // wrong flag (an unknown flag makes the whole session fail to launch).
        session_arguments: &[],
        // Its `colorScheme` lives in the USER-level settings file, needs a
        // restart to apply, and only "dark" is documented verbatim - not enough
        // verified surface to force safely.
        theme_config: None,
        session_id_flag: None,
        environment: &[],
        needs_light_console_fix: false,
        reorders_bidi: false,
    },
    AgentDefinition {
        id: ID_CURSOR,
        label: "Cursor CLI",
        command: "cursor-agent",
        aliases: &[],
        oneshot: None,
        // Cursor's own permissions docs name `--force` as the run-without-prompts
        // switch; deny rules still take precedence.
        session_arguments: &["--force"],
        // No documented theme setting; Cursor staff recommend TERM_THEME=light
        // on light-background terminals (fixes an invisible input cursor). The
        // dark side needs no help - its palette assumes a dark terminal.
        theme_config: Some(ThemeConfig::SpawnEnvironment {
            light: &[("TERM_THEME", "light")],
            dark: &[],
        }),
        session_id_flag: None,
        environment: &[],
        needs_light_console_fix: false,
        reorders_bidi: false,
    },
    AgentDefinition {
        id: ID_AIDER,
        label: "aider",
        command: "aider",
        aliases: &[],
        oneshot: None,
        // aider's "always say yes to every confirmation".
        session_arguments: &["--yes-always"],
        // aider reads its theme once at startup; the documented env pair beats
        // writing a `.aider.conf.yml` into the repo root (which would dirty the
        // user's git status).
        theme_config: Some(ThemeConfig::SpawnEnvironment {
            light: &[("AIDER_LIGHT_MODE", "true")],
            dark: &[("AIDER_DARK_MODE", "true")],
        }),
        session_id_flag: None,
        environment: &[],
        needs_light_console_fix: false,
        reorders_bidi: false,
    },
];
