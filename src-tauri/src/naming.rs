//! Auto-naming for temp workspaces.
//!
//! A throwaway workspace starts life as `temp-<stamp>`. Once the agent has done
//! real work in it, ADE derives a short, human-readable name and stores it as a
//! *display label* — never a disk rename: the live agent process holds the temp
//! dir as its cwd, which Windows locks against rename. The label surfaces in the
//! topbar and the Recent list.
//!
//! Naming is layered and swappable behind the `Namer` trait:
//!   1. the installed agent CLI, one-shot headless (`claude -p …`) — the primary,
//!      cross-platform path; reuses the user's subscription, no extra auth;
//!   2. Copilot on Windows (`copilot.rs`) — optional, currently a stub;
//!   3. a local heuristic (package/Cargo name, README heading, dominant file) —
//!      the always-on fallback.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::agents::oneshot_invocation;
use crate::util::is_on_path;

/// What a namer works from: the workspace's files (relative, `/`-joined) and an
/// optional first task prompt.
pub(crate) struct NameContext {
    pub files: Vec<String>,
    pub prompt: Option<String>,
}

/// One name source. Returns a *raw* candidate; the orchestrator sanitizes it.
pub(crate) trait Namer {
    fn suggest(&self, ctx: &NameContext) -> Option<String>;
}

/// Directories never worth scanning for a name (build output, VCS, deps).
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    ".git",
    ".svelte-kit",
    ".vite",
];

/// Suggest a name for the workspace at `path`, driven by `agent` (its command).
/// Read-only; returns `None` when nothing sensible can be derived. Runs the
/// (possibly slow) CLI call off the UI thread.
#[tauri::command]
pub async fn project_autoname(path: String, agent: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || autoname(&path, &agent))
        .await
        .ok()
        .flatten()
}

fn autoname(path: &str, agent: &str) -> Option<String> {
    // Only ever name ADE's own workspaces — never walk a real project's tree.
    if !crate::workspace::is_owned(path) {
        return None;
    }
    let dir = Path::new(path);
    let files = gather_files(dir);
    if files.is_empty() {
        return None;
    }
    let ctx = NameContext {
        files,
        prompt: None,
    };

    // Layered sources: agent CLI first, then (on Windows) Copilot, then heuristic.
    let mut namers: Vec<Box<dyn Namer>> = Vec::new();
    if let Some(args) = oneshot_invocation(agent) {
        if is_on_path(agent) {
            namers.push(Box::new(AgentCliNamer {
                command: agent.to_string(),
                args,
                cwd: dir.to_path_buf(),
            }));
        }
    }
    #[cfg(windows)]
    namers.push(Box::new(crate::copilot::CopilotNamer));
    namers.push(Box::new(HeuristicNamer {
        dir: dir.to_path_buf(),
    }));

    // First namer whose candidate survives sanitizing wins (lazy — later sources
    // only run if earlier ones yield nothing usable).
    namers
        .iter()
        .filter_map(|n| n.suggest(&ctx))
        .find_map(|raw| sanitize(&raw))
}

/// Collect up to a bounded set of the workspace's files as relative paths,
/// skipping dotfiles and build/dep noise.
fn gather_files(dir: &Path) -> Vec<String> {
    const MAX: usize = 40;
    const MAX_DEPTH: u8 = 2;
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0u8)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            if out.len() >= MAX {
                return out;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let is_noise = name.starts_with('.') || SKIP_DIRS.contains(&name.as_ref());
            if is_noise {
                continue;
            }
            let p = entry.path();
            if p.is_dir() {
                if depth < MAX_DEPTH {
                    stack.push((p, depth + 1));
                }
                continue;
            }
            if let Ok(rel) = p.strip_prefix(dir) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out
}

// ── Agent-CLI namer ─────────────────────────────────────────────────────────

/// Ask the installed agent to name the project via its one-shot headless mode.
struct AgentCliNamer {
    command: String,
    args: &'static [&'static str],
    cwd: PathBuf,
}

impl Namer for AgentCliNamer {
    fn suggest(&self, ctx: &NameContext) -> Option<String> {
        let mut cmd = Command::new(&self.command);
        cmd.current_dir(&self.cwd)
            .args(self.args)
            .arg(naming_prompt(ctx));
        let out = run_capture(cmd, Duration::from_secs(30))?;
        extract_name(&out)
    }
}

