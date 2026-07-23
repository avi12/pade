//! `git clone` for the picker's Get started card.
//!
//! Shells out to the user's `git`, like the rest of `vcs`. Credentials, when
//! the user supplies them, ride the clone URL for that one command only — the
//! saved remote and any error text are scrubbed back to the clean URL before
//! either leaves this module.

use std::path::Path;

use std::time::Duration;

use crate::util::{command, home_dir, is_on_path, percent_encode, succeeds_within};

/// Is the `git` CLI available? Gates the picker's Clone tab — without git the
/// tab shows an install prompt instead of a clone form.
#[tauri::command]
pub async fn vcs_git_installed() -> bool {
    is_on_path("git")
}

/// The private-key filenames `ssh` tries by default; any one present means SSH
/// auth stands a chance.
const SSH_KEY_NAMES: &[&str] = &["id_ed25519", "id_ecdsa", "id_rsa", "id_dsa"];

/// Does the user have an SSH private key? An `ssh://`/`git@` clone URL without
/// one can't authenticate, so the picker offers HTTPS credentials instead.
#[tauri::command]
pub async fn vcs_has_ssh_key() -> bool {
    let Some(home) = home_dir() else {
        return false;
    };
    let ssh_dir = home.join(".ssh");
    SSH_KEY_NAMES
        .iter()
        .any(|name| ssh_dir.join(name).is_file())
}

/// How long the reachability probe may spend before ADE calls the repository
/// unreachable. Prompts are already disabled, but a firewalled host can sit on
/// the TCP connection far longer than a live URL check is worth.
const PROBE_REMOTE_TIMEOUT: Duration = Duration::from_secs(10);

/// Is `url` a repository the current environment can actually reach — it
/// exists, and the user's auth (SSH key, credential manager) can see it?
/// Backs the picker's live URL check: the destination folder name auto-fills
/// only once the repository answers. Prompts are disabled and the wait is
/// bounded, so a private repo the user can't see (or a host that never
/// answers) reports unreachable instead of hanging.
// `async` + `spawn_blocking`: a network round-trip (bounded, but seconds) that
// must never run synchronously on the MAIN thread.
#[tauri::command]
pub async fn vcs_probe_remote(url: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || probe_remote(&url))
        .await
        .unwrap_or(false)
}

/// The bounded `git ls-remote` behind [`vcs_probe_remote`].
fn probe_remote(url: &str) -> bool {
    let mut ls_remote = command("git");
    ls_remote
        .args(["ls-remote", "--exit-code", "--", url, "HEAD"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never");
    succeeds_within(ls_remote, PROBE_REMOTE_TIMEOUT)
}

/// The `host` and `path` of any supported clone URL — `https://host/path`,
/// `ssh://git@host/path`, or scp-like `git@host:path` — or `None` when the
/// shape isn't one of those.
fn host_and_path(url: &str) -> Option<(String, String)> {
    let without_user = |host: &str| {
        host.rsplit_once('@')
            .map_or_else(|| host.to_string(), |(_, bare)| bare.to_string())
    };
    for scheme in ["https://", "http://", "ssh://", "git://"] {
        if let Some(rest) = url.strip_prefix(scheme) {
            let (host, path) = rest.split_once('/')?;
            return Some((without_user(host), path.to_string()));
        }
    }
    // scp-like `git@github.com:org/repo.git` — a user@host before the colon.
    let (user_host, path) = url.split_once(':')?;
    user_host
        .contains('@')
        .then(|| (without_user(user_host), path.to_string()))
}

/// The clean `https://host/path` form of `url` — what the saved remote and any
/// surfaced error carry when credentials were used for the clone itself.
fn https_url(url: &str) -> Option<String> {
    host_and_path(url).map(|(host, path)| format!("https://{host}/{path}"))
}

/// The HTTPS form of `url` with percent-encoded credentials embedded — handed
/// to the one `git clone` invocation and nowhere else.
fn credential_url(url: &str, username: &str, password: &str) -> Option<String> {
    let (host, path) = host_and_path(url)?;
    let user = percent_encode(username, &[]);
    let pass = percent_encode(password, &[]);
    Some(format!("https://{user}:{pass}@{host}/{path}"))
}

/// Clone `url` into `root\name` and hand back the new project path. With
/// credentials the clone runs over HTTPS; the remote is then re-pointed at the
/// credential-free URL so nothing secret lands in `.git/config`.
// `async` + `spawn_blocking`: a clone runs for as long as the network transfer
// takes — never on the MAIN thread, and too long for an async worker.
#[tauri::command]
pub async fn vcs_clone(
    url: String,
    root: String,
    name: String,
    username: Option<String>,
    password: Option<String>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        clone_repository(&url, &root, &name, username, password)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The credential handling + `git clone` behind [`vcs_clone`].
fn clone_repository(
    url: &str,
    root: &str,
    name: &str,
    username: Option<String>,
    password: Option<String>,
) -> Result<String, String> {
    let dest = Path::new(&root).join(name.trim());
    if dest.exists() {
        return Err("that folder already exists — pick another name".into());
    }

    let credentials = username.zip(password);
    let clone_url = match &credentials {
        Some((user, pass)) => {
            credential_url(url, user, pass).ok_or("unrecognized repository URL")?
        }
        None => url.to_string(),
    };

    let out = command("git")
        .args(["clone", "--", &clone_url])
        .arg(&dest)
        // Fail fast with a real error instead of hanging on an invisible
        // terminal prompt (or a credential-manager popup) for a private repo.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    let credentials_embedded = credentials.is_some();
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if !credentials_embedded {
            return Err(stderr);
        }
        // git echoes the URL it tried — never let the embedded secret through.
        return Err(stderr.replace(&clone_url, &https_url(url).unwrap_or_default()));
    }

    if credentials_embedded {
        let clean = https_url(url).ok_or("unrecognized repository URL")?;
        let _ = command("git")
            .arg("-C")
            .arg(&dest)
            .args(["remote", "set-url", "origin", &clean])
            .output();
    }

    Ok(dest.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::{credential_url, host_and_path, https_url};

    #[test]
    fn host_and_path_reads_the_three_supported_shapes() {
        for url in [
            "https://github.com/org/repo.git",
            "ssh://git@github.com/org/repo.git",
            "git://github.com/org/repo.git",
            "git@github.com:org/repo.git",
        ] {
            assert_eq!(
                host_and_path(url),
                Some(("github.com".into(), "org/repo.git".into())),
                "shape: {url}"
            );
        }
    }

    #[test]
    fn host_and_path_rejects_a_windows_path_and_a_bare_word() {
        assert_eq!(host_and_path(r"C:\repositories\repo"), None);
        assert_eq!(host_and_path("repo"), None);
    }

    #[test]
    fn https_url_normalizes_an_ssh_remote() {
        assert_eq!(
            https_url("git@github.com:org/repo.git"),
            Some("https://github.com/org/repo.git".into())
        );
    }

    #[test]
    fn credential_url_percent_encodes_the_secret() {
        assert_eq!(
            credential_url("git@github.com:org/repo.git", "me@corp.com", "p@ss/word"),
            Some("https://me%40corp.com:p%40ss%2Fword@github.com/org/repo.git".into())
        );
    }
}
