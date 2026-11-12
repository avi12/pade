//! The IDE adapters: `JetBrains`, VS Code and Cursor recent-project lists, run only where the moved folder carries that IDE's marker.

use super::paths::moved_path;
use super::paths::rewrite_file_forms;
use std::path::Path;

// ---------------------------------------------------------------------------
// IDE adapter table - marker-gated, easy to extend.
// ---------------------------------------------------------------------------

/// How to re-point one IDE's stored references after a move.
///
/// `marker` is the project-local dir that proves the IDE was used here (checked
/// under the NEW path); the adapter only runs when it exists. `apply` performs
/// the IDE-specific rewrites. Adding an IDE is one more entry in `IDE_ADAPTERS`.
pub(super) struct IdeAdapter {
    /// Project-local marker dir gating this adapter (e.g. `.idea`, `.vscode`).
    marker: &'static str,
    /// Re-point this IDE's references. `new_directory` is the moved project (for
    /// project-local files like `.idea`); `old`/`new` are the absolute paths.
    apply: fn(new_directory: &Path, old: &str, new: &str),
}

/// The IDEs we know how to re-point. Extend this table to support more.
pub(super) const IDE_ADAPTERS: &[IdeAdapter] = &[
    // JetBrains: project-local `.idea/*.xml|*.iml` (safety net) + global recents.
    IdeAdapter {
        marker: ".idea",
        apply: apply_jetbrains,
    },
    // VS Code: recents under ~/AppData/Roaming/Code/User/globalStorage.
    IdeAdapter {
        marker: ".vscode",
        apply: apply_vscode,
    },
    // Cursor (a VS Code fork): same on-disk recents format, under
    // ~/AppData/Roaming/Cursor/... . Opening a folder in Cursor writes `.vscode`;
    // some setups also carry `.cursor`. Gate on either via two entries.
    IdeAdapter {
        marker: ".vscode",
        apply: apply_cursor,
    },
    IdeAdapter {
        marker: ".cursor",
        apply: apply_cursor,
    },
    // TODO: Zed - recents in ~/AppData/Roaming/Zed (SQLite `db.sqlite`); marker
    // `.zed`. Left as a stub until its schema is pinned down.
    // TODO: Sublime Text - session in
    // ~/AppData/Roaming/Sublime Text/Local/Session.sublime_session; marker
    // `.sublime-project`. Left as a stub.
];

/// Run every IDE adapter whose marker dir is present in the moved project.
pub(super) fn rewrite_ide_recents(new_directory: &Path, old: &str, new: &str) {
    for adapter in IDE_ADAPTERS {
        if new_directory.join(adapter.marker).is_dir() {
            (adapter.apply)(new_directory, old, new);
        }
    }
}

/// `JetBrains` adapter: project-local `.idea` files, then the global recents.
pub(super) fn apply_jetbrains(new_directory: &Path, old: &str, new: &str) {
    rewrite_idea(new_directory, old, new);
    rewrite_jetbrains_recents(old, new);
}

/// VS Code adapter: rewrite recents under the VS Code `globalStorage` base.
pub(super) fn apply_vscode(_new_directory: &Path, old: &str, new: &str) {
    rewrite_vscode_family(VsCodeApp::Code, old, new);
}

/// Cursor adapter: identical stores to VS Code, under the Cursor base.
pub(super) fn apply_cursor(_new_directory: &Path, old: &str, new: &str) {
    rewrite_vscode_family(VsCodeApp::Cursor, old, new);
}

/// A VS Code-family app, identifying its `~/AppData/Roaming/<dir>` base. The
/// on-disk `globalStorage` layout (`storage.json` + `state.vscdb`) is identical
/// across the family, so one code path handles them all (DRY).
#[derive(Clone, Copy)]
pub(super) enum VsCodeApp {
    Code,
    Cursor,
}

impl VsCodeApp {
    /// The app's directory name under `~/AppData/Roaming`.
    fn roaming_directory(self) -> &'static str {
        match self {
            VsCodeApp::Code => "Code",
            VsCodeApp::Cursor => "Cursor",
        }
    }
}

// ---------------------------------------------------------------------------
// JetBrains `.idea` project files (cross-platform).
// ---------------------------------------------------------------------------

/// Rewrite absolute occurrences of `old` → `new` in every `*.xml` / `*.iml`
/// under `<new_directory>/.idea/`, in both backslash and forward-slash forms.
/// `.idea` is mostly `$PROJECT_DIR$`-relative, so this is a light safety net for
/// the occasional stored absolute path. Best-effort per file.
pub(super) fn rewrite_idea(new_directory: &Path, old: &str, new: &str) {
    let idea = new_directory.join(".idea");
    let Ok(entries) = std::fs::read_dir(&idea) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_idea_file = matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("xml" | "iml")
        );
        if is_idea_file {
            rewrite_file_forms(&path, old, new);
        }
    }
}

