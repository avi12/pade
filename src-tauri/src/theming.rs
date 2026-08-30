//! Force each installed agent's own UI theme to match ADE's light/dark scheme.
//!
//! Why a file and spawn-time signals, and never the terminal protocol: an
//! agent's `auto` theme follows the *terminal* — Claude Code resolves its scheme
//! as `live override ?? $COLORFGBG ?? dark`, and the live override is only ever
//! set by the answer to an OSC 11 background-color query. On Windows that query
//! never reaches the terminal emulator, so the override is never set and an
//! `auto` theme is frozen at whatever the environment said on launch day. ADE
//! therefore never leaves an agent on `auto`: it names the theme itself.
//!
//! **The DECSET 2031 `?997` relay cannot re-theme a running agent on Windows,
//! and it is a mistake to read it as if it could.** Measured against the real
//! binary under a real `ConPTY` (see `docs/terminal-rendering.md`): Claude does
//! subscribe with `?2031h`, but its report handler *discards the scheme the
//! report carries* and re-probes with OSC 11 instead — the report is only a
//! doorbell. The doorbell arrives (the agent's own debug log records a query
//! attempt within a second of it), but the query it provokes never escapes
//! `ConPTY`, so no terminal-side handler can answer it. Neither can one answer
//! blind: an unsolicited OSC 11 reply written into the PTY does not reach the
//! agent's parser either. Both were measured, twice, on 2.1.220 and 2.1.227.
//!
//! **The palette itself needs no channel at all.** PADE's Claude theme names an
//! **ANSI** base, so Claude paints in the terminal's own sixteen colours rather
//! than in truecolor of its own — and those sixteen are what PADE sets
//! (`--terminal-*`: the app palette, or the Windows Terminal scheme chosen for
//! the terminal). Changing that scheme moves the slots under bytes the agent
//! has already written, so a running session repaints in the new colours on the
//! next frame with nothing published. Only light-vs-dark still has to be told.
//!
//! **What re-themes a RUNNING session is a theme file the agent watches.** Claude
//! Code re-renders when a theme *definition* it is using changes on disk, so ADE
//! owns one definition (`~/.claude/themes/pade.json`), selects it per session
//! with `--settings` — never by writing a settings file of the user's — and
//! rewrites it on every scheme flip. Measured: an idle session repaints into the
//! other scheme within a second, with its conversation untouched. That is
//! `SpawnSelectedLiveTheme`, and `publish_live_themes` is what writes it.
//!
//! Every other CLI is themed at spawn only, through whichever knob it exposes:
//! per-scheme env (aider, cursor-agent), per-scheme launch args (codex), or — for
//! a CLI with neither (opencode) — a whole TUI-config file selected per spawn via
//! an env var, naming a custom per-scheme theme whose colors are plain strings so
//! the ConPTY-poisoned probe stops mattering. All of it is per session, never via
//! a user-global config file that would leak ADE's choice into the user's other
//! terminals. A spawn-time theme cannot follow a mid-session flip, so for those
//! agents the frontend respawns the idle ones (App's `restartSpawnThemedAgents`,
//! each resuming its own conversation) while ADE re-themes xterm's palette in
//! place.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// ADE's resolved appearance — the frontend's `appearance.scheme`, on the wire.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Light,
    Dark,
}

impl Scheme {
    /// The scheme's lowercase name, for per-scheme file names.
    fn as_str(self) -> &'static str {
        match self {
            Scheme::Light => "light",
            Scheme::Dark => "dark",
        }
    }
}

