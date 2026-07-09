# ADE — Agentic Development Environment

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
- ⏳ Highlight → agent bridge, VCS review panel, agent tree (next)

## Stack

| Layer      | Choice                                  |
| ---------- | --------------------------------------- |
| Shell/core | Tauri 2 + Rust                          |
| Terminal   | xterm.js + WebGL, `portable-pty`        |
| Watcher    | `notify`                                |
| Frontend   | Svelte 5 + Vite                         |
| Theme      | Material 3 Expressive (light/dark, OS-follow) |

## Run it

```bash
npm install
npm run app        # tauri dev — opens the native window
```

The terminal launches your platform shell by default. To launch the agent
directly, set the command:

```bash
# macOS/Linux
ADE_AGENT_CMD=claude npm run app
# Windows PowerShell
$env:ADE_AGENT_CMD="claude"; npm run app
```

Then edit any file in this repo and watch it appear in the Change Feed.

## License

Apache-2.0.
