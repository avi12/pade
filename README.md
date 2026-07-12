# PADE — Power ADE (Agentic Development Environment)

An open-source, comprehension-first GUI that wraps your agent CLI (Claude Code,
Codex, Antigravity CLI, …) running **unmodified in an integrated terminal**, and
builds an environment around it. No token metering — it uses your existing
subscription, and stays ToS-clean by only reading the terminal's output and
typing into it (the same posture as the official editor integrations).

## Why

Coding 100% through an agent makes you fast but **blind to the codebase**. ADE's
flagship surface is the **Change Feed**: every edit becomes a readable card —
what changed, and how much — so you stay the owner while the agent writes.

## MVP status

This is step 1–2 of the roadmap:

- ✅ Native window (Tauri 2 + Rust) with a real terminal (xterm.js + PTY)
- ✅ Change Feed — filesystem watcher turns each save into a diff card
- ✅ Open the project in your editor — GUI editors (VS Code, JetBrains, Zed…) or
  console editors (Neovim, Vim, Helix) in a terminal tab; add any editor by path
- ⏳ Highlight → agent bridge, agent tree (next)

## Stack

| Layer      | Choice                                  |
| ---------- | --------------------------------------- |
| Shell/core | Tauri 2 + Rust                          |
| Terminal   | xterm.js + WebGL, `portable-pty`        |
| Watcher    | `notify`                                |
| Frontend   | Svelte 5 + Vite                         |
| Theme      | Material 3 Expressive (light/dark, OS-follow) |

## Prerequisites

- **Node** ≥ 18 and **pnpm** (`corepack enable` picks up the version pinned in
  `package.json`)
- **Rust** (stable) via [rustup](https://rustup.rs)
- Platform WebView + build tools per the
  [Tauri prerequisites](https://tauri.app/start/prerequisites/) — Windows:
  WebView2 (preinstalled on Windows 11); Linux: `webkit2gtk` +
  `libayatana-appindicator`; macOS: Xcode command-line tools.

## Run (development)

```bash
pnpm install
pnpm app          # tauri dev — compiles the Rust core and opens the native window
```

The terminal launches your platform shell by default. To launch an agent CLI
directly, set its command:

```bash
# macOS/Linux
ADE_AGENT_CMD=claude pnpm app
# Windows PowerShell
$env:ADE_AGENT_CMD="claude"; pnpm app
```

Then edit any file in the repo and watch it appear in the Change Feed.

## Build (release)

```bash
pnpm app:build                 # tauri build — every installer for the host OS
pnpm tauri build --bundles nsis   # …or a single installer type
```

Artifacts land under `src-tauri/target/release/`:

- the app binary — `pade.exe` (Windows) / `pade` (macOS, Linux)
- installers under `bundle/` — e.g. `bundle/nsis/PADE_<version>_x64-setup.exe`

Tauri builds for the host platform only, so run the build on each OS you target.
Before cutting a release, run the quality gates: `pnpm lint` and `pnpm test`.

## Architecture

New to the codebase? [`ARCHITECTURE.md`](ARCHITECTURE.md) is a one-read
orientation — how the two layers (Svelte webview + Rust core) talk over the
single `bridge.ts` IPC door, the screen phases, and a module-by-module map, with
diagrams. Product spec lives in [`docs/requirements.md`](docs/requirements.md);
engineering rules in [`CLAUDE.md`](CLAUDE.md).

## License

Licensed under the [Apache License 2.0](LICENSE).
