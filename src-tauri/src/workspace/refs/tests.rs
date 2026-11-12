//! Tests for the move-directory reference updater.

use super::paths::{rebase_paths_in, vscode_uri};
use super::symlinks::moved_link_target;
use std::path::Path;

#[test]
fn vscode_uri_encodes_a_windows_path() {
    assert_eq!(
        vscode_uri(r"C:\repositories\avi\foo"),
        "file:///c%3A/repositories/avi/foo"
    );
}

#[test]
fn rebasing_covers_raw_escaped_forward_slash_and_uri_forms() {
    let haystack = "raw C:\\old\\proj\n json \"C:\\\\old\\\\proj\\\\src\"\n fwd 'C:/old/proj'\n uri file:///c%3A/old/proj";
    assert_eq!(
        rebase_paths_in(haystack, r"C:\old\proj", r"C:\new\proj"),
        "raw C:\\new\\proj\n json \"C:\\\\new\\\\proj\\\\src\"\n fwd 'C:/new/proj'\n uri file:///c%3A/new/proj"
    );
}

#[test]
fn rebasing_leaves_a_sibling_whose_name_merely_starts_the_same() {
    let haystack = r#"["C:/old/proj-effort", "C:/old/proj"]"#;
    assert_eq!(
        rebase_paths_in(haystack, r"C:\old\proj", r"C:\new\Proj"),
        r#"["C:/old/proj-effort", "C:/new/Proj"]"#
    );
}

#[test]
fn a_path_stored_in_lower_case_stays_lower_case() {
    let haystack = r"[projects.'c:\repositories\old']";
    assert_eq!(
        rebase_paths_in(haystack, r"C:\repositories\old", r"C:\repositories\New"),
        r"[projects.'c:\repositories\new']"
    );
}

/// An absolute path on this platform: `C:\…` on Windows, `/…` elsewhere.
fn absolute(relative: &str) -> String {
    if cfg!(windows) {
        format!(r"C:\{}", relative.replace('/', r"\"))
    } else {
        format!("/{relative}")
    }
}

fn moved(target: &str) -> Option<String> {
    moved_link_target(target, &absolute("old/proj"), &absolute("new/proj"))
}

#[test]
fn a_relative_link_target_is_left_alone() {
    assert_eq!(moved("relative/path"), None);
}

#[test]
fn a_link_target_under_the_old_tree_swings_to_the_new_one() {
    assert_eq!(
        moved(&absolute("old/proj/node_modules/pkg")),
        Some(absolute("new/proj/node_modules/pkg"))
    );
}

#[test]
fn a_link_target_outside_the_old_tree_is_left_alone() {
    assert_eq!(moved(&absolute("elsewhere/x")), None);
}

#[test]
fn a_link_into_a_sibling_sharing_the_name_prefix_is_left_alone() {
    assert_eq!(moved(&absolute("old/proj-effort/x")), None);
}

#[test]
fn a_link_target_matches_the_old_folder_ascii_case_insensitively() {
    assert_eq!(moved(&absolute("OLD/Proj/x")), Some(absolute("new/proj/x")));
}

/// The Kelvin sign lowercases to an ASCII `k` but is three bytes long, so a
/// Unicode-lowercased prefix match used to slice the target mid-character.
#[test]
fn a_non_ascii_lookalike_never_matches_or_panics() {
    assert_eq!(
        moved_link_target(&absolute("\u{212A}/x"), &absolute("k"), &absolute("new")),
        None
    );
}

/// A pnpm tree's junctions all point at absolute paths inside the project;
/// after the move each one is re-pointed under the new folder.
#[cfg(windows)]
#[test]
fn a_junction_into_the_moved_folder_is_re_pointed() {
    let scratch = std::env::temp_dir().join(format!("pade-relink-{}", std::process::id()));
    let old = scratch.join("old");
    let new = scratch.join("new");
    std::fs::create_dir_all(new.join("store").join("package")).expect("store folder");
    let link = new.join("package");
    let is_created = crate::util::command(crate::util::COMMAND_SHELL)
        .args([crate::util::COMMAND_SHELL_RUN_FLAG, "mklink", "/J"])
        .arg(&link)
        .arg(old.join("store").join("package"))
        .output()
        .is_ok_and(|output| output.status.success());
    if is_created {
        super::symlinks::fix_project_symlinks(&new, &old.to_string_lossy(), &new.to_string_lossy());
        let target = std::fs::read_link(&link).expect("re-pointed junction");
        assert!(
            target.ends_with(Path::new("new").join("store").join("package")),
            "{}",
            target.display()
        );
    }
    std::fs::remove_dir(&link).ok();
    std::fs::remove_dir_all(&scratch).ok();
}

#[cfg(windows)]
#[test]
fn a_junction_script_doubles_percent_signs_and_quotes_each_path() {
    let junction = super::symlinks::StaleLink {
        link: std::path::PathBuf::from(r"C:\a & b\100%"),
        target: r"C:\c\100%".to_string(),
        is_target_directory: true,
    };
    let script = super::symlinks::junction_script(&[&junction]);
    assert!(
        script.ends_with("@mklink /J \"C:\\a & b\\100%%\" \"C:\\c\\100%%\" >nul\r\n"),
        "{script}"
    );
}