/// How one agent's theme is forced — registry knowledge, declared per agent in
/// `agents.rs` and interpreted here (`SoC`: the registry knows *what*, this
/// module knows *how to apply it*).
// The shared `Spawn` prefix is the point, not noise: every mechanism is
// spawn-time (read once at launch, next spawn picks up a scheme flip), and the
// prefix keeps that contract in every variant's name.
#[allow(clippy::enum_variant_names)]
pub enum ThemeConfig {
    /// Set per-scheme environment when the session spawns — for a CLI whose
    /// theme is env-driven and read once at startup (`pty.rs` applies these;
    /// a scheme flip reaches it on the next spawn). Either side may be empty
    /// when the CLI only needs help on one scheme.
    SpawnEnv {
        light: &'static [(&'static str, &'static str)],
        dark: &'static [(&'static str, &'static str)],
    },
    /// Append per-scheme launch *arguments* when the session spawns — for a CLI
    /// whose theme is chosen by a command-line flag and read once at startup
    /// (`pty.rs` appends these to the interactive argv; a scheme flip reaches it
    /// on the next spawn). Either side may be empty when the CLI only needs help
    /// on one scheme.
    SpawnArgs {
        light: &'static [&'static str],
        dark: &'static [&'static str],
    },
    /// Point the CLI at a whole TUI-config *file* through an env var when the
    /// session spawns — for a CLI (opencode) with no theme flag and no theme
    /// env var, whose light/dark detection rides OSC 10/11 replies that `ConPTY`
    /// answers itself (always black), so no signal can flip its *mode*. Each
    /// scheme selects its own plain-string PADE theme, so the render is truthful
    /// regardless of that broken probe. The per-scheme values are the config
    /// file's *contents*; `None` means the scheme needs no override.
    /// `spawn_tui_config_env` materializes the file under PADE's own config dir
    /// and hands `pty.rs` the `(variable, path)` env pair.
    SpawnTuiConfig {
        variable: &'static str,
        light: Option<&'static str>,
        dark: Option<&'static str>,
    },
    /// Select a PADE-owned theme *definition* at spawn, and re-theme every
    /// RUNNING session by rewriting that definition — the one live theme channel
    /// any agent exposes on Windows (Claude Code, which watches its user theme
    /// directory and re-renders when a definition in use changes).
    ///
    /// The odd one out of this enum, and deliberately so: `args` only *name* the
    /// theme, so they carry no scheme and never change. The scheme lives in the
    /// file's contents, which is exactly what lets a flip reach a session that is
    /// already running — every other variant can only be read at launch.
    SpawnSelectedLiveTheme {
        /// Launch args that select the definition for this session alone (a
        /// `--settings` JSON string), so no settings file of the user's has to
        /// name it and their own terminals are untouched.
        args: &'static [&'static str],
        /// The definition's path under the user's home — PADE owns this file.
        relative_path: &'static str,
        light: &'static str,
        dark: &'static str,
    },
}

/// The per-scheme environment to spawn `command` with (empty for an agent
/// whose theme is arg-driven, or unknown). `pty.rs` applies it alongside the
/// static `agents::spawn_env`.
pub fn spawn_env(command: &str, scheme: Scheme) -> &'static [(&'static str, &'static str)] {
    match crate::agents::theme_config(command) {
        Some(ThemeConfig::SpawnEnv { light, dark }) => match scheme {
            Scheme::Light => light,
            Scheme::Dark => dark,
        },
        Some(
            ThemeConfig::SpawnArgs { .. }
            | ThemeConfig::SpawnTuiConfig { .. }
            | ThemeConfig::SpawnSelectedLiveTheme { .. },
        )
        | None => &[],
    }
}

/// The per-scheme launch arguments to spawn `command` with (empty for an agent
/// whose theme is env-driven, or unknown). `pty.rs` appends it to the
/// interactive session's argv, alongside `agents::session_args`.
pub fn spawn_args(command: &str, scheme: Scheme) -> &'static [&'static str] {
    match crate::agents::theme_config(command) {
        Some(ThemeConfig::SpawnArgs { light, dark }) => match scheme {
            Scheme::Light => light,
            Scheme::Dark => dark,
        },
        // Scheme-independent by design: these args name the definition, and its
        // contents — not the argv — carry the scheme (see the variant's doc).
        Some(ThemeConfig::SpawnSelectedLiveTheme { args, .. }) => args,
        Some(ThemeConfig::SpawnEnv { .. } | ThemeConfig::SpawnTuiConfig { .. }) | None => &[],
    }
}

/// Write every live-theme definition for `scheme`, repainting every running
/// session that selected one. Called on a scheme flip (`agent_theme_publish`)
/// and before each spawn, so a launching session finds the definition already
/// describing the current scheme. `write_if_stale` keeps an unchanged scheme
/// from touching the file — the agent watches it, and a no-op rewrite would
/// make it re-render for nothing.
pub fn publish_live_themes(scheme: Scheme) -> std::io::Result<()> {
    let Some(home) = crate::util::home_dir() else {
        return Err(std::io::Error::other("no home directory"));
    };

    for command in crate::agents::commands() {
        let Some(ThemeConfig::SpawnSelectedLiveTheme {
            relative_path,
            light,
            dark,
            ..
        }) = crate::agents::theme_config(command)
        else {
            continue;
        };
        let contents = match scheme {
            Scheme::Light => light,
            Scheme::Dark => dark,
        };
        write_if_stale(&home.join(relative_path), contents)?;
    }

    Ok(())
}

