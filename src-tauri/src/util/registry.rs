//! The Windows registry reads the live `PATH` and environment expansion need.

use super::process::command;

/// The single home for reading the registry via `reg query` - no registry crate,
/// per the minimize-dependencies rule. `arguments` are the tokens after `query`
/// (a key, optionally `/v <name>`); returns the captured stdout on success, or
/// `None` when the command can't spawn or `reg` reports a missing key. Every
/// registry read (`registry_string`, the context-menu probes) routes here so the
/// spawn and success-check live in exactly one place (DRY).
#[cfg(windows)]
pub(crate) fn registry_query(arguments: &[&str]) -> Option<String> {
    let output = command("reg").arg("query").args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// One string value out of the registry. The value comes back unexpanded, because
/// `Path` is stored as a `REG_EXPAND_SZ` full of `%VAR%` references.
#[cfg(windows)]
pub(crate) fn registry_string(key: &str, name: &str) -> Option<String> {
    let text = registry_query(&[key, "/v", name])?;
    // A hit prints as `    Path    REG_EXPAND_SZ    C:\one;C:\two`.
    text.lines()
        .filter(|line| line.trim_start().starts_with(name))
        .find_map(|line| {
            line.split_once("REG_EXPAND_SZ")
                .or_else(|| line.split_once("REG_SZ"))
        })
        .map(|(_, value)| value.trim().to_string())
}

/// Expand `%VAR%` references against the process environment. An unset variable
/// is left verbatim (the resulting path simply won't exist, and is skipped).
#[cfg(windows)]
pub(super) fn expand_environment_variables(value: &str) -> String {
    let mut expanded = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(open) = rest.find('%') {
        expanded.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('%') else {
            expanded.push('%');
            rest = after;
            continue;
        };
        let name = &after[..close];
        if let Ok(value) = std::env::var(name) {
            expanded.push_str(&value);
        } else {
            expanded.push('%');
            expanded.push_str(name);
            expanded.push('%');
        }
        rest = &after[close + 1..];
    }
    expanded.push_str(rest);
    expanded
}
