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
- R1.1.5 ✅ **Split panes** — show multiple agent sessions side by side (add an
  existing session or launch a new instance into the split; remove a pane). All
  sessions stay mounted so scrollback survives; the grid refits on layout change.
- R1.1.6 ✅ Per-tab **status dot** — starting / working (pulses) / ready (halo) /
  exited, shared from each terminal's idle detection via a `lib/stores` store.

### 1.2 Change Feed (✅ core, 🚧 depth)
- R1.2.1 Filesystem watcher emits an event per save (`notify`).
- R1.2.2 Each event → a card: filename, path, kind, ± line delta, plain summary.
- R1.2.3 Ignore VCS/build noise (`.git`, `node_modules`, `target`, `dist`, …).
- R1.2.4 ✅ Click a card to expand its real per-file diff (via the git seam) with a
  **Unified / Split** toggle and open-in-editor; cached per path. 🔭 agent intent.

### 1.3 Highlight → Agent bridge (✅)
- R1.3.1 Selecting text in a side panel offers "Send to agent".
- R1.3.2 The selection is injected into the terminal's input.

### 1.4 Version-control review panel (✅ core, 🚧 verbs)
- R1.4.1 Behind a git seam (MVP: `git` CLI; `git2`/`gix`/jj later 🔭).
- R1.4.2 ✅ Working-tree status grouped: unstaged = "unreviewed", staged separate.
- R1.4.3 ✅ Recent commits (log) with author, message, relative time.
- R1.4.4 ✅ Per-file colorized diff view (shared parser `lib/diff.ts`; wash tokens).
- R1.4.5 ⏳ Review verbs on agent commits: approve / send-back / explain.
- R1.4.6 ⏳ Manual commit path (agent-oriented by default, not exclusive).
- R1.4.7 ✅ **Restore a version** — a plain-language description ranks prior commits
  (fuzzy token overlap + time hints) and checks the chosen one out **non-
  destructively** on a `pade/restore-<sha>` branch (never a hard reset; dirty-tree
  errors surface). 🔭 `git bisect` oracle plugs into the same `run_git` seam.

### 1.5 Agent tree (⏳)
- R1.5.1 Show the live spawn hierarchy: root session + subagents/background tasks.
- R1.5.2 Per-node status (running/waiting/done/merged/error); blocked nodes surface.
- R1.5.3 🔭 Interrupt/redirect individual nodes.

### 1.6 Usage panel (⏳)
- R1.6.1 Adapt to the active agent; parse its own usage output.
- R1.6.2 Show % used, reset time; 🔭 burn-rate warning.

### 1.7 Config respect (✅ read, 🚧 edit)
- R1.7.1 ✅ Read/surface `CLAUDE.md`, `AGENTS.md`, `.claude/settings*`, `.mcp.json`.
- R1.7.2 ⏳ Edits round-trip through the same files (no shadow store).

### 1.8 Knowledge bank (🔭 for MVP, architected-for)
- Shared, git-backed, two-way (agent writes research; user has full CRUD).

### 1.9 Workspaces & projects (✅ core)
- R1.9.1 Launch modes: open the cwd when it's a project, else a throwaway **temp
  workspace** (default) or the project picker (opt-in `startMode`).
- R1.9.2 Temp workspaces live under the config dir; ADE-owned, so they can be
  deleted, moved (→ permanent), or renamed (→ promoted into the primary root).
- R1.9.3 ✅ **Auto-naming** — after first meaningful activity (≈3 distinct files
  changed) a temp workspace gets a short, human-readable name: the installed agent
  CLI one-shot (`claude -p …`, cross-platform) → local heuristic (package/Cargo
  name, README heading, dominant file) as the always-on fallback. The name is
  applied as a **display label**, never a disk rename — the live agent holds the
  workspace as its cwd, which the OS (Windows) locks against rename. Toggle in the
  picker; disabled via `prefs.autoNameTemp`.
- R1.9.4 🔭 Copilot (Windows) as an optional name source via MSAL native token —
  seam in place (`copilot.rs`), not yet wired; see the auto-naming handoff doc.

### 1.10 External tool launchers (✅)
- R1.10.1 **IDE menu** — open the active project in an installed editor (`ide.rs`;
  VS Code, JetBrains family, Zed, Sublime…). ✅ Ranked by project kind, and ✅ a
  user-set **editor-rules** engine (project kind → chosen editor + a fallback,
  persisted in prefs) resolved rule → fallback → auto-rank.
- R1.10.2 **Design menu** — an AI design/UI-generation tool as a design-to-code
  companion (`design.rs`; Claude, Google Stitch, Vercel v0, Figma Make). Roster
  **ranked for the active agent** (the vendor-matched tool is pinned first);
  one registry entry per product (DRY). 🚧 Currently opens in the browser; a
  **docked native webview panel** (in-app, side-by-side) is the planned form —
  iframes are blocked by all four sites (`X-Frame-Options`), so it needs a Tauri
  webview.
- R1.10.3 🔭 **Agent usage meter** — show the selected agent's remaining quota
  (e.g. weekly), parsed from the vendor's site (not by invoking the CLI, which
  would consume quota).
- R1.10.4 ✅ **Task-runner dock** — runnable tasks parsed from manifests
  (`package.json` scripts, Cargo/Make/pyproject) launch as tracked **runners**
  (`runner.rs`, `std::process`) that stream their output live into a bottom dock
  (not a throwaway tab), with stop and **pipe-output-into-an-agent** (via the PTY).
  Auto-synced with the files; monorepo-aware (multiple manifests).

## 2. Non-functional requirements

- R2.1 **Performance** — native core (Rust); web build reuses logic as WASM,
  renders via WebGPU. Small binary (`opt-level=s`, LTO, strip).
- R2.2 **Theming** — Material 3 Expressive, light/dark graded, follows OS. Tokens
  only; alternate skins (🔭) swap the token set.
- R2.3 **Fonts** — JetBrains Mono (code/terminal), **Figtree** (expressive UI sans),
  both **self-hosted** `woff2` (no runtime CDN); configurable.
- R2.4 **UI** — simple by default (terminal + feed); other panels summon-on-demand;
  🔭 multi-monitor tear-off (native windows).
- R2.5 **i18n** — 🔭 full Unicode + RTL (Hebrew/Arabic bidi).
- R2.6 **Deployment** — one codebase → desktop (Tauri) and 🔭 web (headless core).
- R2.7 **Open source** — Apache-2.0; TS plugin SDK (🔭); public RFCs.

## 3. Architecture (SoC map)

```
Frontend (Svelte)          Rust core (Tauri)
  lib/bridge.ts    ──IPC──▶  pty.rs        (terminal)
  lib/validate.ts            runner.rs     (task-runner dock)
  lib/diff.ts                watcher.rs    (change feed)
  lib/stores/*               vcs.rs        (git review + restore)
  panels/*.svelte            ide.rs        (editor rules)
  theme.css                  usage.rs      (subscription)
                             lib.rs        (wiring only)
```
All backend access goes through `lib/bridge.ts`, zod-validated at the boundary
(DRY). **User input** is validated the same way at entry via `lib/validate.ts`.
Cross-component state lives in `lib/stores/*`. Each panel is one concern (SoC);
panels lazy-load (tree-shaking). Internal modules import via the `@/` alias.
