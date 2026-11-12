use super::{
    installed_names, needs_light_console_fix, oneshot_invocation, resolve_codex_home,
    session_arguments, session_id_flag, spawn_environment, theme_config, ConversationRecord,
    REGISTRY,
};
use std::path::PathBuf;

#[test]
fn a_configured_codex_home_wins_over_the_default() {
    assert_eq!(
        resolve_codex_home(Some("D:/codex".into()), Some(PathBuf::from("C:/home"))),
        Some(PathBuf::from("D:/codex"))
    );
}

#[test]
fn codex_home_defaults_to_dot_codex_in_the_user_home() {
    assert_eq!(
        resolve_codex_home(None, Some(PathBuf::from("C:/home"))),
        Some(PathBuf::from("C:/home").join(".codex"))
    );
    assert_eq!(resolve_codex_home(None, None), None);
}
use crate::terminal::theming::ThemeConfig;
use crate::test_support::assert_empty;

#[test]
fn installed_names_lead_with_the_canonical_command() {
    let names = installed_names("codex");
    assert_eq!(names.first(), Some(&"codex"));
    assert!(names.contains(&"codex-x86_64-pc-windows-msvc"));
}

/// A command ADE has no entry for - a task runner, an editor - resolves under
/// its own name and carries no agent baggage.
#[test]
fn an_unknown_command_is_only_ever_itself() {
    assert_eq!(installed_names("pnpm"), vec!["pnpm"]);
    assert_empty(spawn_environment("pnpm"));
    assert!(oneshot_invocation("pnpm").is_none());
    assert_empty(session_arguments("pnpm"));
    assert!(theme_config("pnpm").is_none());
    assert!(session_id_flag("pnpm").is_none());
}

/// Claude can pin/resume a conversation by id; agents without the flag can't
/// be targeted, so ADE leaves them out of the restart-to-resume flow.
#[test]
fn only_agents_with_a_session_id_flag_can_be_resumed_by_id() {
    assert_eq!(
        session_id_flag("claude").map(|flag| flag.arguments("id", ConversationRecord::Unrecorded)),
        Some(vec!["--session-id".to_string(), "id".to_string()])
    );
    assert_eq!(
        session_id_flag("copilot").map(|flag| flag.arguments("id", ConversationRecord::Unrecorded)),
        Some(vec!["--resume=id".to_string()])
    );
    assert_eq!(
        session_id_flag("claude").map(|flag| flag.arguments("id", ConversationRecord::Recorded)),
        Some(vec!["--resume".to_string(), "id".to_string()])
    );
    assert!(session_id_flag("codex").is_none());
    assert!(session_id_flag("powershell.exe").is_none());
}

