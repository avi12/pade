# Releasing PADE

PADE ships three ways, all attached to every GitHub release:

- **a portable exe**, `PADE-<version>.exe`, that runs from wherever it is saved;
- **an MSI installer**, `PADE-<version>-setup.msi`, that installs PADE under
  Program Files. Its Shortcuts page (added to Tauri's WiX template in
  `src-tauri/windows/installer.wxs`) has two checkboxes, both on by default:
  a desktop shortcut and a Start menu shortcut. A silent install turns one off
  with `DESKTOPSHORTCUT=""` or `STARTMENUSHORTCUT=""`;
- **a signed MSIX package**, `PADE-<version>.msix` (and the same file as
  `PADE.msix`), with the public certificate `PADE.cer` that trusts it and the
  update feed `PADE.appinstaller`. Windows installs it per user, keeps it up to
  date and removes it cleanly (see "Installing the MSIX" below).

Every way PADE keeps its settings in `%APPDATA%\pade` and its WebView2 data in
`%LOCALAPPDATA%`. The exe and the MSI carry the Windows 11 context-menu handler
inside the exe (written out beside it when "Open in PADE" is turned on); the
MSIX carries it as a file its manifest declares. WebView2 ships with Windows 11.

## The MSIX package

`packaging/msix/AppxManifest.xml` is the package manifest, a template the
release script fills (identity `avi12.PADE`, the version as
`<major>.<minor>.<patch>.0`, and the publisher, which must equal the signing
certificate's subject `CN=avi12` exactly). It declares:

- one full-trust desktop application, `pade.exe`, with the logos from
  `src-tauri/icons`;
- **file-system and registry write virtualization turned off**
  (`desktop6:FileSystemWriteVirtualization` / `RegistryWriteVirtualization`,
  allowed by the `unvirtualizedResources` capability), so the packaged PADE
  shares `%APPDATA%\pade`, `%LOCALAPPDATA%\pade` (its PTY host copies),
  `~\.claude` and every other agent's config, and `HKCU\Software\PADE` with an
  unpackaged copy instead of writing into a private package store;
- the Explorer "Open in PADE" verb natively: `contextmenu_handler.dll` as a
  packaged COM server and the folder and folder-background verbs, the same
  CLSID as the sparse manifest the unpackaged copy registers at run time.

A packaged PADE (`src-tauri/src/platform/package.rs` asks Windows whether the
process has package identity) leaves both to Windows: the self-updater never
starts, and Config → Updates says updates come through Windows; the context
menu toggle only flips the handler's show/hide flag, since the manifest
already declares the verb. Its install folder is read-only, which nothing else
minds: the PTY host runs from a copy under `%LOCALAPPDATA%\pade\pty-host\bin`
already.

`PADE.appinstaller` points at `releases/latest/download/PADE.msix` and asks
Windows to look for a newer package every time PADE starts
(`OnLaunch HoursBetweenUpdateChecks="0"`), so a copy installed from the feed
follows every later release.

The signing certificate lives in `Cert:\CurrentUser\My`; its thumbprint is
`DEFAULT_SIGNING_THUMBPRINT` in the script, overridden by the
`PADE_SIGNING_THUMBPRINT` environment variable. The script stops before
changing anything when `makeappx`/`signtool` (Windows SDK) or the certificate
is missing, has no private key, has expired, or carries another subject.
Signing timestamps through `http://timestamp.digicert.com`, so a package stays
valid after the certificate expires.

To build and sign the package alone, from what `src-tauri/target/release`
already holds (build it as in AGENTS.md first), with no version bump, git,
GitHub or winget step:

```powershell
node scripts/release.mjs --msix-only <major.minor.patch>
```

It writes `release/PADE-<version>.msix`, `release/PADE.msix`,
`release/PADE.cer` and `release/PADE.appinstaller`.

### Installing the MSIX

The certificate is self-issued, so Windows trusts the package only once its
certificate is trusted on that machine - once, in an elevated PowerShell:

```powershell
Import-Certificate -FilePath .\PADE.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople
```

(or double-click `PADE.cer` → Install Certificate → Local Machine → "Place
all certificates in the following store" → Trusted People). Then open
`PADE.appinstaller` (or `PADE-<version>.msix`) and choose Install, or run
`Add-AppxPackage -AppInstallerFile .\PADE.appinstaller`. Installing through
the `.appinstaller` is what turns on the automatic updates; a bare `.msix`
install is updated by installing the next one.

## Cutting a release

Every push to `main` is released (see `CLAUDE.md`). With the work committed:

1. Write the notes to a file outside the repository: the changelog
   **highlights**, in plain sentences for users, no file names.
2. Run

   ```powershell
   pnpm release <major.minor.patch> <notes-file>
   ```

`scripts/release.mjs` (a thin entry over one module per stage in
`scripts/release-pipeline/`, never a directory named `release/`, which
`.gitignore` excludes) then:

1. checks that `makeappx`, `signtool` and the signing certificate are there,
   before it changes anything;
2. sets the version in `src-tauri/Cargo.toml` - its one home; the exe and the
   Config panel read it from there;
3. builds the context-menu handler, then PADE and its MSI
   (`tauri build --bundles msi`), embedding the handler. It links the MSI
   itself with WiX's `light -sval`, skipping the ICE checks: they run on
   VBScript, which Windows no longer installs by default, so the `Installer`
   GitHub Actions workflow runs them whenever the installer's sources change;
4. lays out the MSIX folder (`pade.exe`, `contextmenu_handler.dll`, the
   logos, the filled manifest), packs it with `makeappx pack`, signs it with
   `signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256`,
   exports `PADE.cer` and fills `PADE.appinstaller`;
5. commits `chore(release): <version>`, tags `v<version>` and pushes both;
6. creates the GitHub release with the exe, the MSI, `PADE-<version>.msix`,
   `PADE.msix`, `PADE.cer` and `PADE.appinstaller`, appending a link to every
   commit since the previous tag;
7. writes `docs/updates.json` from both files' SHA-256 (the exe at the top
   level, the MSI under `installer`) and pushes it.

The manifest goes last: installed copies see a release only once it is on
`main`, so it never points at a file that is not uploaded yet.

8. fills the winget manifests in `packaging/winget` (version, date, exe address
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

An MSIX install never updates itself: Windows does, from the
`PADE.appinstaller` feed it was installed from, each time PADE starts.

- Only a published exe updates itself: a debug build, or a release build run from
  `src-tauri\target`, never does.
- **Check for updates** in Config turns the check off, and with it every
  connection the updater makes.
- 0.0.1 shipped as an installer without the updater; replace it by hand once.

To try an update without a release, serve a manifest locally and point
`PADE_UPDATE_FEED` at it (a `http://127.0.0.1` or `http://localhost` address is
accepted) before starting a published exe.
