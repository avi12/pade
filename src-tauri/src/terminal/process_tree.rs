//! Process-tree work on an agent session's OS process: ending a whole tree (a
//! session's teardown, in the PTY host) and finding the subtree an agent spawned
//! for a known task (the Tasks dock's stop and liveness, in the app). Keyed on
//! the OS process tree and the task command only - nothing agent-specific.

use std::collections::{HashMap, HashSet};

/// Terminate a process and every descendant it spawned. `Child::kill` reaches
/// only the immediate child, so an agent's grandchildren live on and hold the
/// workspace cwd; `taskkill /T` walks the tree from `process_id` and ends them all, `/F`
/// forces it. Best-effort - a process already gone just makes this a no-op.
/// Shelled out through `util::command` so it inherits `CREATE_NO_WINDOW`.
#[cfg(windows)]
pub(crate) fn kill_process_tree(process_id: u32) {
    let _ = crate::util::command("taskkill")
        .args(["/F", "/T", "/PID", &process_id.to_string()])
        .output();
}

/// One running process from a system snapshot: its id, its parent's id, and the
/// command line it was launched with - the minimum needed to walk the tree and
/// recognise the task an agent started.
#[derive(Debug, Clone)]
struct ProcessInfo {
    process_id: u32,
    parent_process_id: u32,
    command_line: String,
}

/// Collapse every run of whitespace to one space, so a command matches regardless
/// of the shell's own reformatting of the line it launched.
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A shell separator or quote - anything that is not part of a command token.
fn is_command_boundary(character: char) -> bool {
    matches!(
        character,
        ' ' | '"' | '\'' | '&' | '|' | ';' | '(' | ')' | '<' | '>'
    )
}

/// Whether `command_line` runs `command` as a whole token run - bounded on both
/// sides by a shell separator. A bounded substring match, so it fires when the
/// shell wraps the command (`bash -c "pnpm dev"`, a `cd … &&` prefix, a redirect)
/// yet never mistakes `pnpm dev` for a run of `pnpm dev:prod`.
fn command_matches(command_line: &str, command: &str) -> bool {
    let haystack = collapse_whitespace(command_line);
    let needle = collapse_whitespace(command);
    if needle.is_empty() {
        return false;
    }

    haystack
        .match_indices(needle.as_str())
        .any(|(start, matched)| {
            let end = start + matched.len();
            let is_bounded_before = haystack[..start]
                .chars()
                .next_back()
                .is_none_or(is_command_boundary);
            let is_bounded_after = haystack[end..]
                .chars()
                .next()
                .is_none_or(is_command_boundary);
            is_bounded_before && is_bounded_after
        })
}

/// The processes under `root` whose command line runs `command` - the subtree an
/// agent spawned for a known task. Pure over a process snapshot so the tree-walk
/// and match are unit-testable without enumerating the live system. `root` (the
/// agent itself) is never included, so a Stop can never kill the agent.
fn task_processes(processes: &[ProcessInfo], root: u32, command: &str) -> Vec<u32> {
    // Index children by parent once, so the walk is linear in the snapshot
    // rather than a full rescan per visited process.
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for process in processes {
        let is_own_parent = process.process_id == process.parent_process_id;
        if !is_own_parent {
            children
                .entry(process.parent_process_id)
                .or_default()
                .push(process.process_id);
        }
    }

    let mut descendants: HashSet<u32> = HashSet::new();
    let mut frontier = vec![root];
    while let Some(parent) = frontier.pop() {
        for &child in children.get(&parent).into_iter().flatten() {
            if descendants.insert(child) {
                frontier.push(child);
            }
        }
    }

    processes
        .iter()
        .filter(|process| descendants.contains(&process.process_id))
        .filter(|process| command_matches(&process.command_line, command))
        .map(|process| process.process_id)
        .collect()
}

/// One row of the CIM process snapshot. Fields are optional because a process can
/// vanish mid-query and CIM then reports nulls.
#[cfg(windows)]
#[derive(serde::Deserialize)]
struct CimProcess {
    #[serde(rename = "ProcessId")]
    process_id: Option<u32>,
    #[serde(rename = "ParentProcessId")]
    parent_process_id: Option<u32>,
    #[serde(rename = "CommandLine")]
    command_line: Option<String>,
}

