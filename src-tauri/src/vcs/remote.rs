//! Remote-URL resolution — the browsable base behind "open on GitHub".

use super::branches::current_branch;
use super::run_git;

const DEFAULT_REMOTE_NAME: &str = "origin";

fn branch_remote_name(cwd: &str) -> String {
    let configured = current_branch(cwd)
        .map(|branch| format!("branch.{branch}.remote"))
        .and_then(|key| run_git(cwd, &["config", "--get", &key]).ok())
        .map(|remote| remote.trim().to_string())
        .filter(|remote| !remote.is_empty() && remote != ".");
    configured.unwrap_or_else(|| DEFAULT_REMOTE_NAME.to_string())
}

fn remote_default_branch(cwd: &str) -> Option<String> {
    let remote = branch_remote_name(cwd);
    let remote_head = format!("refs/remotes/{remote}/HEAD");
    let symbolic = run_git(cwd, &["symbolic-ref", "--short", &remote_head]).ok()?;
    symbolic
        .trim()
        .strip_prefix(&format!("{remote}/"))
        .map(str::to_string)
}

/// The checked-out branch's configured remote URL, falling back to `origin`,
/// normalized to a browsable `https://host/owner/repo` form.
#[tauri::command]
pub async fn vcs_remote_url(cwd: String) -> Option<String> {
    let remote = branch_remote_name(&cwd);
    let raw = run_git(&cwd, &["remote", "get-url", &remote]).ok()?;
    let url = raw.trim();
    if url.is_empty() {
        return None;
    }
    Some(normalize_remote(url))
}

/// The branch selected by the configured remote's local HEAD tracking ref.
/// This is network-free; `None` lets the frontend use conventional fallbacks.
#[tauri::command]
pub async fn vcs_default_branch(cwd: String) -> Option<String> {
    remote_default_branch(&cwd)
}

/// Normalize a git remote to an `https://host/owner/repo` browse URL:
///  - `git@github.com:owner/repo.git` → `https://github.com/owner/repo`
///  - `ssh://git@host/owner/repo.git` → `https://host/owner/repo`
///  - an `https://…/repo.git` just loses its `.git` suffix.
fn normalize_remote(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    // strip_suffix removes a single ".git"; trim_end_matches would peel repeated
    // suffixes (e.g. "repo.git.git" → "repo"), mangling a legitimate path.
    let stripped = trimmed.strip_suffix(".git").unwrap_or(trimmed);

    // scp-like syntax: `git@host:owner/repo`.
    let scp_like = stripped
        .strip_prefix("git@")
        .and_then(|rest| rest.split_once(':'));
    if let Some((host, path)) = scp_like {
        return format!("https://{host}/{path}");
    }
    // `ssh://git@host/owner/repo` or `git://host/owner/repo`.
    for prefix in ["ssh://git@", "ssh://", "git://"] {
        if let Some(rest) = stripped.strip_prefix(prefix) {
            return format!("https://{rest}");
        }
    }
    stripped.to_string()
}

#[cfg(test)]
mod tests {
    use super::{branch_remote_name, normalize_remote, remote_default_branch};

    #[test]
    fn configured_branch_remote_leads_over_origin() {
        let scratch =
            std::env::temp_dir().join(format!("pade-branch-remote-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("create scratch repository");
        let cwd = scratch.to_string_lossy();
        if super::run_git(&cwd, &["init", "-q"]).is_err() {
            let _ = std::fs::remove_dir_all(scratch);
            return;
        }

        super::run_git(
            &cwd,
            &[
                "-c",
                "user.name=PADE Test",
                "-c",
                "user.email=pade@example.invalid",
                "commit",
                "-qm",
                "baseline",
                "--allow-empty",
            ],
        )
        .expect("commit so HEAD points at a born branch");
        let branch = super::current_branch(&cwd).expect("read initial branch");
        let key = format!("branch.{branch}.remote");
        super::run_git(&cwd, &["config", &key, "upstream"]).expect("configure branch remote");
        super::run_git(
            &cwd,
            &[
                "symbolic-ref",
                "refs/remotes/upstream/HEAD",
                "refs/remotes/upstream/trunk",
            ],
        )
        .expect("configure remote head");

        assert_eq!(branch_remote_name(&cwd), "upstream");
        assert_eq!(remote_default_branch(&cwd).as_deref(), Some("trunk"));
        std::fs::remove_dir_all(scratch).expect("scratch cleanup");
    }

    #[test]
    fn scp_style_becomes_https() {
        assert_eq!(
            normalize_remote("git@github.com:avi/pade.git"),
            "https://github.com/avi/pade"
        );
    }

    #[test]
    fn ssh_scheme_becomes_https() {
        assert_eq!(
            normalize_remote("ssh://git@host.example/avi/pade.git"),
            "https://host.example/avi/pade"
        );
    }

    #[test]
    fn git_scheme_becomes_https() {
        assert_eq!(
            normalize_remote("git://host.example/avi/pade"),
            "https://host.example/avi/pade"
        );
    }

    #[test]
    fn https_just_loses_the_git_suffix_and_trailing_slash() {
        assert_eq!(
            normalize_remote("https://github.com/avi/pade.git/"),
            "https://github.com/avi/pade"
        );
    }

    #[test]
    fn only_one_git_suffix_is_peeled() {
        assert_eq!(
            normalize_remote("https://host/owner/repo.git.git"),
            "https://host/owner/repo.git"
        );
    }
}
