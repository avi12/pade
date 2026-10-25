# Releasing PADE

PADE ships two ways, both attached to every GitHub release:

- **a portable exe**, `PADE-<version>.exe`, that runs from wherever it is saved;
- **an MSI installer**, `PADE-<version>-setup.msi`, that installs PADE under
  Program Files. Its Shortcuts page (added to Tauri's WiX template in
  `src-tauri/windows/installer.wxs`) has two checkboxes, both on by default:
  a desktop shortcut and a Start menu shortcut. A silent install turns one off
  with `DESKTOPSHORTCUT=""` or `STARTMENUSHORTCUT=""`.

Either way PADE keeps its settings in `%APPDATA%\pade` and its WebView2 data in
`%LOCALAPPDATA%`, and carries the Windows 11 context-menu handler inside itself
(written out beside the exe when "Open in PADE" is turned on). WebView2 ships
with Windows 11.

## Cutting a release

Every push to `main` is released (see `CLAUDE.md`). With the work committed:

1. Write the notes to a file outside the repository: the changelog
   **highlights**, in plain sentences for users, no file names.
2. Run

   ```powershell
   pnpm release <major.minor.patch> <notes-file>
   ```

`scripts/release.mjs` then:

1. sets the version in `src-tauri/Cargo.toml` - its one home; the exe and the
   Config panel read it from there;
2. builds the context-menu handler, then PADE and its MSI
   (`tauri build --bundles msi`), embedding the handler. It links the MSI
   itself with WiX's `light -sval`, skipping the ICE checks: they run on
   VBScript, which Windows no longer installs by default, so the `Installer`
   GitHub Actions workflow runs them whenever the installer's sources change;
3. commits `chore(release): <version>`, tags `v<version>` and pushes both;
4. creates the GitHub release with the exe and the MSI, appending a link to
   every commit since the previous tag;
5. writes `docs/updates.json` from both files' SHA-256 (the exe at the top
   level, the MSI under `installer`) and pushes it.

The manifest goes last: installed copies see a release only once it is on
`main`, so it never points at a file that is not uploaded yet.

6. fills the winget manifests in `packaging/winget` (version, date, exe address
   and SHA-256), runs `winget validate` on them, and opens the pull request to
   `microsoft/winget-pkgs` with `wingetcreate submit` (titled "New package" the
   first time, "New version" after), using `gh auth token`. winget's reviewers
   merge it, usually within a day or two. Pass `--no-winget` after the notes
   file to skip this step.

A patch release (`0.0.x`) is the default; a minor bump marks a release with a
new feature users would notice.

## How installed copies update

`src-tauri/src/update.rs` reads `docs/updates.json` from `main`
(raw.githubusercontent.com) at start and every six hours, the way a browser
extension polls its update URL. When it names a newer version, the exe is
downloaded to `%LOCALAPPDATA%\pade\updates` and checked against the manifest's
SHA-256, and Config → Updates offers **Restart to update**. Otherwise the swap
happens as PADE closes, so the next start is the new version.

The swap renames the running exe aside (a running exe can be renamed, not
overwritten), puts the new one under its name, and the next start deletes what
was set aside.

A copy the MSI installed (the folder it records under
`HKLM\Software\avi12\PADE`) can't swap its own exe under Program Files, so it
downloads the release's MSI the same checked way, and a hidden PowerShell waits
for PADE to close and runs it with `msiexec /passive`, which asks for elevation.
It passes along which shortcuts exist, so an update never brings back one the
user removed. **Restart to update** closes PADE, and the installer starts it
again.

A copy installed with `winget install avi12.PADE` (it lives under
`…\WinGet\Packages\`) updates through winget instead, so `winget list` stays
true: it reads which versions `microsoft/winget-pkgs` holds (the GitHub API, at
the same times), and when one is newer, a hidden PowerShell waits for PADE to
close and runs `winget upgrade --id avi12.PADE`. **Restart to update** closes
PADE, and the upgrade starts it again. A winget copy sees a release once its
winget pull request is merged, not when the GitHub release is published.

- Only a published exe updates itself: a debug build, or a release build run from
  `src-tauri\target`, never does.
- **Check for updates** in Config turns the check off, and with it every
  connection the updater makes.
- 0.0.1 shipped as an installer without the updater; replace it by hand once.

To try an update without a release, serve a manifest locally and point
`PADE_UPDATE_FEED` at it (a `http://127.0.0.1` or `http://localhost` address is
accepted) before starting a published exe.
