//! Force each installed agent's own UI theme to match ADE's light/dark scheme.
//!
//! Why spawn-time signals and not the terminal protocol: an agent's `auto` theme
//! follows the *terminal* — Claude Code queries the background color (OSC 11) at
//! startup and listens for color-scheme reports (DECSET 2031 → `CSI ?997;n`) —
//! but Windows `ConPTY` consumes the startup query on the way through, so the
//! agent cannot learn ADE's initial palette from xterm. The frontend applies a
//! fallback at spawn, then relays the DECSET 2031 `?997` report directly through
//! the PTY whenever the app palette changes; that live input path reaches the
//! already-running Claude process without replacing its conversation.
//!
//! What does work is the tier *above* the probe: Claude's `auto` detection reads
//! `$COLORFGBG` before it ever sends OSC 11, and the other CLIs expose their own
//! spawn-time env or launch-arg knobs — and for a CLI with neither (opencode),
//! a whole TUI-config file selected per spawn via an env var, naming a custom
//! theme whose colors are mode-independent so the poisoned probe stops
//! mattering. So every agent is themed at spawn — per
//! session, never via a user-global config file that would leak ADE's choice
//! into the user's other terminals. (A project settings.local.json `theme` key
//! IS honored by Claude Code ≥2.1, but it pins a named theme in a user-owned
//! file rather than following the scheme — stale keys the old file-driven
//! mechanism left behind forced wrong themes long after ADE moved on, which is
//! exactly why writing user files is the wrong channel.) A spawn-time theme
//! cannot follow a mid-session scheme flip. ADE re-themes xterm's palette in
//! place to preserve the running conversation; the agent receives its own
//! spawn-time syntax choice on the next natural launch.

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
    /// answers itself (always black), so no signal can flip its *mode*. The
    /// per-scheme values are the config file's *contents*; `None` means the
    /// scheme needs no override. `spawn_tui_config_env` materializes the file
    /// under PADE's own config dir and hands `pty.rs` the `(variable, path)`
    /// env pair.
    SpawnTuiConfig {
        variable: &'static str,
        light: Option<&'static str>,
        dark: Option<&'static str>,
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
        Some(ThemeConfig::SpawnArgs { .. } | ThemeConfig::SpawnTuiConfig { .. }) | None => &[],
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
        Some(ThemeConfig::SpawnEnv { .. } | ThemeConfig::SpawnTuiConfig { .. }) | None => &[],
    }
}