// ---------------------------------------------------------------------------
// Windows-only IDE recents (real bodies below, no-ops off-Windows).
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub(super) mod windows {
    use std::path::PathBuf;

    use crate::util::home_directory;

    use super::{rewrite_file_forms, VsCodeApp};

    /// Rewrite `old` → `new` in every `JetBrains` IDE's `recentProjects.xml`
    /// (`~/AppData/Roaming/JetBrains/*/options/recentProjects.xml`) - one file per
    /// installed IDE version. Both backslash and forward-slash forms. Best-effort.
    pub(super) fn rewrite_jetbrains_recents(old: &str, new: &str) {
        let Some(home) = home_directory() else { return };
        let jetbrains = home.join("AppData").join("Roaming").join("JetBrains");
        let Ok(entries) = std::fs::read_dir(&jetbrains) else {
            return;
        };
        for entry in entries.flatten() {
            let recents = entry.path().join("options").join("recentProjects.xml");
            if recents.is_file() {
                rewrite_file_forms(&recents, old, new);
            }
        }
    }

    /// The VS Code-family app's `User/globalStorage` dir, if home is known.
    fn global_storage(app: VsCodeApp) -> Option<PathBuf> {
        Some(
            home_directory()?
                .join("AppData")
                .join("Roaming")
                .join(app.roaming_directory())
                .join("User")
                .join("globalStorage"),
        )
    }

    /// Rewrite `old` → `new` across one VS Code-family app's recents: the
    /// plain-text `storage.json` and the `SQLite` `state.vscdb`. Best-effort.
    pub(super) fn rewrite_vscode_family(app: VsCodeApp, old: &str, new: &str) {
        let Some(base) = global_storage(app) else {
            return;
        };
        rewrite_vscode_storage(&base, old, new);
        rewrite_vscode_database(&base, old, new);
    }

    /// Rewrite `old` → `new` in `<globalStorage>/storage.json` (which caches,
    /// among other things, recently-opened paths) in backslash, forward-slash,
    /// and file-URI forms. Best-effort.
    fn rewrite_vscode_storage(base: &std::path::Path, old: &str, new: &str) {
        let storage = base.join("storage.json");
        if storage.is_file() {
            rewrite_file_forms(&storage, old, new);
        }
    }

    /// Rewrite `old` → `new` in `<globalStorage>/state.vscdb`'s recents list.
    ///
    /// `state.vscdb` is a `SQLite` database; the recents live in table `ItemTable`,
    /// key `history.recentlyOpenedPathsList`, whose value is a JSON string of file
    /// URIs. We open the DB read-write, replace all path forms in that JSON, and
    /// write the row back. Best-effort: if the DB is missing or locked (the app is
    /// open), any step errors and we simply return.
    fn rewrite_vscode_database(base: &std::path::Path, old: &str, new: &str) {
        use rusqlite::Connection;

        const RECENTS_KEY: &str = "history.recentlyOpenedPathsList";

        let database = base.join("state.vscdb");
        if !database.is_file() {
            return;
        }

        let Ok(connection) = Connection::open(&database).inspect_err(|error| {
            eprintln!("refs: failed to open {}: {error}", database.display());
        }) else {
            return;
        };

        // No such row / locked / other - nothing safe to do.
        let Ok(value) = connection
            .query_row(
                "SELECT value FROM ItemTable WHERE key = ?1",
                [RECENTS_KEY],
                |row| row.get::<_, String>(0),
            )
            .inspect_err(|error| {
                eprintln!(
                    "refs: could not read recents from {}: {error}",
                    database.display()
                );
            })
        else {
            return;
        };

        let Some(updated) = super::moved_path(&value, old, new) else {
            return;
        };

        if let Err(error) = connection.execute(
            "UPDATE ItemTable SET value = ?1 WHERE key = ?2",
            rusqlite::params![updated, RECENTS_KEY],
        ) {
            eprintln!(
                "refs: failed to update recents in {}: {error}",
                database.display()
            );
        }
    }
}

#[cfg(windows)]
use windows::{rewrite_jetbrains_recents, rewrite_vscode_family};

// Off-Windows: no JetBrains/VS Code layout to touch. No-op stubs keep the IDE
// adapters cross-platform and the crate compiling everywhere.
#[cfg(not(windows))]
pub(super) fn rewrite_jetbrains_recents(_old: &str, _new: &str) {}

#[cfg(not(windows))]
pub(super) fn rewrite_vscode_family(_app: VsCodeApp, _old: &str, _new: &str) {}
