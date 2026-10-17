//! The open pull request for a project's current branch, so an agent's
//! summary can link it whenever it mentions it.
//!
//! Asked of the GitHub CLI (`gh`), which already knows the user's account and
//! follows a fork to the upstream repository its pull requests live in - a
//! branch pushed to a fork and opened against upstream is found too. No `gh`,
//! no GitHub remote, no open pull request: `None`, and the summary is written
//! without one. A lookup is a network call, so each folder's answer is kept
//! for a couple of minutes - every agent summarized meanwhile shares it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::vcs::git_in;

/// The CLI the lookup asks.
const GITHUB_CLI: &str = "gh";
/// Keeps `gh` from ever stopping to ask (an expired login, a host prompt) -
/// the lookup runs unattended, so it fails instead.
const PROMPT_DISABLED_VARIABLE: &str = "GH_PROMPT_DISABLED";

/// How long one folder's lookup is reused before `gh` is asked again.
const LOOKUP_REUSE: Duration = Duration::from_secs(120);

/// One open pull request: its number, title and web address.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub(crate) struct PullRequest {
    pub(crate) number: u64,
    pub(crate) title: String,
    pub(crate) url: String,
}

type Lookup = (Instant, Option<PullRequest>);
static LOOKUPS: LazyLock<Mutex<HashMap<PathBuf, Lookup>>> = LazyLock::new(Default::default);

/// The open pull request whose head is `cwd`'s current branch, or `None` -
/// from the last couple of minutes' lookup when there is one. Blocking: two
/// subprocesses, one of them a network call.
pub(crate) fn open_pull_request(cwd: &Path) -> Option<PullRequest> {
    let reused = LOOKUPS.lock().ok().and_then(|lookups| {
        let (looked_up_at, pull_request) = lookups.get(cwd)?;
        (looked_up_at.elapsed() < LOOKUP_REUSE).then(|| pull_request.clone())
    });
    if let Some(pull_request) = reused {
        return pull_request;
    }

    let pull_request = look_up_pull_request(cwd);
    if let Ok(mut lookups) = LOOKUPS.lock() {
        lookups.insert(cwd.to_path_buf(), (Instant::now(), pull_request.clone()));
    }
    pull_request
}

fn look_up_pull_request(cwd: &Path) -> Option<PullRequest> {
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