/// The custom opencode theme a `SpawnTuiConfig` selects on a light ADE: every
/// color is a plain string (no light/dark variants), so opencode's
/// ConPTY-poisoned mode detection is irrelevant to how it renders. One Light
/// palette, pade-namespaced.
const PADE_LIGHT_THEME_FILE: &str = "pade-light.json";
const PADE_LIGHT_THEME_JSON: &str = r##"{"$schema":"https://opencode.ai/theme.json","theme":{"primary":"#4078f2","secondary":"#a626a4","accent":"#0184bc","error":"#e45649","warning":"#c18401","success":"#50a14f","info":"#0184bc","text":"#383a42","textMuted":"#696c77","background":"#fafafa","backgroundPanel":"#f0f0f1","backgroundElement":"#e5e5e6","border":"#d4d4d5","borderActive":"#a0a1a7","borderSubtle":"#e5e5e6","diffAdded":"#50a14f","diffRemoved":"#e45649","diffContext":"#696c77","diffHunkHeader":"#696c77","diffHighlightAdded":"#2d6b2c","diffHighlightRemoved":"#a8342a","diffAddedBg":"#e8f5e9","diffRemovedBg":"#fdecea","diffContextBg":"#fafafa","diffLineNumber":"#d4d4d5","diffAddedLineNumberBg":"#d7ecd8","diffRemovedLineNumberBg":"#f8d7d3","markdownText":"#383a42","markdownHeading":"#a626a4","markdownLink":"#4078f2","markdownLinkText":"#0184bc","markdownCode":"#50a14f","markdownBlockQuote":"#696c77","markdownEmph":"#c18401","markdownStrong":"#383a42","markdownHorizontalRule":"#d4d4d5","markdownListItem":"#4078f2","markdownListEnumeration":"#0184bc","markdownImage":"#4078f2","markdownImageText":"#0184bc","markdownCodeBlock":"#383a42","syntaxComment":"#a0a1a7","syntaxKeyword":"#a626a4","syntaxFunction":"#4078f2","syntaxVariable":"#e45649","syntaxString":"#50a14f","syntaxNumber":"#986801","syntaxType":"#c18401","syntaxOperator":"#0184bc","syntaxPunctuation":"#383a42"}}"##;

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
/// `theme` keys the old file mechanism left behind). `pade-light.json` is an
/// ADDITIVE, pade-namespaced theme *definition* — inert until a spawn's
/// `OPENCODE_TUI_CONFIG` selects it. The user's own tui.json, and therefore
/// every opencode session the user launches outside ADE, is untouched.
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
        materialize_tui_config, spawn_args, spawn_env, spawn_tui_config_env, Scheme, ThemeConfig,
        PADE_LIGHT_THEME_FILE, PADE_LIGHT_THEME_JSON,
    };

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
        assert!(spawn_args("claude", Scheme::Light).is_empty());
        assert!(spawn_args("pnpm", Scheme::Dark).is_empty());
    }

    /// The env-themed accessor likewise reads from the registry SSOT; an
    /// arg-themed agent and an unknown command carry no theme env.
    #[test]
    fn spawn_env_routes_each_scheme_to_its_registry_side() {
        let ThemeConfig::SpawnEnv { light, dark } =
            crate::agents::theme_config("claude").expect("claude is env-themed")
        else {
            panic!("claude should force its theme via SpawnEnv");
        };
        assert_eq!(spawn_env("claude", Scheme::Light), *light);
        assert_eq!(spawn_env("claude", Scheme::Dark), *dark);
        assert_ne!(light, dark);
        assert!(spawn_env("codex", Scheme::Light).is_empty());
        assert!(spawn_env("pnpm", Scheme::Dark).is_empty());
    }

    /// Claude's theme rides `$COLORFGBG` — the first tier of its `auto`
    /// detection, and the only one that survives `ConPTY`. Both sides must set it,
    /// and the background field (after the `;`) must name the scheme's ground.
    #[test]
    fn claude_signals_its_scheme_through_colorfgbg() {
        assert_eq!(spawn_env("claude", Scheme::Light), &[("COLORFGBG", "0;15")]);
        assert_eq!(spawn_env("claude", Scheme::Dark), &[("COLORFGBG", "15;0")]);
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
            std::fs::read_to_string(themes_dir.join(PADE_LIGHT_THEME_FILE)).expect("theme written"),
            PADE_LIGHT_THEME_JSON
        );

        // A second run with identical contents is a no-op rewrite-wise: the
        // files keep their bytes and the same path comes back.
        let again = materialize_tui_config(contents, Scheme::Light, &themes_dir, &config_dir)
            .expect("second materialize succeeds");
        assert_eq!(again, path);

        std::fs::remove_dir_all(&scratch).expect("scratch cleanup");
    }

    /// Only the scheme with override contents yields an env pair: opencode's
    /// dark side is `None` (`ConPTY`'s poisoned detection already lands dark,
    /// correct on a dark ADE), and non-file-themed commands yield nothing.
    #[test]
    fn tui_config_env_only_fires_for_a_scheme_with_contents() {
        assert!(spawn_tui_config_env("opencode", Scheme::Dark).is_empty());
        assert!(spawn_tui_config_env("claude", Scheme::Light).is_empty());
        assert!(spawn_tui_config_env("pnpm", Scheme::Light).is_empty());
    }
}
