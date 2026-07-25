//! `git clone` for the picker's Get started card.
//!
//! Shells out to the user's `git`, like the rest of `vcs`. Passwords are exposed
//! only through a short-lived askpass environment, never process arguments or
//! the saved remote.

use std::path::{Path, PathBuf};

use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Url;

use crate::util::{command, home_dir, is_on_path, percent_encode, succeeds_within};
use crate::workspace::validated_child_path;

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
    if !validate_clone_url(&url) {
        return false;
    }
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

/// Clone transports accepted at the native boundary. Plain HTTP and `git://`
/// provide no server authentication, so the backend refuses them even if a
/// compromised renderer bypasses the form schema.
fn validate_clone_url(url: &str) -> bool {
    let has_forbidden_character = url
        .chars()
        .any(|character| character.is_control() || character.is_whitespace());
    if has_forbidden_character || url.len() > 2048 {
        return false;
    }
    if let Ok(parsed) = Url::parse(url) {
        let secure_scheme = matches!(parsed.scheme(), "https" | "ssh");
        let has_host_and_path = parsed.host_str().is_some() && parsed.path() != "/";
        let has_embedded_secret = parsed.password().is_some()
            || (parsed.scheme() == "https" && !parsed.username().is_empty());
        return secure_scheme && has_host_and_path && !has_embedded_secret;
    }

    let Some((user_host, path)) = url.split_once(':') else {
        return false;
    };
    let Some((user, host)) = user_host.split_once('@') else {
        return false;
    };
    !user.is_empty() && !host.is_empty() && !path.is_empty()
}

/// The clean `https://host/path` form of `url` — what the saved remote and any
/// surfaced error carry when credentials were used for the clone itself.
fn https_url(url: &str) -> Option<String> {
    host_and_path(url).map(|(host, path)| format!("https://{host}/{path}"))
}

/// The HTTPS form with only the non-secret username included. Git obtains the
/// password through askpass, so it never appears in argv or `.git/config`.
fn authenticated_url(url: &str, username: &str) -> Option<String> {
    let (host, path) = host_and_path(url)?;
    let user = percent_encode(username, &[]);
    Some(format!("https://{user}@{host}/{path}"))
}

struct AskPass {
    path: PathBuf,
}

impl AskPass {
    fn create() -> Result<Self, String> {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let extension = if cfg!(windows) { "cmd" } else { "sh" };
        let path = std::env::temp_dir().join(format!(
            "pade-git-askpass-{}-{suffix}.{extension}",
            std::process::id()
        ));
        let script = if cfg!(windows) {
            "@echo off\r\n\"%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" -NoProfile -NonInteractive -Command \"[Console]::Out.Write($env:PADE_GIT_PASSWORD)\"\r\n"
        } else {
            "#!/bin/sh\nprintf '%s' \"$PADE_GIT_PASSWORD\"\n"
        };
        std::fs::write(&path, script).map_err(|e| e.to_string())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        Ok(Self { path })
    }
}

impl Drop for AskPass {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
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
    if !validate_clone_url(url) {
        return Err("repository URL must use HTTPS or SSH".into());
    }
    let destination = validated_child_path(Path::new(&root), name)?;
    if destination.exists() {
        return Err("that folder already exists — pick another name".into());
    }

    let credentials = username.zip(password);
    let clone_url = match &credentials {
        Some((user, _)) => authenticated_url(url, user).ok_or("unrecognized repository URL")?,
        None => url.to_string(),
    };

    let mut clone = command("git");
    clone
        .args(["-c", "credential.helper=", "clone", "--", &clone_url])
        .arg(&destination)
        // Fail fast with a real error instead of hanging on an invisible
        // terminal prompt (or a credential-manager popup) for a private repo.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never");
    let askpass = if let Some((_, password)) = &credentials {
        let askpass = AskPass::create()?;
        clone
            .env("GIT_ASKPASS", &askpass.path)
            .env("GIT_ASKPASS_REQUIRE", "force")
            .env("PADE_GIT_PASSWORD", password);
        Some(askpass)
    } else {
        None
    };
    let output = clone
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    drop(askpass);

    let credentials_supplied = credentials.is_some();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !credentials_supplied {
            return Err(stderr);
        }
        let _ = std::fs::remove_dir_all(&destination);
        return Err(stderr.replace(&clone_url, &https_url(url).unwrap_or_default()));
    }

    if !credentials_supplied {
        return Ok(destination.to_string_lossy().into_owned());
    }

    let clean = https_url(url).ok_or("unrecognized repository URL")?;
    let sanitized = command("git")
        .arg("-C")
        .arg(&destination)
        .args(["remote", "set-url", "origin", &clean])
        .output()
        .map_err(|e| format!("failed to sanitize cloned remote: {e}"))?;
    if !sanitized.status.success() {
        let _ = std::fs::remove_dir_all(&destination);
        return Err("could not remove credentials from the cloned remote".into());
    }
    let stored = command("git")
        .arg("-C")
        .arg(&destination)
        .args(["remote", "get-url", "origin"])
        .output()
        .map_err(|e| format!("failed to verify cloned remote: {e}"))?;
    if !stored.status.success() || String::from_utf8_lossy(&stored.stdout).trim() != clean {
        let _ = std::fs::remove_dir_all(&destination);
        return Err("could not verify the credential-free cloned remote".into());
    }

    Ok(destination.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::{authenticated_url, host_and_path, https_url, validate_clone_url};

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
    fn clone_urls_require_an_authenticated_transport() {
        assert!(validate_clone_url("https://github.com/org/repo.git"));
        assert!(validate_clone_url("ssh://git@github.com/org/repo.git"));
        assert!(validate_clone_url("git@github.com:org/repo.git"));
        assert!(!validate_clone_url("http://github.com/org/repo.git"));
        assert!(!validate_clone_url("git://github.com/org/repo.git"));
        assert!(!validate_clone_url("https://token@github.com/org/repo.git"));
        assert!(!validate_clone_url(
            "https://github.com/org/repo.git\n--upload-pack=bad"
        ));
    }

    #[test]
    fn https_url_normalizes_an_ssh_remote() {
        assert_eq!(
            https_url("git@github.com:org/repo.git"),
            Some("https://github.com/org/repo.git".into())
        );
    }

    #[test]
    fn authenticated_url_keeps_the_password_out_of_argv() {
        assert_eq!(
            authenticated_url("git@github.com:org/repo.git", "me@corp.com"),
            Some("https://me%40corp.com@github.com/org/repo.git".into())
        );
    }
}
