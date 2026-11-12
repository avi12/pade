//! .NET and `MSBuild` manifests: solutions, `.csproj` family and `.vcxproj` projects.

use std::path::Path;

use super::{quoted_file_name, task, Task};

/// A solution names its projects instead of holding them, so both the .NET and
/// the MSVC family can claim one; each extractor reads it to decide. `.slnx` is
/// the newer XML form of the same file.
pub(super) const SOLUTION_EXTENSION: &str = ".sln";

pub(super) const XML_SOLUTION_EXTENSION: &str = ".slnx";

pub(super) const SOLUTION_EXTENSIONS: &[&str] = &[SOLUTION_EXTENSION, XML_SOLUTION_EXTENSION];

/// Native C++ projects build through `MSBuild` - `dotnet` cannot build one.
pub(super) const NATIVE_PROJECT_EXTENSION: &str = ".vcxproj";

/// Whether a path is a solution file - the `.sln` a project list lives in, or
/// its newer `.slnx` form.
pub(super) fn is_solution(path: &Path) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };
    let name = name.to_string_lossy().to_ascii_lowercase();
    SOLUTION_EXTENSIONS
        .iter()
        .any(|extension| name.ends_with(extension))
}

/// Whether a solution lists a native C++ project. A `.sln` is just a list of
/// project files, so this is what separates the two families that can claim
/// one: `MSBuild` builds the native solutions, `dotnet` the managed ones.
pub(super) fn references_native_project(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    text.to_ascii_lowercase().contains(NATIVE_PROJECT_EXTENSION)
}

/// A solution or a `.csproj` / `.fsproj` / `.vbproj` - the standard `dotnet`
/// verbs against that one file, in both configurations. A solution has no
/// single entry point to `run` or watch, and a solution of native C++ projects
/// is `MSBuild`'s, not the SDK's.
pub(super) fn dotnet_tasks(path: &Path) -> Vec<Task> {
    let solution = is_solution(path);
    if solution && references_native_project(path) {
        return Vec::new();
    }
    let target = quoted_file_name(path);
    let mut tasks = vec![
        task("build", format!("dotnet build {target}")),
        task(
            "build (Release)",
            format!("dotnet build {target} -c Release"),
        ),
    ];
    if !solution {
        tasks.push(task("run", format!("dotnet run --project {target}")));
        tasks.push(task(
            "watch",
            format!("dotnet watch --non-interactive --project {target} run"),
        ));
    }
    tasks.push(task("test", format!("dotnet test {target}")));
    tasks.push(task(
        "publish",
        format!("dotnet publish {target} -c Release"),
    ));
    tasks.push(task("restore", format!("dotnet restore {target}")));
    tasks.push(task("clean", format!("dotnet clean {target}")));
    for name in declared_targets(path) {
        tasks.push(task(&name, format!("dotnet build {target} -t:{name}")));
    }
    tasks
}

/// The verbs above, under the target names `MSBuild` answers them by - a project
/// that overrides one of these is offering nothing the list does not have.
pub(super) const STANDARD_TARGETS: &[&str] = &[
    "build", "rebuild", "clean", "restore", "publish", "pack", "test", "run", "watch",
];

/// The targets a project declares for itself, which are what a `.csproj` has
/// instead of npm's scripts: `dotnet build <project> -t:<name>` runs one.
///
/// Read out of the manifest itself - the file the panel already watches - and
/// deliberately NOT out of what it imports: the registry decides which files the
/// frontend watches, and a `Directory.Build.targets` beside the project is not
/// one of them, so a target read from there would be listed once and then never
/// refresh when it changed. A project that wants its target on the panel
/// declares it in the project file.
pub(super) fn declared_targets(path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    entry_point_targets(&text)
}

/// The pure scan behind [`declared_targets`]: one project file's own entry
/// points, in the order it declares them. A target is an entry point - something
/// a person would click - unless it is internal (`_` by `MSBuild`'s own
/// convention), a hook that already runs as part of another build
/// (`BeforeTargets` / `AfterTargets`), or a standard verb the group already
/// offers.
pub(super) fn entry_point_targets(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for element in elements(text, "Target") {
        let Some(name) = attribute(&element, "Name") else {
            continue;
        };
        let is_entry_point = !name.starts_with('_')
            && attribute(&element, "BeforeTargets").is_none()
            && attribute(&element, "AfterTargets").is_none()
            && !STANDARD_TARGETS.contains(&name.to_ascii_lowercase().as_str())
            && !names.contains(&name);
        if is_entry_point {
            names.push(name);
        }
    }
    names
}

/// Every named element's attribute text, tag by tag rather than line by line:
/// `MSBuild` lets one element's attributes span several lines. The closing `>` is
/// looked for outside quotes, since XML leaves `>` legal inside an attribute
/// value (a `Condition` comparing versions writes one).
pub(super) fn elements(text: &str, name: &str) -> Vec<String> {
    let opening = format!("<{name}");
    let mut elements = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(&opening) {
        let after = &rest[start + opening.len()..];
        let is_this_element = after.starts_with([' ', '\t', '\r', '\n', '>', '/']);
        let Some(end) = element_end(after) else {
            break;
        };
        if is_this_element {
            elements.push(after[..end].to_string());
        }
        rest = &after[end + 1..];
    }
    elements
}

/// Where an element's attribute text ends: the first `>` that is not inside a
/// quoted attribute value.
pub(super) fn element_end(text: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (index, character) in text.char_indices() {
        let Some(open) = quote else {
            if character == '>' {
                return Some(index);
            }

            if matches!(character, '"' | '\'') {
                quote = Some(character);
            }

            continue;
        };
        if character == open {
            quote = None;
        }
    }
    None
}

/// One attribute's value out of an element's attribute text, matching the whole
/// name so `Name` never reads `BeforeTargets`' value or its own `TargetName`
/// neighbour. Both quote styles are legal XML.
pub(super) fn attribute(element: &str, name: &str) -> Option<String> {
    let mut rest = element;
    while let Some(start) = rest.find(name) {
        let is_whole_name = rest[..start]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace);
        let after = rest[start + name.len()..].trim_start();
        rest = &rest[start + name.len()..];
        if !is_whole_name {
            continue;
        }
        let Some(value) = after.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let Some(quote) = value
            .chars()
            .next()
            .filter(|character| matches!(character, '"' | '\''))
        else {
            continue;
        };
        let value = &value[quote.len_utf8()..];
        let end = value.find(quote)?;
        return Some(value[..end].trim().to_string()).filter(|value| !value.is_empty());
    }
    None
}

/// A `.vcxproj`, or the solution that carries one: native C++ builds through
/// `MSBuild`. `Debug` is the configuration Visual Studio opens with, so it is the
/// one a "build" here means.
pub(super) fn msbuild_tasks(path: &Path) -> Vec<Task> {
    if is_solution(path) && !references_native_project(path) {
        return Vec::new();
    }
    let target = quoted_file_name(path);
    vec![
        task(
            "build",
            format!("msbuild {target} -restore -p:Configuration=Debug"),
        ),
        task(
            "build (Release)",
            format!("msbuild {target} -restore -p:Configuration=Release"),
        ),
        task(
            "rebuild",
            format!("msbuild {target} -restore -t:Rebuild -p:Configuration=Debug"),
        ),
        task("clean", format!("msbuild {target} -t:Clean")),
    ]
}