fn naming_prompt(ctx: &NameContext) -> String {
    let list = ctx
        .files
        .iter()
        .take(12)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    let mut p = String::from(
        "Suggest a concise, descriptive project name in kebab-case (2-4 lowercase \
         words joined by hyphens) for a codebase containing these files:\n",
    );
    p.push_str(&list);
    if let Some(task) = &ctx.prompt {
        p.push_str("\n\nInitial task: ");
        p.push_str(task);
    }
    p.push_str("\n\nReply with ONLY the name — no quotes, no explanation.");
    p
}

/// Pull the name out of a CLI reply: prefer a line that is already a bare token,
/// else the last non-empty line (models tend to conclude with the answer).
fn extract_name(out: &str) -> Option<String> {
    let bare = out.lines().map(str::trim).find(|l| {
        !l.is_empty()
            && l.len() <= 40
            && l.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    });
    bare.map(str::to_string).or_else(|| {
        out.lines()
            .rev()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(str::to_string)
    })
}

/// Run `cmd`, capturing stdout and killing it after `timeout`. stdin is closed so
/// a CLI that expects input gets EOF instead of hanging. Returns stdout on a
/// clean exit, else `None`.
fn run_capture(mut cmd: Command, timeout: Duration) -> Option<String> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                break;
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => return None,
        }
    }
    let mut buf = String::new();
    child.stdout.take()?.read_to_string(&mut buf).ok()?;
    Some(buf)
}

// ── Heuristic namer ─────────────────────────────────────────────────────────

/// Offline fallback: derive a name from the project's own metadata or files.
struct HeuristicNamer {
    dir: PathBuf,
}

impl Namer for HeuristicNamer {
    fn suggest(&self, ctx: &NameContext) -> Option<String> {
        pkg_name(&self.dir)
            .or_else(|| cargo_name(&self.dir))
            .or_else(|| readme_title(&self.dir))
            .or_else(|| dominant_stem(&ctx.files))
    }
}

fn read_file(dir: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(name)).ok()
}

fn pkg_name(dir: &Path) -> Option<String> {
    let text = read_file(dir, "package.json")?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let name = json.get("name")?.as_str()?;
    // Drop an npm scope: "@acme/widget" -> "widget".
    Some(name.rsplit('/').next().unwrap_or(name).to_string())
}

fn cargo_name(dir: &Path) -> Option<String> {
    let text = read_file(dir, "Cargo.toml")?;
    // Light scan — the first `name = "…"` (the [package] name) is enough here.
    text.lines()
        .map(str::trim)
        .find_map(|l| {
            let rest = l.strip_prefix("name")?.trim_start();
            Some(rest.strip_prefix('=')?.trim().trim_matches('"').to_string())
        })
        .filter(|s| !s.is_empty())
}

fn readme_title(dir: &Path) -> Option<String> {
    for candidate in ["README.md", "readme.md", "Readme.md", "README"] {
        if let Some(text) = read_file(dir, candidate) {
            if let Some(title) = text
                .lines()
                .map(str::trim)
                .find_map(|l| l.strip_prefix("# "))
            {
                return Some(title.to_string());
            }
        }
    }
    None
}

fn dominant_stem(files: &[String]) -> Option<String> {
    use std::collections::HashMap;
    const NOISE: &[&str] = &[
        "index", "main", "mod", "lib", "app", "readme", "license", "makefile",
    ];
    let mut counts: HashMap<String, usize> = HashMap::new();
    for f in files {
        let stem = Path::new(f)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if stem.is_empty() || NOISE.contains(&stem.as_str()) {
            continue;
        }
        *counts.entry(stem).or_insert(0) += 1;
    }
    counts.into_iter().max_by_key(|(_, n)| *n).map(|(s, _)| s)
}

// ── Sanitizer (shared) ──────────────────────────────────────────────────────

/// Normalize a raw candidate to a safe, short kebab-case label. `None` if nothing
/// usable survives. Shared with `workspace_set_label` so hand-set and derived
/// names go through the same gate.
pub(crate) fn sanitize(raw: &str) -> Option<String> {
    let first = raw.trim().lines().next().unwrap_or("").to_lowercase();
    let mut kebab = String::with_capacity(first.len());
    for ch in first.chars() {
        if ch.is_ascii_alphanumeric() {
            kebab.push(ch);
        } else if !kebab.ends_with('-') {
            kebab.push('-');
        }
    }
    // At most 4 words, then cap length (all-ASCII by now, so byte == char).
    let name = kebab
        .trim_matches('-')
        .split('-')
        .filter(|p| !p.is_empty())
        .take(4)
        .collect::<Vec<_>>()
        .join("-");
    let name = name
        .get(..name.len().min(40))
        .unwrap_or(&name)
        .trim_end_matches('-')
        .to_string();
    (name.len() >= 2).then_some(name)
}
