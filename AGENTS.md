# Agent Instructions

Before working in this repository, read and follow `CLAUDE.md` in the repository
root. `CLAUDE.md` is the single authoritative source for project instructions;
do not duplicate those rules here.

## Building

Stack: Tauri 2 + Rust core, Svelte 5 + Vite frontend. Windows is the shipping
platform (one portable exe, no installer); Tauri builds for the host OS only.

### Prerequisites

- Node 18 or newer, with pnpm via corepack. The pnpm version is pinned in
  `package.json` (`packageManager`), currently pnpm 11.1.2.
- Rust stable via rustup.
- Platform prerequisites per Tauri: on Windows, WebView2 (preinstalled on
  Windows 11) and the MSVC build tools.

If `pnpm` is not recognized in your shell, fix that before building anything:
Tauri's `beforeBuildCommand` spawns `pnpm build` as a child process, so
invoking through `corepack pnpm tauri build` is not enough - `pnpm` itself
must resolve on `PATH` in the same shell. `corepack enable` installs the
shims next to Node. Verify with `pnpm --version` (expect 11.1.2).

### Install and develop

```powershell
pnpm install
pnpm app          # tauri dev: Rust core + native window, frontend on :1430
pnpm dev          # Vite only, no native window
```

### Build the frontend only

```powershell
pnpm build        # vite build -> dist/
```

### Build for release (local, publishes nothing)

A plain `pnpm app:build` produces an exe WITHOUT the embedded Windows 11
context-menu handler: `src-tauri/build.rs` embeds the handler only when
`PADE_CONTEXT_MENU_HANDLER` points at the built DLL. To get the exe that
actually ships, replicate the build steps from `scripts/release.mjs`:

```powershell
cargo build --release --manifest-path src-tauri/Cargo.toml -p contextmenu-handler
$env:PADE_CONTEXT_MENU_HANDLER = (Resolve-Path 'src-tauri/target/release/contextmenu_handler.dll').Path
pnpm tauri build --no-bundle
```

Output: `src-tauri/target/release/pade.exe`. Verify it before reporting done:

```powershell
$exe = Get-Item 'src-tauri/target/release/pade.exe'
$exe.VersionInfo.FileVersion   # matches `version` in src-tauri/Cargo.toml
(Get-FileHash $exe.FullName -Algorithm SHA256).Hash
```

The version lives in exactly one place: `version` in `src-tauri/Cargo.toml`.
The exe and the Config panel read it from there. Do not bump it for a local
build, and expect a local build to carry whatever uncommitted changes are in
the working tree.

### Quality gates

Run these before calling work done (see `package.json` for the scripts):

```powershell
pnpm check        # svelte-check type gate
pnpm lint         # oxlint + eslint, stylelint, cargo clippy -D warnings + fmt, fallow audit
pnpm test         # vitest + cargo test
```

Note: `pnpm lint:rust` runs `cargo fmt`, which rewrites files. Check
`git status` afterwards and do not keep formatting churn you did not intend.

### Publishing a release (only when the user explicitly asks to publish)

`pnpm release <major.minor.patch> <notes-file>` is NOT a build command. It
bumps the version, builds as above, commits `chore(release): <version>`, tags,
pushes, creates the GitHub release with `PADE-<version>.exe`, pushes the
update manifest `docs/updates.json`, and submits a winget PR (pass
`--no-winget` after the notes file to skip winget). It refuses to run on a
dirty tracked tree or with a version that is not newer than the current one,
and it needs `gh` authenticated. Full details: `docs/releasing.md`.

When the user says "build for release", do the local build above and stop.
Commit, push, tag, publish, and bump the version only on an explicit
instruction to release or ship.