/// Re-theme every running live-themed session to `scheme`. The frontend calls
/// this the moment ADE's appearance flips; the agent's own file watcher does the
/// rest, so a conversation in flight is never restarted to follow the theme.
// `async` so Tauri runs it off the main thread: it writes to disk, and a
// synchronous command would do that on the Win32 message pump.
#[tauri::command]
pub async fn agent_theme_publish(scheme: Scheme) -> Result<(), String> {
    publish_live_themes(scheme).map_err(|error| error.to_string())
}

/// The custom opencode themes a `SpawnTuiConfig` selects — one per scheme. Every
/// color is a plain string (no `{light,dark}` variants), so opencode's
/// ConPTY-poisoned mode detection is irrelevant: whichever theme the spawn
/// selects renders verbatim. That is what keeps the initial paint truthful on
/// BOTH schemes; a mid-session flip re-themes by respawn (App's
/// `restartSpawnThemedAgents`), not by the agent following a live signal.
/// `pade-light` is Atom One Light, `pade-dark` Atom One Dark — pade-namespaced.
const PADE_LIGHT_THEME_FILE: &str = "pade-light.json";
const PADE_LIGHT_THEME_JSON: &str = r##"{"$schema":"https://opencode.ai/theme.json","theme":{"primary":"#4078f2","secondary":"#a626a4","accent":"#0184bc","error":"#e45649","warning":"#c18401","success":"#50a14f","info":"#0184bc","text":"#383a42","textMuted":"#696c77","background":"#fafafa","backgroundPanel":"#f0f0f1","backgroundElement":"#e5e5e6","border":"#d4d4d5","borderActive":"#a0a1a7","borderSubtle":"#e5e5e6","diffAdded":"#50a14f","diffRemoved":"#e45649","diffContext":"#696c77","diffHunkHeader":"#696c77","diffHighlightAdded":"#2d6b2c","diffHighlightRemoved":"#a8342a","diffAddedBg":"#e8f5e9","diffRemovedBg":"#fdecea","diffContextBg":"#fafafa","diffLineNumber":"#d4d4d5","diffAddedLineNumberBg":"#d7ecd8","diffRemovedLineNumberBg":"#f8d7d3","markdownText":"#383a42","markdownHeading":"#a626a4","markdownLink":"#4078f2","markdownLinkText":"#0184bc","markdownCode":"#50a14f","markdownBlockQuote":"#696c77","markdownEmph":"#c18401","markdownStrong":"#383a42","markdownHorizontalRule":"#d4d4d5","markdownListItem":"#4078f2","markdownListEnumeration":"#0184bc","markdownImage":"#4078f2","markdownImageText":"#0184bc","markdownCodeBlock":"#383a42","syntaxComment":"#a0a1a7","syntaxKeyword":"#a626a4","syntaxFunction":"#4078f2","syntaxVariable":"#e45649","syntaxString":"#50a14f","syntaxNumber":"#986801","syntaxType":"#c18401","syntaxOperator":"#0184bc","syntaxPunctuation":"#383a42"}}"##;
const PADE_DARK_THEME_FILE: &str = "pade-dark.json";
const PADE_DARK_THEME_JSON: &str = r##"{"$schema":"https://opencode.ai/theme.json","theme":{"primary":"#61afef","secondary":"#c678dd","accent":"#56b6c2","error":"#e06c75","warning":"#e5c07b","success":"#98c379","info":"#56b6c2","text":"#abb2bf","textMuted":"#828997","background":"#282c34","backgroundPanel":"#21252b","backgroundElement":"#2c313a","border":"#3e4451","borderActive":"#5c6370","borderSubtle":"#2c313a","diffAdded":"#98c379","diffRemoved":"#e06c75","diffContext":"#828997","diffHunkHeader":"#828997","diffHighlightAdded":"#6cbe6c","diffHighlightRemoved":"#e06c75","diffAddedBg":"#2b3328","diffRemovedBg":"#3a2a2a","diffContextBg":"#282c34","diffLineNumber":"#4b5263","diffAddedLineNumberBg":"#33402f","diffRemovedLineNumberBg":"#463131","markdownText":"#abb2bf","markdownHeading":"#c678dd","markdownLink":"#61afef","markdownLinkText":"#56b6c2","markdownCode":"#98c379","markdownBlockQuote":"#828997","markdownEmph":"#e5c07b","markdownStrong":"#abb2bf","markdownHorizontalRule":"#3e4451","markdownListItem":"#61afef","markdownListEnumeration":"#56b6c2","markdownImage":"#61afef","markdownImageText":"#56b6c2","markdownCodeBlock":"#abb2bf","syntaxComment":"#5c6370","syntaxKeyword":"#c678dd","syntaxFunction":"#61afef","syntaxVariable":"#e06c75","syntaxString":"#98c379","syntaxNumber":"#d19a66","syntaxType":"#e5c07b","syntaxOperator":"#56b6c2","syntaxPunctuation":"#abb2bf"}}"##;

