# ADE — Software Requirements (MVP)

Status legend: ✅ done · 🚧 in progress · ⏳ planned (MVP) · 🔭 later

## 0. Product thesis

ADE wraps an agent CLI running **unmodified** in an integrated terminal and builds
a comprehension-first GUI around it. It uses the user's existing subscription (no
token metering) and stays ToS-clean by only reading the PTY stream and typing into
it. Core mission: cure "code-blindness" — the user stays the owner while the agent
writes.

## 1. Functional requirements

### 1.1 Terminal (✅)
- R1.1.1 Run an agent CLI in a real PTY (Windows ConPTY via `portable-pty`).
- R1.1.2 GPU-accelerated rendering (xterm.js WebGL).
- R1.1.3 Bi-directional: stream output in, send keystrokes/resize out.
- R1.1.4 Command configurable via `ADE_AGENT_CMD` (default: platform shell).

### 1.2 Change Feed (✅ core, 🚧 depth)
- R1.2.1 Filesystem watcher emits an event per save (`notify`).
- R1.2.2 Each event → a card: filename, path, kind, ± line delta, plain summary.
- R1.2.3 Ignore VCS/build noise (`.git`, `node_modules`, `target`, `dist`, …).
- R1.2.4 🔭 Real per-hunk diffs and agent-authored intent replace the heuristic.

### 1.3 Highlight → Agent bridge (⏳)
- R1.3.1 Selecting text in any panel offers "Send to agent".
- R1.3.2 The selection is injected into the terminal's input.

### 1.4 Version-control review panel (⏳)
- R1.4.1 Behind a `Vcs` abstraction; Git first (`git2`), others later (🔭).
- R1.4.2 Show working-tree status grouped into changelists; agent changes in
  their own "unreviewed" list, separate from the user's.
- R1.4.3 Show recent commits (log) with author, message, stats.
- R1.4.4 Per-file diff view.
- R1.4.5 Review verbs on agent commits: approve / send-back / explain.
- R1.4.6 User can still commit manually (agent-oriented by default, not exclusive).

### 1.5 Agent tree (⏳)
- R1.5.1 Show the live spawn hierarchy: root session + subagents/background tasks.
- R1.5.2 Per-node status (running/waiting/done/merged/error); blocked nodes surface.
- R1.5.3 🔭 Interrupt/redirect individual nodes.

### 1.6 Usage panel (⏳)
- R1.6.1 Adapt to the active agent; parse its own usage output.
- R1.6.2 Show % used, reset time; 🔭 burn-rate warning.

### 1.7 Config respect (⏳)
- R1.7.1 Read/surface `CLAUDE.md`, `AGENTS.md`, `.claude/`, `.mcp.json` as-is.
- R1.7.2 Edits round-trip through the same files (no shadow store).

### 1.8 Knowledge bank (🔭 for MVP, architected-for)
- Shared, git-backed, two-way (agent writes research; user has full CRUD).

## 2. Non-functional requirements

- R2.1 **Performance** — native core (Rust); web build reuses logic as WASM,
  renders via WebGPU. Small binary (`opt-level=s`, LTO, strip).
- R2.2 **Theming** — Material 3 Expressive, light/dark graded, follows OS. Tokens
  only; alternate skins (🔭) swap the token set.
- R2.3 **Fonts** — JetBrains Mono (code/terminal), M3/Google Sans (UI); configurable.
- R2.4 **UI** — simple by default (terminal + feed); other panels summon-on-demand;
  🔭 multi-monitor tear-off (native windows).
- R2.5 **i18n** — 🔭 full Unicode + RTL (Hebrew/Arabic bidi).
- R2.6 **Deployment** — one codebase → desktop (Tauri) and 🔭 web (headless core).
- R2.7 **Open source** — Apache-2.0; TS plugin SDK (🔭); public RFCs.

## 3. Architecture (SoC map)

```
Frontend (Svelte)          Rust core (Tauri)
  lib/bridge.ts    ──IPC──▶  pty.rs        (terminal)
  lib/stores/*               watcher.rs    (change feed)
  panels/*.svelte            vcs.rs        (git review)
  theme.css                  usage.rs      (subscription)
                             lib.rs        (wiring only)
```
All backend access goes through `lib/bridge.ts` (DRY). Each panel is one concern
(SoC). Panels lazy-load (tree-shaking).
