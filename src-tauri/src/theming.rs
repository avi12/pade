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
//! What does work is the tier *above* the probe: PADE creates Claude's registered
//! project-local `theme:auto` seed before launch, then its detection reads
//! `$COLORFGBG` before it ever sends OSC 11. The other CLIs expose their own
//! spawn-time env or launch-arg knobs — and for a CLI with neither (opencode),
//! a whole TUI-config file selected per spawn via an env var, naming a custom
//! adaptive theme that carries both a light and a dark variant per color;
//! opencode subscribes to the same DECSET 2031 channel Claude does, so the
//! live `?997` report both corrects its ConPTY-poisoned initial probe and
//! flips it between the two variants in place. So every agent is themed at
//! spawn — per
//! session, never via a user-global config file that would leak ADE's choice
//! into the user's other terminals. The registry may also declare a project-local
//! adaptive-theme seed for future launches; PADE creates it only when absent and
//! never merges into or overwrites user settings. This avoids the stale fixed
//! light/dark keys the old file-driven mechanism left behind. A spawn-time theme
//! cannot follow a mid-session scheme flip. ADE re-themes xterm's palette in
//! place to preserve the running conversation; the agent receives its own
//! spawn-time syntax choice on the next natural launch.

use serde::Deserialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// ADE's resolved appearance — the frontend's `appearance.scheme`, on the wire.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Light,
    Dark,
}

pub struct ProjectThemeRequest<'a> {
    pub command: &'a str,
    pub root: &'a Path,
}

/// Seed an agent's registered project-local adaptive theme when absent. The
/// registry owns native paths/content; this layer provides one create-only file
/// operation. Existing user-owned settings are never merged or overwritten.
pub fn ensure_project_theme(request: ProjectThemeRequest<'_>) -> std::io::Result<bool> {
    let Some(seed) = crate::agents::project_theme_seed(request.command) else {
        return Ok(false);
    };

    let path = request.root.join(seed.relative_path);
    if path.exists() {
        return Ok(false);
    }

    let Some(parent) = path.parent() else {
        return Ok(false);
    };
    std::fs::create_dir_all(parent)?;
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            file.write_all(seed.contents.as_bytes())?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error),
    }
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
    /// env var. The config selects a PADE-owned adaptive theme (both light and
    /// dark variants per color); opencode's own mode detection then follows the
    /// live DECSET 2031 `?997` report PADE relays, so the file is the same on
    /// both schemes. The per-scheme values are the config file's *contents*;
    /// `None` means the scheme needs no override. `spawn_tui_config_env`
    /// materializes the file under PADE's own config dir and hands `pty.rs` the
    /// `(variable, path)` env pair.
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

