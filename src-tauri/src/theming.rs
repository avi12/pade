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
//! spawn-time env or launch-arg knobs. So every agent is themed at spawn — per
//! session, never via a user-global config file that would leak ADE's choice
//! into the user's other terminals. (A project-settings `theme` key is not an
//! option: Claude Code's settings.json schema has no such key — theme lives in
//! its global config — so writing one is silently ignored.) A spawn-time theme
//! cannot follow a mid-session scheme flip. ADE re-themes xterm's palette in
//! place to preserve the running conversation; the agent receives its own
//! spawn-time syntax choice on the next natural launch.

use serde::Deserialize;

/// ADE's resolved appearance — the frontend's `appearance.scheme`, on the wire.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Light,
    Dark,
}

/// How one agent's theme is forced — registry knowledge, declared per agent in
/// `agents.rs` and interpreted here (`SoC`: the registry knows *what*, this
/// module knows *how to apply it*).
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
        Some(ThemeConfig::SpawnArgs { .. }) | None => &[],
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
        Some(ThemeConfig::SpawnEnv { .. }) | None => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::{spawn_args, spawn_env, Scheme, ThemeConfig};

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

}
