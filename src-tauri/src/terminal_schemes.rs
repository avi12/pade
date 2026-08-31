//! The Windows Terminal colour schemes PADE offers for its own terminal.
//!
//! Three sources, one catalogue. The schemes Windows Terminal *ships* live in a
//! `defaults.json` compiled into the app package, which PADE cannot count on
//! being installed or readable — so they are vendored here as an asset
//! (principle #10: nothing fetched at runtime). A short *curated* list adds the
//! schemes that asset leaves out, in the same vocabulary. The schemes the user
//! *wrote* come from their own `settings.json`, read live so a scheme they add
//! shows up without a PADE release. Later sources win on a name collision, so a
//! user scheme beats a curated one and a curated one beats a shipped one —
//! which is exactly what Windows Terminal itself does with a user override.
//!
//! On the wire the palette is an ANSI-indexed array rather than Windows
//! Terminal's twenty named colour fields: index *is* the SGR colour, which is
//! the order the frontend's `ANSI_COLOR_TOKENS` already maps onto the
//! `--terminal-*` design tokens. The named fields stay in the deserializer,
//! where the files that use that vocabulary are read.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The shipped schemes, vendored from Windows Terminal's own `defaults.json`
/// (MIT). Regenerate from an installed Windows Terminal to pick up new ones.
const BUNDLED_SCHEMES: &str = include_str!("../assets/windows-terminal-schemes.json");

/// Schemes Windows Terminal does not ship, kept in their own asset so
/// regenerating [`BUNDLED_SCHEMES`] from an installed Windows Terminal can never
/// drop them. Currently the two light schemes worth offering: Selenized Light
/// (Solarized rebuilt in CIE L*a*b*, which fixes the low contrast Solarized
/// Light is criticised for) and Catppuccin Latte. The vendored asset carries
/// only three light schemes, all of them dated, and a light app theme with a
/// near-black terminal is the mismatch this closes.
const CURATED_SCHEMES: &str = include_str!("../assets/curated-terminal-schemes.json");

/// One colour scheme in PADE's vocabulary.
#[derive(Serialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TerminalScheme {
    pub name: String,
    pub background: String,
    pub foreground: String,
    /// Absent in several shipped schemes; the frontend falls back to the
    /// foreground rather than inventing a colour.
    pub cursor: Option<String>,
    /// Likewise absent in several — Windows Terminal composites its own default
    /// wash there, and PADE derives one from the foreground.
    pub selection: Option<String>,
    /// The 16 ANSI colours in ANSI order: 0–7 standard, 8–15 bright.
    pub ansi: [String; 16],
}

/// A scheme as Windows Terminal writes it — its own twenty-field vocabulary,
/// read from `defaults.json`, the vendored asset, and the user's settings alike.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowsTerminalScheme {
    name: String,
    background: String,
    foreground: String,
    cursor_color: Option<String>,
    selection_background: Option<String>,
    black: String,
    red: String,
    green: String,
    yellow: String,
    blue: String,
    purple: String,
    cyan: String,
    white: String,
    bright_black: String,
    bright_red: String,
    bright_green: String,
    bright_yellow: String,
    bright_blue: String,
    bright_purple: String,
    bright_cyan: String,
    bright_white: String,
}

impl From<WindowsTerminalScheme> for TerminalScheme {
    fn from(scheme: WindowsTerminalScheme) -> Self {
        Self {
            name: scheme.name,
            background: scheme.background,
            foreground: scheme.foreground,
            cursor: scheme.cursor_color,
            selection: scheme.selection_background,
            ansi: [
                scheme.black,
                scheme.red,
                scheme.green,
                scheme.yellow,
                scheme.blue,
                scheme.purple,
                scheme.cyan,
                scheme.white,
                scheme.bright_black,
                scheme.bright_red,
                scheme.bright_green,
                scheme.bright_yellow,
                scheme.bright_blue,
                scheme.bright_purple,
                scheme.bright_cyan,
                scheme.bright_white,
            ],
        }
    }
}

/// Deserialize each entry on its own, so one scheme the user broke mid-edit
/// costs only itself instead of the whole file.
fn schemes_from_values(values: Vec<serde_json::Value>) -> Vec<TerminalScheme> {
    values
        .into_iter()
        .filter_map(|value| {
            serde_json::from_value::<WindowsTerminalScheme>(value)
                .ok()
                .map(TerminalScheme::from)
        })
        .collect()
}

