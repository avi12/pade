//! Remote-URL resolution — the browsable base behind "open on GitHub".

use super::run_git;

/// The `origin` remote URL, normalized to a browsable `https://host/owner/repo`
/// form. `None` when there's no `origin` remote.
#[tauri::command]
pub fn vcs_remote_url(cwd: String) -> Option<String> {
    let raw = run_git(&cwd, &["remote", "get-url", "origin"]).ok()?;
    let url = raw.trim();
    if url.is_empty() {
        return None;
    }
    Some(normalize_remote(url))
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
    if let Some(rest) = stripped.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            return format!("https://{host}/{path}");
        }
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
    use super::normalize_remote;

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
