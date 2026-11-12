use super::executables::find_in;
use super::ipc::{owner_access, OwnerAccess};
use super::json::strip_jsonc_sugar;
use super::process::capture_within;
use super::project_encoding::encode_project;
#[cfg(windows)]
use super::registry::expand_environment_variables;
use std::fs;
use std::path::PathBuf;
use std::slice;
use std::time::{Duration, Instant};

/// Output bigger than any OS pipe buffer (64 KB on Windows and Linux).
const OVERSIZED_OUTPUT_BYTES: usize = 128 * 1024;
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(20);

/// A shell line printing more than [`OVERSIZED_OUTPUT_BYTES`] of text.
fn oversized_output_command() -> String {
    if cfg!(windows) {
        "for /L %i in (1,1,2500) do @echo 0123456789012345678901234567890123456789012345678901234567890123"
            .to_string()
    } else {
        format!("yes 0123456789 | head -c {}", OVERSIZED_OUTPUT_BYTES + 1)
    }
}

/// The reader used to start only after the exit, so output that filled the
/// pipe blocked the child until the timeout killed it and nothing came back.
#[test]
fn output_bigger_than_the_pipe_buffer_is_captured_without_waiting_out_the_timeout() {
    let started = Instant::now();
    let run = capture_within(
        super::shell_command(&oversized_output_command()),
        CAPTURE_TIMEOUT,
    )
    .expect("a clean exit returns its whole output");
    assert!(run.is_success);
    assert!(run.stdout.len() > OVERSIZED_OUTPUT_BYTES);
    assert!(started.elapsed() < CAPTURE_TIMEOUT);
}

#[test]
fn a_failed_run_still_answers_what_it_printed_to_stderr() {
    let run = capture_within(
        super::shell_command("echo not logged in 1>&2 && exit 3"),
        CAPTURE_TIMEOUT,
    )
    .expect("the run finishes");
    assert!(!run.is_success);
    assert!(run.stderr.contains("not logged in"), "{}", run.stderr);
}

/// A scratch directory populated with empty, executable `files`.
fn scratch(name: &str, files: &[&str]) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("pade-search-{name}"));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("create scratch dir");
    for file in files {
        let path = directory.join(file);
        fs::write(&path, b"").expect("create scratch file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
                .expect("mark scratch file executable");
        }
    }
    directory
}

const CODEX_NAMES: &[&str] = &["codex", "codex-x86_64-pc-windows-msvc"];

#[test]
fn owner_access_distinguishes_vacant_owned_and_foreign_resources() {
    assert_eq!(owner_access(None, "main"), OwnerAccess::Vacant);
    assert_eq!(owner_access(Some("main"), "main"), OwnerAccess::Owned);
    assert_eq!(owner_access(Some("other"), "main"), OwnerAccess::Foreign);
}

#[test]
fn find_in_falls_back_to_an_alias() {
    // What winget's Codex package actually looks like: no `codex` anywhere,
    // only the release binary under its target-triple name.
    let directory = scratch("alias", &["codex-x86_64-pc-windows-msvc"]);
    assert_eq!(
        find_in(slice::from_ref(&directory), CODEX_NAMES),
        Some(directory.join("codex-x86_64-pc-windows-msvc"))
    );
}

#[test]
fn find_in_prefers_the_canonical_name_over_an_alias() {
    let directory = scratch("canonical", &["codex", "codex-x86_64-pc-windows-msvc"]);
    assert_eq!(
        find_in(slice::from_ref(&directory), CODEX_NAMES),
        Some(directory.join("codex"))
    );
}

#[test]
fn find_in_reports_a_command_that_is_not_installed() {
    let directory = scratch("empty", &[]);
    assert_eq!(find_in(slice::from_ref(&directory), CODEX_NAMES), None);
}

/// The whole point of the JSONC reader: a settings file a human edits keeps
/// its comments, and the `//` inside a URL is not one of them.
#[test]
fn strip_jsonc_sugar_drops_comments_but_never_touches_a_string() {
    let raw = concat!(
        "{\n",
        "  // the schema link is a comment magnet\n",
        "  \"$schema\": \"https://aka.ms/terminal-profiles-schema\",\n",
        "  /* block comments too */\n",
        "  \"quoted\": \"a // b /* c */ d\",\n",
        "  \"escaped\": \"a \\\" // still a string\",\n",
        "  \"trailing\": [1, 2,],\n",
        "}\n"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&strip_jsonc_sugar(raw)).expect("JSONC parses once relaxed");
    assert_eq!(parsed["$schema"], "https://aka.ms/terminal-profiles-schema");
    assert_eq!(parsed["quoted"], "a // b /* c */ d");
    assert_eq!(parsed["escaped"], "a \" // still a string");
    assert_eq!(parsed["trailing"], serde_json::json!([1, 2]));
}

/// The registry hands back `PATH` as a `REG_EXPAND_SZ`, so a directory only
/// becomes searchable once its `%VAR%`s are resolved.
#[cfg(windows)]
#[test]
fn expand_environment_resolves_known_variables_and_leaves_unknown_ones() {
    let home = std::env::var("USERPROFILE").expect("USERPROFILE is always set on Windows");
    assert_eq!(
        expand_environment_variables(r"%USERPROFILE%\bin;%PADE_UNSET_VAR%\bin"),
        format!(r"{home}\bin;%PADE_UNSET_VAR%\bin")
    );
}

mod project_encoding_tests {
    use super::encode_project;

    #[test]
    fn every_character_but_a_letter_or_digit_becomes_a_dash() {
        assert_eq!(
            encode_project(r"C:\repositories\my_app.v2 (old)"),
            "C--repositories-my-app-v2--old-"
        );
    }

    #[test]
    fn a_long_path_is_cut_and_suffixed_with_claudes_hash() {
        let path = format!(r"C:\Users\Avi\AppData\Local\Temp\{}\é.dir", "x".repeat(200));
        let encoded = encode_project(&path);
        assert_eq!(encoded.len(), 207);
        assert_eq!(&encoded[196..], "xxxx-v0c901");
    }
}
