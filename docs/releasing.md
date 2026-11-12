# Releasing PADE

PADE ships **only as a signed MSIX package**. Every GitHub release attaches
exactly four files:

- `PADE-<version>.msix`, and the same file as `PADE.msix` (the stable name the
  update feed points at);
- the public certificate `PADE.cer` that trusts it;
- the update feed `PADE.appinstaller`.

Windows installs the package per user, keeps it up to date and removes it
cleanly (see "Installing the MSIX" below). There is no portable exe, no MSI
installer and no winget package; the exe is still built, because the MSIX packs
it.

PADE keeps its settings in `%APPDATA%\pade` and its WebView2 data in
`%LOCALAPPDATA%`. The MSIX carries the Windows 11 context-menu handler as a file
its manifest declares; an unpackaged exe (`pnpm app:build`, a development run)
carries it inside itself and writes it out beside itself when "Open in PADE" is
turned on. WebView2 ships with Windows 11.

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
process has package identity) leaves both to Windows: the release notice never
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
already holds (build it as in AGENTS.md first), with no version bump, git
or GitHub step:

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
3. builds the context-menu handler, then PADE (`tauri build --no-bundle`),
   embedding the handler;
4. lays out the MSIX folder (`pade.exe`, `contextmenu_handler.dll`, the
   logos, the filled manifest), packs it with `makeappx pack`, signs it with
   `signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256`,
   exports `PADE.cer` and fills `PADE.appinstaller`;
5. commits `chore(release): <version>`, tags `v<version>` and pushes both;
6. creates the GitHub release with `PADE-<version>.msix`, `PADE.msix`,
   `PADE.cer` and `PADE.appinstaller`, appending a link to every commit since
   the previous tag;
7. writes `docs/updates.json` (the version and the release page) and pushes it.

The manifest goes last: an unpackaged copy sees a release only once it is on
`main`, so it never points at files that are not uploaded yet.

A patch release (`0.0.x`) is the default; a minor bump marks a release with a
new feature users would notice.

## How installed copies update

A packaged PADE updates through Windows: the `PADE.appinstaller` feed it was
installed from asks Windows for a newer package each time PADE starts, and PADE
itself never checks (`src-tauri/src/platform/package.rs` is the one answer).

A copy that is **not** packaged - an exe or MSI PADE shipped before it became
MSIX-only, or a `pnpm app:build` exe - cannot update itself any more. A
published one still polls `docs/updates.json` from `main`
(raw.githubusercontent.com) at start and every six hours
(`src-tauri/src/update.rs`), only to compare versions. When the manifest names a
newer one, Config → Updates and a toast say "PADE now ships as an MSIX package -
install it from the latest release", and **Open release page** opens that
release in the browser. Nothing is downloaded, swapped or elevated.

- A development build (`pnpm app`) or a release build run from `src-tauri\target`
  never checks, so never nags.
- **Check for updates** in Config turns the check off, and with it every
  connection it makes.
- The manifest holds only `version` and `releaseUrl`. Copies from before the
  MSIX-only release read different fields and so see nothing from it: they
  have to be moved to the MSIX by hand once.

To try the notice without a release, serve a manifest locally and point
`PADE_UPDATE_FEED` at it (its `releaseUrl` must be https) before starting a
published exe.