/// Write `contents` to `path` only when the file is missing or differs, so a
/// spawn never churns the disk (or file-watcher events) with identical bytes.
fn write_if_stale(path: &Path, contents: &str) -> std::io::Result<()> {
    let is_current = std::fs::read_to_string(path).is_ok_and(|existing| existing == contents);
    if is_current {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)
}

/// Write the pade-namespaced theme + the per-scheme tui config, returning the
/// tui-config path to expose via the env variable. `themes_dir` is opencode's
/// user theme directory and `config_dir` PADE's own config dir — injected so
/// tests exercise this against temp dirs.
///
/// Why writing into the *user's* opencode themes dir doesn't violate the
/// never-write-user-global-config doctrine: the doctrine guards against ADE
/// changing what the user's own sessions render (the stale settings.json
/// `theme` keys the old file mechanism left behind). `pade-light.json` /
/// `pade-dark.json` are ADDITIVE, pade-namespaced theme *definitions* — inert
/// until a spawn's `OPENCODE_TUI_CONFIG` selects one. The user's own tui.json,
/// and therefore every opencode session the user launches outside ADE, is
/// untouched.
fn materialize_tui_config(
    contents: &str,
    scheme: Scheme,
    themes_dir: &Path,
    config_dir: &Path,
) -> std::io::Result<PathBuf> {
    write_if_stale(
        &themes_dir.join(PADE_LIGHT_THEME_FILE),
        PADE_LIGHT_THEME_JSON,
    )?;
    write_if_stale(&themes_dir.join(PADE_DARK_THEME_FILE), PADE_DARK_THEME_JSON)?;
    let tui_config_path = config_dir.join(format!("opencode-tui-{}.json", scheme.as_str()));
    write_if_stale(&tui_config_path, contents)?;
    Ok(tui_config_path)
}

