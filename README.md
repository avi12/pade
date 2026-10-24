# PADE - Power ADE (Agentic Development Environment)

An open-source, comprehension-first GUI that wraps your agent CLI (Claude Code,
Codex, Copilot CLI, Grok CLI, Muse Code, Qwen Code, …) running **unmodified in an integrated terminal**, and
builds an environment around it. No token metering - it uses your existing
subscription, and stays ToS-clean by only reading the terminal's output and
typing into it (the same posture as the official editor integrations).

<p align="center">
  <img alt="PADE: an agent in the terminal beside the Change Feed" src="https://github.com/user-attachments/assets/d96f8f9a-d19d-485f-80f3-77c4b24a7e2e" width="900">
</p>

## Why

Coding 100% through an agent makes you fast but **blind to the codebase**. ADE's
flagship surface is the **Change Feed**: every edit becomes a readable card -
what changed, and how much - so you stay the owner while the agent writes.

## Features

### Agents dashboard: every project at a glance

<img alt="PADE: the Agents dashboard across every open project" src="https://github.com/user-attachments/assets/6ab95559-0c39-4b39-a9b0-ebfc51862b77" width="800">

### Project switcher

<img alt="PADE: the project switcher" src="https://github.com/user-attachments/assets/980c68d5-af97-49f0-9859-23f855aaeb80" width="800">

### Usage meter with reset times

<img alt="PADE: the usage meter" src="https://github.com/user-attachments/assets/77b9d74c-0f88-4093-81b6-96d75379ba5b" width="800">

### Live model and effort switching, usage pacing

<img alt="PADE: the agent config panel and model picker" src="https://github.com/user-attachments/assets/07b64fbd-8acd-4c4d-840b-9d8de1e17a8c" width="800">

### Project tasks, one click to run

<img alt="PADE: the Tasks panel" src="https://github.com/user-attachments/assets/c45773cc-4227-418a-8804-855b5ec9e0bc" width="800">

## MVP status

- ✅ Native window (Tauri 2 + Rust) with a real terminal (xterm.js + PTY), split panes and session tabs
- ✅ Change Feed - filesystem watcher turns each save into a diff card
- ✅ Highlight → agent bridge - send a selection or a file to the agent
- ✅ Git review panel, usage meter with pacing, auto-handoff, Tasks and Config panels
- ✅ Open the project in your editor - GUI editors (VS Code, JetBrains, Zed…) or
  console editors (Neovim, Vim, Helix) in a terminal tab; add any editor by path
- ⏳ Agent tree, review verbs on agent commits (next)

Full status per requirement: [`docs/requirements.md`](docs/requirements.md).

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
  [Tauri prerequisites](https://tauri.app/start/prerequisites/) - Windows:
  WebView2 (preinstalled on Windows 11); Linux: `webkit2gtk` +
  `libayatana-appindicator`; macOS: Xcode command-line tools.

## Run (development)

```bash
pnpm install
pnpm app          # tauri dev - compiles the Rust core and opens the native window
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

PADE ships as one portable exe - no installer. Download `PADE-<version>.exe`
from the [releases](https://github.com/avi12/pade/releases) and run it from
anywhere, or install it with `winget install avi12.PADE`. Either way it keeps
itself up to date (Config → Updates).

```bash
pnpm app:build                 # tauri build --no-bundle → src-tauri/target/release/pade.exe
pnpm release <version> <notes> # build, tag, publish the GitHub release and update manifest
```

Tauri builds for the host platform only, so run the build on each OS you target.
Before cutting a release, run the quality gates: `pnpm lint` and `pnpm test`.
[`docs/releasing.md`](docs/releasing.md) has the release steps and how updates work.

## Architecture

New to the codebase? [`ARCHITECTURE.md`](ARCHITECTURE.md) is a one-read
orientation - how the two layers (Svelte webview + Rust core) talk over the
single `lib/ipc` messaging door, the screen phases, and a module-by-module map, with
diagrams. Product spec lives in [`docs/requirements.md`](docs/requirements.md);
engineering rules in [`CLAUDE.md`](CLAUDE.md).

## License

Licensed under the [Apache License 2.0](LICENSE).