/// The `schemes` array of one Windows Terminal settings file, or nothing when
/// the file is absent or unreadable — every path below is a build that may
/// simply not be installed.
fn schemes_in_settings(path: &Path) -> Vec<TerminalScheme> {
    let Some(serde_json::Value::Array(schemes)) = crate::util::read_jsonc(path)
        .as_mut()
        .and_then(|settings| settings.get_mut("schemes"))
        .map(serde_json::Value::take)
    else {
        return Vec::new();
    };

    schemes_from_values(schemes)
}

/// Every settings file a Windows Terminal build keeps its user schemes in: the
/// Store release, the Store preview, and the unpackaged (portable/`scoop`)
/// build. All three are read — a machine can have more than one.
fn user_settings_paths() -> Vec<PathBuf> {
    let Some(local_app_data) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else {
        return Vec::new();
    };

    let packaged = |package: &str| {
        local_app_data
            .join("Packages")
            .join(package)
            .join("LocalState")
            .join("settings.json")
    };
    vec![
        packaged("Microsoft.WindowsTerminal_8wekyb3d8bbwe"),
        packaged("Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe"),
        local_app_data
            .join("Microsoft")
            .join("Windows Terminal")
            .join("settings.json"),
    ]
}

/// Every scheme in one vendored asset, or nothing when it will not parse — an
/// asset is data, and a broken one must not take the catalogue down with it.
fn schemes_in_asset(asset: &str) -> Vec<TerminalScheme> {
    schemes_from_values(serde_json::from_str(asset).unwrap_or_default())
}

/// The merged catalogue, by name — later sources overwrite earlier ones, so the
/// user's own schemes win over curated, and curated over shipped. Sorted by
/// construction (`BTreeMap`), which is the order the picker shows.
fn catalogue(settings_paths: &[PathBuf]) -> Vec<TerminalScheme> {
    let mut by_name: BTreeMap<String, TerminalScheme> = schemes_in_asset(BUNDLED_SCHEMES)
        .into_iter()
        .chain(schemes_in_asset(CURATED_SCHEMES))
        .map(|scheme| (scheme.name.clone(), scheme))
        .collect();

    for path in settings_paths {
        for scheme in schemes_in_settings(path) {
            by_name.insert(scheme.name.clone(), scheme);
        }
    }

    by_name.into_values().collect()
}

/// Every colour scheme the terminal can be painted with.
// `async` so Tauri runs it off the main thread: it reads files, and a
// synchronous command would do that on the Win32 message pump.
#[tauri::command]
pub async fn terminal_schemes() -> Vec<TerminalScheme> {
    catalogue(&user_settings_paths())
}

