//! `git clone` for the picker's Get started card.
//!
//! Shells out to the user's `git`, like the rest of `vcs`. Passwords are exposed
//! only through a short-lived askpass environment, never process arguments or
//! the saved remote.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use tauri::Url;

use crate::util::{
    command, home_directory, is_on_path, percent_encode, since_unix_epoch, succeeds_within,
};
use crate::workspace::validated_child_path;

use super::DEFAULT_REMOTE_NAME;

/// Is the `git` CLI available? Gates the picker's Clone tab - without git the
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
    let Some(home) = home_directory() else {
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

/// Is `url` a repository the current environment can actually reach - it
/// exists, and the user's auth (SSH key, credential manager) can see it?
/// Backs the picker's live URL check: the destination folder name auto-fills
/// only once the repository answers. Prompts are disabled and the wait is
/// bounded, so a private repo the user can't see (or a host that never
/// answers) reports unreachable instead of hanging.
// `async` + `spawn_blocking`: a network round-trip (bounded, but seconds) that
// must never run synchronously on the MAIN thread.
#[tauri::command]
pub async fn vcs_probe_remote(url: String) -> bool {
    if !is_valid_clone_url(&url) {
        return false;
    }
    tauri::async_runtime::spawn_blocking(move || probe_remote(&url))
        .await
        .unwrap_or(false)
}

/// The bounded `git ls-remote` behind [`vcs_probe_remote`].
fn probe_remote(url: &str) -> bool {
    let mut ls_remote = command("git");
    ls_remote.args(["ls-remote", "--exit-code", "--", url, super::HEAD_REFERENCE]);
    super::harden_git(&mut ls_remote);
    succeeds_within(ls_remote, PROBE_REMOTE_TIMEOUT)
}

/// The `host` and `path` of any supported clone URL - `https://host/path`,
/// `ssh://git@host/path`, or scp-like `git@host:path` - or `None` when the
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
    // scp-like `git@github.com:org/repo.git` - a user@host before the colon.
    let (user_host, path) = url.split_once(':')?;
    user_host
        .contains('@')
        .then(|| (without_user(user_host), path.to_string()))
}

/// The longest clone URL accepted at the native boundary.
const MAX_CLONE_URL_LENGTH: usize = 2048;

/// The environment variable the askpass script reads the password from, so the
/// password never appears in a process argument.
const PASSWORD_ENVIRONMENT_VARIABLE: &str = "PADE_GIT_PASSWORD";

/// Clone transports accepted at the native boundary. Plain HTTP and `git://`
/// provide no server authentication, so the backend refuses them even if a
/// compromised renderer bypasses the form schema.
fn is_valid_clone_url(url: &str) -> bool {
    let has_forbidden_character = url
        .chars()
        .any(|character| character.is_control() || character.is_whitespace());
    if has_forbidden_character || url.len() > MAX_CLONE_URL_LENGTH {
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

/// The clean `https://host/path` form of `url` - what the saved remote and any
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
        let suffix = since_unix_epoch().as_nanos();
        let extension = if cfg!(windows) { "cmd" } else { "sh" };
        let path = std::env::temp_dir().join(format!(
            "pade-git-askpass-{}-{suffix}.{extension}",
            std::process::id()
        ));
        let script = if cfg!(windows) {
            format!("@echo off\r\n\"%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" -NoProfile -NonInteractive -Command \"[Console]::Out.Write($env:{PASSWORD_ENVIRONMENT_VARIABLE})\"\r\n")
        } else {
            format!("#!/bin/sh\nprintf '%s' \"${PASSWORD_ENVIRONMENT_VARIABLE}\"\n")
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
// takes - never on the MAIN thread, and too long for an async worker.
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
    if !is_valid_clone_url(url) {
        return Err("repository URL must use HTTPS or SSH".into());
    }
    let destination = validated_child_path(Path::new(&root), name)?;
    if destination.exists() {
        return Err("that folder already exists - pick another name".into());
    }

    let credentials = username.zip(password);
    let clone_url = match &credentials {
        Some((user, _)) => authenticated_url(url, user).ok_or(UNRECOGNIZED_URL_MESSAGE)?,
        None => url.to_string(),
    };

    let mut clone = command("git");
    clone
        .args(["-c", "credential.helper=", "clone", "--", &clone_url])
        .arg(&destination);
    super::harden_git(&mut clone);
    let askpass = credentials
        .as_ref()
        .map(|(_, password)| arm_askpass(&mut clone, password))
        .transpose()?;
    let output = clone
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    drop(askpass);

    let is_credentialed = credentials.is_some();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !is_credentialed {
            return Err(stderr);
        }
        let _ = std::fs::remove_dir_all(&destination);
        return Err(stderr.replace(&clone_url, &https_url(url).unwrap_or_default()));
    }

    if is_credentialed {
        let clean_url = https_url(url).ok_or(UNRECOGNIZED_URL_MESSAGE)?;
        remove_credentials_from_remote(&destination, &clean_url)?;
    }
    Ok(destination.to_string_lossy().into_owned())
}

const UNRECOGNIZED_URL_MESSAGE: &str = "unrecognized repository URL";

/// Point `clone` at a fresh askpass script that hands git `password` from the
/// environment. The returned guard deletes the script when dropped.
fn arm_askpass(clone: &mut Command, password: &str) -> Result<AskPass, String> {
    let askpass = AskPass::create()?;
    clone
        .env("GIT_ASKPASS", &askpass.path)
        .env("GIT_ASKPASS_REQUIRE", "force")
        .env(PASSWORD_ENVIRONMENT_VARIABLE, password);
    Ok(askpass)
}

/// Re-point a credentialed clone's remote at `clean_url` and confirm it stuck,
/// deleting the clone when either step fails so no username-bearing remote is
/// left behind in `.git/config`.
fn remove_credentials_from_remote(destination: &Path, clean_url: &str) -> Result<(), String> {
    let git_in_clone = |arguments: &[&str]| {
        command("git")
            .arg("-C")
            .arg(destination)
            .args(arguments)
            .output()
    };
    let discard_clone = || {
        let _ = std::fs::remove_dir_all(destination);
    };

    let sanitized = git_in_clone(&["remote", "set-url", DEFAULT_REMOTE_NAME, clean_url])
        .map_err(|e| format!("failed to sanitize cloned remote: {e}"))?;
    if !sanitized.status.success() {
        discard_clone();
        return Err("could not remove credentials from the cloned remote".into());
    }
    let stored = git_in_clone(&["remote", "get-url", DEFAULT_REMOTE_NAME])
        .map_err(|e| format!("failed to verify cloned remote: {e}"))?;
    let is_clean =
        stored.status.success() && String::from_utf8_lossy(&stored.stdout).trim() == clean_url;
    if !is_clean {
        discard_clone();
        return Err("could not verify the credential-free cloned remote".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{authenticated_url, host_and_path, https_url, is_valid_clone_url};

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
        assert!(is_valid_clone_url("https://github.com/org/repo.git"));
        assert!(is_valid_clone_url("ssh://git@github.com/org/repo.git"));
        assert!(is_valid_clone_url("git@github.com:org/repo.git"));
        assert!(!is_valid_clone_url("http://github.com/org/repo.git"));
        assert!(!is_valid_clone_url("git://github.com/org/repo.git"));
        assert!(!is_valid_clone_url("https://token@github.com/org/repo.git"));
        assert!(!is_valid_clone_url(
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
