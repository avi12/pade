//! Starting a PTY host process so that nothing which ends the app ends it too.
//!
//! On Windows that takes more than a detached spawn. Under `tauri dev`, the app
//! runs inside cargo's job object, whose kill-on-close limit (no breakaway
//! allowed) ends every process in it the moment cargo is killed for a rebuild;
//! and a tree-kill by parent process id reaches any child of `pade.exe`. So the
//! host is created by the WMI provider host instead (`Win32_Process.Create`):
//! outside every job the app is in, and with no parent the app's tree reaches.
//! A plain detached spawn remains as the fallback when WMI refuses.

use std::io;
use std::path::Path;

use super::PTY_HOST_FLAG;

/// The two values the WMI script reads from its environment, so neither path is
/// ever spliced into script text (no quoting to get wrong).
#[cfg(windows)]
const COMMAND_LINE_VARIABLE: &str = "PADE_PTY_HOST_COMMAND_LINE";
#[cfg(windows)]
const DIRECTORY_VARIABLE: &str = "PADE_PTY_HOST_DIRECTORY";

/// Create the process through WMI, hidden (`ShowWindow = 0` - a debug build is a
/// console program), in the instance directory, and exit with WMI's own code
/// (0 = created).
#[cfg(windows)]
const WMI_CREATE_SCRIPT: &str = "$startup = New-CimInstance -ClassName Win32_ProcessStartup -ClientOnly -Property @{ ShowWindow = [uint16]0 }; \
$created = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $env:PADE_PTY_HOST_COMMAND_LINE; CurrentDirectory = $env:PADE_PTY_HOST_DIRECTORY; ProcessStartupInformation = $startup }; \
exit [int]$created.ReturnValue";

/// Start `executable` in PTY-host mode for `instance_directory`. Returns once
/// the process is created; the caller waits for it to publish its state file.
/// Blocking: it runs a short-lived `PowerShell`.
pub(crate) fn launch_host(executable: &Path, instance_directory: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        match launch_through_wmi(executable, instance_directory) {
            Ok(()) => return Ok(()),
            Err(error) => eprintln!(
                "[pty-host] WMI could not start the host ({error}); starting it detached instead"
            ),
        }
    }
    let arguments = [
        PTY_HOST_FLAG.to_string(),
        instance_directory.to_string_lossy().into_owned(),
    ];
    crate::util::spawn_detached(executable, &arguments).map(drop)
}

#[cfg(windows)]
fn launch_through_wmi(executable: &Path, instance_directory: &Path) -> io::Result<()> {
    let command_line = format!(
        "\"{}\" {PTY_HOST_FLAG} \"{}\"",
        executable.display(),
        instance_directory.display()
    );
    let output = crate::util::command("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            WMI_CREATE_SCRIPT,
        ])
        .env(COMMAND_LINE_VARIABLE, command_line)
        .env(DIRECTORY_VARIABLE, instance_directory)
        .output()?;
    if output.status.success() {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "Win32_Process.Create answered {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}