#[cfg(test)]
mod tests {
    use super::{catalogue, schemes_in_settings, TerminalScheme, BUNDLED_SCHEMES};
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pade-terminal-schemes-{}-{name}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir.join("settings.json")
    }

    /// The vendored asset is the whole catalogue on a machine with no Windows
    /// Terminal settings of its own, so it has to parse and carry the schemes
    /// people actually name.
    #[test]
    fn the_vendored_asset_carries_the_shipped_schemes() {
        let schemes = catalogue(&[]);
        assert!(schemes.len() >= 9, "{} schemes", schemes.len());

        let campbell = schemes
            .iter()
            .find(|scheme| scheme.name == "Campbell")
            .expect("Campbell is a shipped scheme");
        assert_eq!(campbell.background, "#0C0C0C");
        assert_eq!(campbell.ansi[1], "#C50F1F");
        assert_eq!(campbell.ansi[15], "#F2F2F2");
        // Several shipped schemes name no selection colour at all.
        assert!(campbell.selection.is_none());
        assert!(BUNDLED_SCHEMES.contains("Solarized Light"));
    }

    /// Relative luminance per WCAG, so "is this scheme light?" is a measurement
    /// rather than a guess from the hex digits.
    fn luminance(hex: &str) -> f64 {
        let hex = hex.trim_start_matches('#');
        let channel = |offset: usize| {
            let byte = u8::from_str_radix(&hex[offset..offset + 2], 16).expect("hex pair");
            let value = f64::from(byte) / 255.0;
            if value <= 0.03928 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
    }

    fn named<'a>(schemes: &'a [TerminalScheme], name: &str) -> &'a TerminalScheme {
        schemes
            .iter()
            .find(|scheme| scheme.name == name)
            .unwrap_or_else(|| panic!("{name} is in the catalogue"))
    }

    /// The curated asset exists because the vendored one ships almost no usable
    /// light schemes. If these ever stopped being light, it would defeat the
    /// whole point of carrying them.
    #[test]
    fn the_curated_schemes_are_light_and_in_the_catalogue() {
        let schemes = catalogue(&[]);

        for name in ["Selenized Light", "Catppuccin Latte"] {
            let scheme = named(&schemes, name);
            assert!(
                luminance(&scheme.background) > 0.5,
                "{name} background {} is not light",
                scheme.background
            );
            assert!(
                luminance(&scheme.foreground) < 0.2,
                "{name} foreground {} is not dark enough to read on it",
                scheme.foreground
            );
        }

        let selenized = named(&schemes, "Selenized Light");
        assert_eq!(selenized.background, "#FBF3DB");
        assert_eq!(selenized.ansi[1], "#D2212D");
        assert_eq!(selenized.ansi[15], "#3A4D53");
        assert_eq!(selenized.cursor.as_deref(), Some("#3A4D53"));
    }

    /// A name the user also defines is theirs, not ours — the same precedence
    /// Windows Terminal gives it.
    #[test]
    fn a_user_scheme_overrides_a_shipped_one_of_the_same_name() {
        let path = scratch("override");
        std::fs::write(
            &path,
            r##"{
                // Windows Terminal writes JSONC, so the reader must take comments.
                "schemes": [
                    {
                        "name": "Campbell",
                        "background": "#101010", "foreground": "#EEEEEE",
                        "cursorColor": "#FFFFFF", "selectionBackground": "#264F78",
                        "black": "#0", "red": "#1", "green": "#2", "yellow": "#3",
                        "blue": "#4", "purple": "#5", "cyan": "#6", "white": "#7",
                        "brightBlack": "#8", "brightRed": "#9", "brightGreen": "#10",
                        "brightYellow": "#11", "brightBlue": "#12", "brightPurple": "#13",
                        "brightCyan": "#14", "brightWhite": "#15",
                    },
                ],
            }"##,
        )
        .expect("write settings");

        let schemes = catalogue(std::slice::from_ref(&path));
        let campbell = schemes
            .iter()
            .find(|scheme| scheme.name == "Campbell")
            .expect("still listed once");
        assert_eq!(campbell.background, "#101010");
        assert_eq!(campbell.selection.as_deref(), Some("#264F78"));
        assert_eq!(
            schemes.iter().filter(|s| s.name == "Campbell").count(),
            1,
            "an override replaces, never appends"
        );
        std::fs::remove_file(path).expect("cleanup");
    }

    /// One half-typed scheme must not cost the user the rest of their file.
    #[test]
    fn a_broken_scheme_is_skipped_and_the_rest_survive() {
        let path = scratch("broken");
        std::fs::write(
            &path,
            r##"{"schemes":[
                {"name":"Half typed","background":"#000000"},
                {"name":"Whole","background":"#000000","foreground":"#FFFFFF",
                 "black":"#0","red":"#1","green":"#2","yellow":"#3","blue":"#4",
                 "purple":"#5","cyan":"#6","white":"#7","brightBlack":"#8",
                 "brightRed":"#9","brightGreen":"#10","brightYellow":"#11",
                 "brightBlue":"#12","brightPurple":"#13","brightCyan":"#14",
                 "brightWhite":"#15"}
            ]}"##,
        )
        .expect("write settings");

        let names: Vec<String> = schemes_in_settings(&path)
            .into_iter()
            .map(|scheme| scheme.name)
            .collect();
        assert_eq!(names, vec!["Whole".to_string()]);
        std::fs::remove_file(path).expect("cleanup");
    }

    /// A build that isn't installed is the normal case, not an error.
    #[test]
    fn a_missing_settings_file_contributes_nothing() {
        let absent = std::env::temp_dir().join("pade-no-such-windows-terminal.json");
        assert!(schemes_in_settings(&absent).is_empty());
        assert_eq!(catalogue(&[absent]).len(), catalogue(&[]).len());
    }
}