/// The custom opencode theme a `SpawnTuiConfig` selects. Every color carries
/// both a `light` and a `dark` variant (opencode's `{light,dark}` per-color
/// form), so opencode paints the right one for whichever mode it has detected —
/// and follows a live flip: PADE relays the DECSET 2031 `?997` report to
/// opencode (it subscribes at startup, same channel as Claude), which updates
/// opencode's detected mode, and this adaptive theme then has a variant to swap
/// to. The startup relay also corrects the ConPTY-poisoned initial probe (which
/// always reads dark) on a light ADE. Light values are Atom One Light; dark are
/// Atom One Dark — pade-namespaced.
const PADE_THEME_FILE: &str = "pade.json";
const PADE_THEME_JSON: &str = r##"{"$schema":"https://opencode.ai/theme.json","theme":{"primary":{"light":"#4078f2","dark":"#61afef"},"secondary":{"light":"#a626a4","dark":"#c678dd"},"accent":{"light":"#0184bc","dark":"#56b6c2"},"error":{"light":"#e45649","dark":"#e06c75"},"warning":{"light":"#c18401","dark":"#e5c07b"},"success":{"light":"#50a14f","dark":"#98c379"},"info":{"light":"#0184bc","dark":"#56b6c2"},"text":{"light":"#383a42","dark":"#abb2bf"},"textMuted":{"light":"#696c77","dark":"#828997"},"background":{"light":"#fafafa","dark":"#282c34"},"backgroundPanel":{"light":"#f0f0f1","dark":"#21252b"},"backgroundElement":{"light":"#e5e5e6","dark":"#2c313a"},"border":{"light":"#d4d4d5","dark":"#3e4451"},"borderActive":{"light":"#a0a1a7","dark":"#5c6370"},"borderSubtle":{"light":"#e5e5e6","dark":"#2c313a"},"diffAdded":{"light":"#50a14f","dark":"#98c379"},"diffRemoved":{"light":"#e45649","dark":"#e06c75"},"diffContext":{"light":"#696c77","dark":"#828997"},"diffHunkHeader":{"light":"#696c77","dark":"#828997"},"diffHighlightAdded":{"light":"#2d6b2c","dark":"#6cbe6c"},"diffHighlightRemoved":{"light":"#a8342a","dark":"#e06c75"},"diffAddedBg":{"light":"#e8f5e9","dark":"#2b3328"},"diffRemovedBg":{"light":"#fdecea","dark":"#3a2a2a"},"diffContextBg":{"light":"#fafafa","dark":"#282c34"},"diffLineNumber":{"light":"#d4d4d5","dark":"#4b5263"},"diffAddedLineNumberBg":{"light":"#d7ecd8","dark":"#33402f"},"diffRemovedLineNumberBg":{"light":"#f8d7d3","dark":"#463131"},"markdownText":{"light":"#383a42","dark":"#abb2bf"},"markdownHeading":{"light":"#a626a4","dark":"#c678dd"},"markdownLink":{"light":"#4078f2","dark":"#61afef"},"markdownLinkText":{"light":"#0184bc","dark":"#56b6c2"},"markdownCode":{"light":"#50a14f","dark":"#98c379"},"markdownBlockQuote":{"light":"#696c77","dark":"#828997"},"markdownEmph":{"light":"#c18401","dark":"#e5c07b"},"markdownStrong":{"light":"#383a42","dark":"#abb2bf"},"markdownHorizontalRule":{"light":"#d4d4d5","dark":"#3e4451"},"markdownListItem":{"light":"#4078f2","dark":"#61afef"},"markdownListEnumeration":{"light":"#0184bc","dark":"#56b6c2"},"markdownImage":{"light":"#4078f2","dark":"#61afef"},"markdownImageText":{"light":"#0184bc","dark":"#56b6c2"},"markdownCodeBlock":{"light":"#383a42","dark":"#abb2bf"},"syntaxComment":{"light":"#a0a1a7","dark":"#5c6370"},"syntaxKeyword":{"light":"#a626a4","dark":"#c678dd"},"syntaxFunction":{"light":"#4078f2","dark":"#61afef"},"syntaxVariable":{"light":"#e45649","dark":"#e06c75"},"syntaxString":{"light":"#50a14f","dark":"#98c379"},"syntaxNumber":{"light":"#986801","dark":"#d19a66"},"syntaxType":{"light":"#c18401","dark":"#e5c07b"},"syntaxOperator":{"light":"#0184bc","dark":"#56b6c2"},"syntaxPunctuation":{"light":"#383a42","dark":"#abb2bf"}}}"##;

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
/// `theme` keys the old file mechanism left behind). `pade.json` is an
/// ADDITIVE, pade-namespaced theme *definition* — inert until a spawn's
/// `OPENCODE_TUI_CONFIG` selects it. The user's own tui.json, and therefore
/// every opencode session the user launches outside ADE, is untouched.
fn materialize_tui_config(
    contents: &str,
    scheme: Scheme,
    themes_dir: &Path,
    config_dir: &Path,
) -> std::io::Result<PathBuf> {
    write_if_stale(&themes_dir.join(PADE_THEME_FILE), PADE_THEME_JSON)?;
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
        ensure_project_theme, materialize_tui_config, spawn_args, spawn_env, spawn_tui_config_env,
        ProjectThemeRequest, Scheme, ThemeConfig, PADE_THEME_FILE, PADE_THEME_JSON,
    };

    #[test]
    fn claude_project_theme_is_seeded_once_without_overwriting() {
        let scratch =
            std::env::temp_dir().join(format!("pade-claude-theme-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);

        assert!(!ensure_project_theme(ProjectThemeRequest {
            command: "opencode",
            root: &scratch,
        })
        .expect("unregistered seed"));
        assert!(!scratch.exists());

        assert!(ensure_project_theme(ProjectThemeRequest {
            command: "claude",
            root: &scratch,
        })
        .expect("seed theme"));
        let settings = scratch.join(".claude/settings.local.json");
        assert_eq!(
            std::fs::read_to_string(&settings).expect("read theme"),
            "{\n  \"theme\": \"auto\"\n}\n"
        );

        std::fs::write(&settings, "{\n  \"theme\": \"dark\"\n}\n").expect("replace fixture");
        assert!(!ensure_project_theme(ProjectThemeRequest {
            command: "claude",
            root: &scratch,
        })
        .expect("preserve theme"));
        assert_eq!(
            std::fs::read_to_string(settings).expect("read preserved theme"),
            "{\n  \"theme\": \"dark\"\n}\n"
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
            std::fs::read_to_string(themes_dir.join(PADE_THEME_FILE)).expect("theme written"),
            PADE_THEME_JSON
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

    /// The pade theme is adaptive: every color declares both a light and a dark
    /// variant, so opencode has a variant to swap to on a live scheme flip.
    #[test]
    fn pade_theme_declares_light_and_dark_variants() {
        assert!(PADE_THEME_JSON.contains(r#""light":"#));
        assert!(PADE_THEME_JSON.contains(r#""dark":"#));
    }
}
