//! JVM build manifests: Gradle and Maven, run through the project's committed wrapper when it has one.

use std::path::Path;

use super::{task, Task};

/// A build tool whose projects usually commit a wrapper script that pins the
/// tool's version - the JVM pair. Running the wrapper is the point of shipping
/// it, so it wins over whatever is on PATH.
pub(super) struct WrappedTool {
    pub(super) windows_script: &'static str,
    pub(super) unix_script: &'static str,
    pub(super) on_path: &'static str,
}

pub(super) const GRADLE: WrappedTool = WrappedTool {
    windows_script: "gradlew.bat",
    unix_script: "gradlew",
    on_path: "gradle",
};

pub(super) const MAVEN: WrappedTool = WrappedTool {
    windows_script: "mvnw.cmd",
    unix_script: "mvnw",
    on_path: "mvn",
};

impl WrappedTool {
    /// What to type in `directory`: the committed wrapper, named with an explicit
    /// relative path because the working directory is not on PATH, else the
    /// tool itself.
    pub(super) fn launcher(&self, directory: &Path) -> String {
        let script = if cfg!(windows) {
            self.windows_script
        } else {
            self.unix_script
        };
        if directory.join(script).is_file() {
            return format!(".{}{script}", std::path::MAIN_SEPARATOR);
        }
        self.on_path.to_string()
    }
}

/// `build.gradle` / `build.gradle.kts` - the lifecycle tasks every Gradle build
/// has, whatever plugins it applies.
pub(super) fn gradle_tasks(path: &Path) -> Vec<Task> {
    tool_tasks(path, &GRADLE, &["build", "test", "clean"])
}

/// `pom.xml` - the Maven lifecycle phases a build always answers to.
pub(super) fn maven_tasks(path: &Path) -> Vec<Task> {
    tool_tasks(path, &MAVEN, &["compile", "test", "package", "clean"])
}

/// The shared shape of both JVM extractors: `<launcher> <verb>`, resolved
/// against the manifest's own directory.
pub(super) fn tool_tasks(path: &Path, tool: &WrappedTool, verbs: &[&str]) -> Vec<Task> {
    let directory = path.parent().unwrap_or(path);
    let launcher = tool.launcher(directory);
    verbs
        .iter()
        .map(|verb| task(verb, format!("{launcher} {verb}")))
        .collect()
}
