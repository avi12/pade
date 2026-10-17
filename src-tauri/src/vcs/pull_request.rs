//! The open pull request for a project's current branch, so the agents
//! dashboard can link it.
//!
//! Asked of the GitHub CLI (`gh`), which already knows the user's account and
//! follows a fork to the upstream repository its pull requests live in - a
//! branch pushed to a fork and opened against upstream is found too. No `gh`,
//! no GitHub remote, no open pull request: `None`, and the card simply shows
//! no link.

use serde::{Deserialize, Serialize};

use crate::vcs::git_in;

/// The CLI the lookup asks.
const GITHUB_CLI: &str = "gh";
/// Keeps `gh` from ever stopping to ask (an expired login, a host prompt) -
/// the lookup runs unattended, so it fails instead.
const PROMPT_DISABLED_VARIABLE: &str = "GH_PROMPT_DISABLED";

/// One open pull request, as the dashboard links it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    number: u64,
    title: String,
    url: String,
}

/// The open pull request whose head is `cwd`'s current branch, or `None`.
// `async` + `spawn_blocking`: two subprocesses, one of them a network call.
#[tauri::command]
pub async fn vcs_pull_request(cwd: String) -> Option<PullRequest> {
    tauri::async_runtime::spawn_blocking(move || open_pull_request(std::path::Path::new(&cwd)))
        .await
        .ok()
        .flatten()
}

fn open_pull_request(cwd: &std::path::Path) -> Option<PullRequest> {
    if !crate::util::is_on_path(GITHUB_CLI) {
        return None;
    }
    let branch_output = git_in(cwd)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    let branch = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_string();
    let is_on_branch = branch_output.status.success() && !branch.is_empty() && branch != "HEAD";
    if !is_on_branch {
        return None;
    }

    let output = crate::util::command(GITHUB_CLI)
        .args(["pr", "list", "--head", &branch, "--state", "open"])
        .args(["--json", "number,title,url", "--limit", "1"])
        .current_dir(cwd)
        .env(PROMPT_DISABLED_VARIABLE, "1")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    first_pull_request(&output.stdout)
}

/// The first pull request in `gh pr list --json`'s answer.
fn first_pull_request(listing: &[u8]) -> Option<PullRequest> {
    serde_json::from_slice::<Vec<PullRequest>>(listing)
        .ok()?
        .into_iter()
        .next()
}

#[cfg(test)]
mod tests {
    use super::{first_pull_request, PullRequest};

    #[test]
    fn reads_the_first_open_pull_request() {
        let listing = br#"[{"number":1278,"title":"Weather light","url":"https://github.com/a/b/pull/1278"}]"#;
        assert_eq!(
            first_pull_request(listing),
            Some(PullRequest {
                number: 1278,
                title: "Weather light".to_string(),
                url: "https://github.com/a/b/pull/1278".to_string(),
            })
        );
    }

    #[test]
    fn no_open_pull_request_is_none() {
        assert_eq!(first_pull_request(b"[]"), None);
        assert_eq!(first_pull_request(b"not json"), None);
    }
}