/// The per-scheme `(variable, path)` env pair for a file-themed CLI (opencode),
/// with the referenced files freshly materialized on disk — empty for other
/// agents, an unknown command, or a scheme needing no override. Does filesystem
/// I/O: `pty.rs` calls it from `build_command`, before any session lock is
/// taken. On an I/O failure the override is dropped (the agent falls back to
/// its own terminal-detected theme) with a diagnostic on stderr.
pub fn spawn_tui_config_env(command: &str, scheme: Scheme) -> Vec<(String, String)> {
    let Some(ThemeConfig::SpawnTuiConfig {
        variable,
        light,
        dark,
    }) = crate::agents::theme_config(command)
    else {
        return Vec::new();
    };
    let contents = match scheme {
        Scheme::Light => light,
        Scheme::Dark => dark,
    };
    let Some(contents) = contents else {
        return Vec::new();
    };
    let Some(themes_dir) =
        crate::util::home_dir().map(|home| home.join(".config").join("opencode").join("themes"))
    else {
        eprintln!("theming: no home dir; spawning {command} without a theme override");
        return Vec::new();
    };
    let config_dir = match crate::workspace::ensure_config_dir() {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!(
                "theming: no config dir ({error}); spawning {command} without a theme override"
            );
            return Vec::new();
        }
    };
    match materialize_tui_config(contents, scheme, &themes_dir, &config_dir) {
        Ok(path) => vec![(variable.to_string(), path.to_string_lossy().into_owned())],
        Err(error) => {
            eprintln!("theming: {error}; spawning {command} without a theme override");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        materialize_tui_config, spawn_args, spawn_env, spawn_tui_config_env, write_if_stale,
        Scheme, ThemeConfig, PADE_DARK_THEME_FILE, PADE_DARK_THEME_JSON, PADE_LIGHT_THEME_FILE,
        PADE_LIGHT_THEME_JSON,
    };

    fn claude_live_theme() -> (&'static str, &'static str, &'static str, &'static str) {
        let Some(ThemeConfig::SpawnSelectedLiveTheme {
            args,
            relative_path,
            light,
            dark,
        }) = crate::agents::theme_config("claude")
        else {
            panic!("claude should carry a live theme definition");
        };
        let selector = args.last().copied().expect("the --settings payload");
        (selector, relative_path, light, dark)
    }

    /// The `custom:` selector, the definition's file stem and the `name` inside
    /// it must agree, or the agent resolves the launch arg to a theme that does
    /// not exist and silently falls back. Three literals, one invariant — worth
    /// a test rather than a comment.
    #[test]
    fn the_live_theme_selector_names_the_file_it_writes() {
        let (selector, relative_path, light, dark) = claude_live_theme();
        assert_eq!(selector, r#"{"theme":"custom:pade"}"#);
        assert!(relative_path.ends_with("/pade.json"), "{relative_path}");
        for contents in [light, dark] {
            assert!(contents.contains(r#""name":"pade""#), "{contents}");
        }
    }

    /// The scheme lives in the definition's contents — the launch args that
    /// select it are the same on both schemes, which is what lets a flip reach a
    /// session that is already running.
    #[test]
    fn a_live_theme_carries_its_scheme_in_the_file_not_the_argv() {
        let (_, _, light, dark) = claude_live_theme();
        assert!(light.contains(r#""base":"light-ansi""#), "{light}");
        assert!(dark.contains(r#""base":"dark-ansi""#), "{dark}");
        assert_eq!(
            spawn_args("claude", Scheme::Light),
            spawn_args("claude", Scheme::Dark)
        );
        assert!(!spawn_args("claude", Scheme::Light).is_empty());
    }

    /// The agent watches the definition, so republishing an unchanged scheme
    /// must not touch the file — a rewrite it can see is a repaint it does not
    /// need.
    #[test]
    fn republishing_an_unchanged_theme_leaves_the_file_alone() {
        let scratch = std::env::temp_dir().join(format!("pade-live-theme-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        let path = scratch.join("themes").join("pade.json");

        write_if_stale(&path, r#"{"base":"dark"}"#).expect("first write");
        let written = std::fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .expect("first mtime");

        write_if_stale(&path, r#"{"base":"dark"}"#).expect("identical rewrite");
        let unchanged = std::fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .expect("second mtime");
        assert_eq!(written, unchanged);

        write_if_stale(&path, r#"{"base":"light"}"#).expect("scheme flip");
        assert_eq!(
            std::fs::read_to_string(&path).expect("read flipped theme"),
            r#"{"base":"light"}"#
        );
        std::fs::remove_dir_all(scratch).expect("scratch cleanup");
    }

    /// The arg-themed accessor routes each scheme to its own side of the
    /// registry entry (read from the SSOT, so the theme literals stay defined in
    /// exactly one place — the codex `AgentDef`); an env-themed agent and an
    /// unknown command carry no launch args.
    #[test]
    fn spawn_args_route_each_scheme_to_its_registry_side() {
        let ThemeConfig::SpawnArgs { light, dark } =
            crate::agents::theme_config("codex").expect("codex is arg-themed")
        else {
            panic!("codex should force its theme via SpawnArgs");
        };
        assert_eq!(spawn_args("codex", Scheme::Light), *light);
        assert_eq!(spawn_args("codex", Scheme::Dark), *dark);
        assert_ne!(light, dark);
        assert!(spawn_args("aider", Scheme::Light).is_empty());
        assert!(spawn_args("pnpm", Scheme::Dark).is_empty());
    }

    /// The env-themed accessor likewise reads from the registry SSOT; an
    /// arg-themed agent and an unknown command carry no theme env.
    #[test]
    fn spawn_env_routes_each_scheme_to_its_registry_side() {
        let ThemeConfig::SpawnEnv { light, dark } =
            crate::agents::theme_config("aider").expect("aider is env-themed")
        else {
            panic!("aider should force its theme via SpawnEnv");
        };
        assert_eq!(spawn_env("aider", Scheme::Light), *light);
        assert_eq!(spawn_env("aider", Scheme::Dark), *dark);
        assert_ne!(light, dark);
        assert!(spawn_env("codex", Scheme::Light).is_empty());
        assert!(spawn_env("pnpm", Scheme::Dark).is_empty());
    }

    /// A live-themed agent carries no theme *env*: its scheme travels in the
    /// definition file, so nothing scheme-shaped may leak into the environment
    /// and go stale the moment the user flips.
    #[test]
    fn a_live_themed_agent_has_no_theme_env() {
        assert!(spawn_env("claude", Scheme::Light).is_empty());
        assert!(spawn_env("claude", Scheme::Dark).is_empty());
    }

    /// A file-themed agent (opencode) carries no theme env or args through the
    /// static accessors — its whole signal is the tui-config env pair.
    #[test]
    fn a_file_themed_agent_has_no_static_theme_env_or_args() {
        assert!(spawn_env("opencode", Scheme::Light).is_empty());
        assert!(spawn_args("opencode", Scheme::Light).is_empty());
    }

    /// Materializing a tui config writes both files idempotently — the additive
    /// `pade-light` theme next to the user's opencode themes, and the
    /// per-scheme config under PADE's own dir — and returns the config path the
    /// env variable will carry.
    #[test]
    fn materialize_writes_theme_and_config_and_skips_identical_rewrites() {
        let scratch = std::env::temp_dir().join(format!("pade-theming-{}", std::process::id()));
        let themes_dir = scratch.join("themes");
        let config_dir = scratch.join("config");
        let contents = r#"{"theme":"pade-light"}"#;

        let path = materialize_tui_config(contents, Scheme::Light, &themes_dir, &config_dir)
            .expect("first materialize succeeds");
        assert_eq!(path, config_dir.join("opencode-tui-light.json"));
        assert_eq!(
            std::fs::read_to_string(&path).expect("config written"),
            contents
        );
        assert_eq!(
            std::fs::read_to_string(themes_dir.join(PADE_LIGHT_THEME_FILE))
                .expect("light theme written"),
            PADE_LIGHT_THEME_JSON
        );
        assert_eq!(
            std::fs::read_to_string(themes_dir.join(PADE_DARK_THEME_FILE))
                .expect("dark theme written"),
            PADE_DARK_THEME_JSON
        );

        // A second run with identical contents is a no-op rewrite-wise: the
        // files keep their bytes and the same path comes back.
        let again = materialize_tui_config(contents, Scheme::Light, &themes_dir, &config_dir)
            .expect("second materialize succeeds");
        assert_eq!(again, path);

        std::fs::remove_dir_all(&scratch).expect("scratch cleanup");
    }

    /// opencode's adaptive theme is selected on BOTH schemes, so each yields an
    /// env pair; a non-file-themed command and an unknown command yield nothing.
    #[test]
    fn tui_config_env_fires_for_the_file_themed_agent_on_both_schemes() {
        assert!(!spawn_tui_config_env("opencode", Scheme::Light).is_empty());
        assert!(!spawn_tui_config_env("opencode", Scheme::Dark).is_empty());
        assert!(spawn_tui_config_env("claude", Scheme::Light).is_empty());
        assert!(spawn_tui_config_env("pnpm", Scheme::Light).is_empty());
    }

    /// Each scheme has its own truthful theme: the dark palette is a distinct
    /// definition, not a fallback to the light one (the old `dark: None` gap).
    #[test]
    fn light_and_dark_themes_are_distinct() {
        assert_ne!(PADE_LIGHT_THEME_JSON, PADE_DARK_THEME_JSON);
        assert!(PADE_DARK_THEME_JSON.contains(r##""background":"#282c34""##));
        assert!(PADE_LIGHT_THEME_JSON.contains(r##""background":"#fafafa""##));
    }
}