/// Snapshot every running process (id, parent, command line) via a single CIM
/// query. Shelled through `util::command` (no console window; not `wmic`, which is
/// deprecated on current Windows) rather than PEB-walking each process by hand.
#[cfg(windows)]
fn snapshot_processes() -> Result<Vec<ProcessInfo>, String> {
    let output = crate::util::command("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,CommandLine | ConvertTo-Json -Compress",
        ])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let rows: Vec<CimProcess> =
        serde_json::from_str(text.trim()).map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            row.process_id.map(|process_id| ProcessInfo {
                process_id,
                parent_process_id: row.parent_process_id.unwrap_or_default(),
                command_line: row.command_line.unwrap_or_default(),
            })
        })
        .collect())
}

/// Find and kill the subtree(s) an agent spawned for `command`, anywhere under its
/// process `root`. Returns whether anything matched. Windows only - the snapshot
/// and the tree-kill both go through Windows tooling.
#[cfg(windows)]
pub(crate) fn stop_task_tree(root: u32, command: &str) -> Result<bool, String> {
    let processes = snapshot_processes()?;
    let targets = task_processes(&processes, root, command);
    for process_id in &targets {
        kill_process_tree(*process_id);
    }

    Ok(!targets.is_empty())
}

#[cfg(not(windows))]
pub(crate) fn stop_task_tree(_root: u32, _command: &str) -> Result<bool, String> {
    Ok(false)
}

/// Whether any process under `root` still runs `command`. Snapshot-and-match with
/// no kill - the read half of [`stop_task_tree`], used to poll an attached task's
/// liveness. Windows only.
#[cfg(windows)]
pub(crate) fn task_running(root: u32, command: &str) -> Result<bool, String> {
    let processes = snapshot_processes()?;
    Ok(!task_processes(&processes, root, command).is_empty())
}

#[cfg(not(windows))]
pub(crate) fn task_running(_root: u32, _command: &str) -> Result<bool, String> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::assert_empty;

    fn process(process_id: u32, parent_process_id: u32, command_line: &str) -> ProcessInfo {
        ProcessInfo {
            process_id,
            parent_process_id,
            command_line: command_line.to_string(),
        }
    }

    #[test]
    fn command_matches_survives_shell_wrapping() {
        assert!(command_matches(
            "bash -c \"cd app && pnpm dev\"",
            "pnpm dev"
        ));
        assert!(command_matches("pnpm   dev 2>&1", "pnpm dev"));
        assert!(command_matches(
            "cargo check --all-targets | tail",
            "cargo check --all-targets"
        ));
    }

    #[test]
    fn command_matches_rejects_a_longer_sibling() {
        assert!(!command_matches("pnpm dev:prod", "pnpm dev"));
        assert!(!command_matches("pnpm develop", "pnpm dev"));
        assert!(!command_matches("", "pnpm dev"));
    }

    #[test]
    fn task_processes_finds_the_agent_subtree_only() {
        let processes = vec![
            process(100, 1, "claude"),                 // the agent (root)
            process(200, 100, "bash -c \"pnpm dev\""), // the task shell it spawned
            process(300, 200, "node vite"),            // the dev server (no literal match)
            process(400, 1, "pnpm dev"),               // an UNRELATED sibling outside the agent
        ];
        assert_eq!(task_processes(&processes, 100, "pnpm dev"), vec![200]);
    }

    #[test]
    fn task_processes_walks_every_generation_and_survives_a_self_parented_process() {
        let processes = vec![
            process(0, 0, "pnpm dev"),            // the idle process is its own parent
            process(100, 0, "claude"),            // the agent (root)
            process(200, 100, "bash"),            // a child shell
            process(300, 200, "cmd /c pnpm dev"), // a grandchild running the task
            process(400, 300, "pnpm dev"),        // and its own child
        ];
        assert_eq!(task_processes(&processes, 100, "pnpm dev"), vec![300, 400]);
    }

    #[test]
    fn task_processes_never_returns_the_agent_root() {
        let processes = vec![process(100, 1, "claude pnpm dev")];
        assert_empty(&task_processes(&processes, 100, "pnpm dev"));
    }
}