/// Claude declares the live definition (the only agent that can be re-themed
/// without relaunching); the env-themed CLIs declare spawn pairs; Codex
/// declares launch args.
#[test]
fn theme_mechanisms_match_each_agent() {
    assert!(matches!(
        theme_config("claude"),
        Some(ThemeConfig::SpawnSelectedLiveTheme { .. })
    ));
    assert!(matches!(
        theme_config("aider"),
        Some(ThemeConfig::SpawnEnvironment { .. })
    ));
    // Codex themes its whole TUI from a named `[tui] theme`; ADE forces
    // Codex's own light/dark defaults per launch with `-c tui.theme=…`.
    assert!(matches!(
        theme_config("codex"),
        Some(ThemeConfig::SpawnArguments {
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
fn session_arguments_carry_the_skip_permissions_flag() {
    assert_eq!(
        session_arguments("claude"),
        &["--dangerously-skip-permissions"]
    );
    // Codex also gets forced onto the alternate screen so it renders fullscreen
    // like Claude (see the codex AgentDef); the `-c` pair rides `session_arguments`.
    assert_eq!(
        session_arguments("codex"),
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
        session_arguments("copilot"),
        &["--allow-all-tools", "--alt-screen", "on"]
    );
    assert_empty(session_arguments("powershell.exe"));
    // Keyed by the canonical command, not the file an installer laid down.
    assert_empty(session_arguments("codex-x86_64-pc-windows-msvc"));
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
    // Claude's renderer is the user's choice, not ADE's: forcing the
    // alt-screen one moves the scrollback inside the agent, and every wheel
    // tick then round-trips through it instead of scrolling locally.
    assert_empty(spawn_environment("claude"));
}

/// opencode's whole registry surface: `run` is its headless one-shot,
/// `--auto` its skip-permissions mode, and its `system` theme follows the
/// terminal - but `ConPTY` poisons the OSC 10/11 probe its light/dark
/// detection rides, so a light ADE forces the mode-independent `pade-light`
/// theme via a per-spawn `OPENCODE_TUI_CONFIG` file while dark needs no
/// override. No session-id handle.
#[test]
fn opencode_runs_headless_via_run_and_autonomous_via_auto() {
    assert_eq!(oneshot_invocation("opencode"), Some(&["run"][..]));
    assert_eq!(session_arguments("opencode"), &["--auto"]);
    let Some(ThemeConfig::SpawnTuiConfig {
        variable,
        light,
        dark,
    }) = theme_config("opencode")
    else {
        panic!("opencode should force its theme via SpawnTuiConfig");
    };
    assert_eq!(*variable, "OPENCODE_TUI_CONFIG");
    // Each scheme selects its own plain-string pade theme (both defined, so a
    // dark ADE is truthful too - a flip re-themes by respawn).
    assert_eq!(
        *light,
        Some(r#"{"$schema":"https://opencode.ai/tui.json","theme":"pade-light"}"#)
    );
    assert_eq!(
        *dark,
        Some(r#"{"$schema":"https://opencode.ai/tui.json","theme":"pade-dark"}"#)
    );
    // `--session <id>` only continues an existing session, so opencode has
    // no restart-to-resume handle.
    assert!(session_id_flag("opencode").is_none());
    assert_empty(spawn_environment("opencode"));
    // ConPTY resize refills paint from the console buffer, so a light ADE
    // keeps the console-buffer flip (like Codex).
    assert!(needs_light_console_fix("opencode"));
    // Installed under its own name only - no installer aliases.
    assert_eq!(installed_names("opencode"), vec!["opencode"]);
}

/// A Copilot failure falls back along every headless agent, the session's
/// own first - and an agent with no headless mode is skipped, not tried.
#[test]
fn headless_agents_lead_with_the_preferred_one() {
    let chain: Vec<&str> = super::headless_agents("codex").collect();
    assert_eq!(chain.first(), Some(&"codex"));
    assert!(chain.contains(&"claude"));
    assert_eq!(
        chain.iter().filter(|command| **command == "codex").count(),
        1
    );
    assert!(!chain.contains(&"copilot"));
    let without_headless: Vec<&str> = super::headless_agents("copilot").collect();
    assert_eq!(without_headless.first(), Some(&"claude"));
}

/// Muse Code and Qwen Code: a headless one-shot, a yolo switch, and nothing
/// ADE can force or resume by id.
#[test]
fn muse_and_qwen_run_headless_and_autonomous() {
    assert_eq!(oneshot_invocation("muse"), Some(&["exec"][..]));
    assert_eq!(oneshot_invocation("qwen"), Some(&[][..]));
    for command in ["muse", "qwen"] {
        assert_eq!(session_arguments(command), &["--yolo"]);
        assert!(theme_config(command).is_none());
        assert!(session_id_flag(command).is_none());
        assert_eq!(installed_names(command), vec![command]);
    }
}

#[test]
fn only_claude_is_known_to_reorder_its_own_right_to_left_text() {
    // Measured on 2.1.227: Claude Code runs the bidi algorithm over each row
    // before writing it, so the terminal must paint that row, not reorder it
    // again. Every other entry writes logical order like a normal program -
    // and an unknown command answers no, which is the safe way round: the
    // overlay reorders, which is right for logical order.
    let reorders: Vec<&str> = REGISTRY
        .iter()
        .filter(|agent| agent.reorders_bidi)
        .map(|agent| agent.command)
        .collect();
    assert_eq!(reorders, vec!["claude"]);
}
