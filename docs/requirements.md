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
- R1.1.7 ✅ **Session persistence across an accidental reload** — the window
  persists its pane mapping (sessionStorage) and boot re-attaches whatever the
  backend still hosts (`pty_list` ∩ snapshot), replaying `pty_history` per pane;
  the `w=` window query is kept rewritten to the project actually on screen so a
  reload never routes off a stale spawn intent. Never across an app restart:
  sessionStorage dies with the window, and the backend kills all PTYs on exit.
- R1.1.8 ✅ **Intent-based leave** — a deliberate leave (project switch, back to
  the picker, closing the window) kills the project's agents *gracefully*: it
  waits per session for the idle prompt (`sessionStatus === ready`, the
  output-quiet signal — never child-process counting, which mis-reads persistent
  MCP servers), capped at 30s, so nothing mid-flight is severed; the agent's
  auto-save + `/resume` cover continuity. The window close is intercepted
  (`onCloseRequested`) so the X waits the same way before the window is
  destroyed. An accidental reload records no leave and re-attaches instead.

### 1.2 Change Feed (✅ core, 🚧 depth)
- R1.2.1 Filesystem watcher emits an event per save (`notify`).
- R1.2.2 Each event → a card: filename, path, kind, ± line delta, plain summary.
- R1.2.3 Ignore VCS/build noise (`.git`, `node_modules`, `target`, `dist`, …).
- R1.2.4 ✅ Click a card to expand its real per-file diff (via the git seam) with a
  **Unified / Split** toggle and open-in-editor; cached per path. 🔭 agent intent.
- R1.2.5 ✅ The watcher follows the open workspace: switching projects re-roots
  it (`ChangeFeed` passes the open project's path to `watch_start`, which re-arms
  on that explicit root — never the process's cwd, which can drift from it).
- R1.2.6 ✅ Every diff surface (feed card, Git panel, commit modal) prints each
  code line **in full**: long lines wrap (`DiffView.svelte`), never clip or hide
  behind a horizontal scroll; the diff stays hunks + context, never the whole file.
- R1.2.7 ✅ **Exclude what the project ignores.** Beyond the always-on build/VCS
  noise list, the watcher skips ignored paths via an ignore policy fixed per
  `watch_start`: in a git work tree it defers to git itself (`git check-ignore` —
  nested `.gitignore`, `.git/info/exclude`, global excludes, negations), and with
  no git it falls back to **technology-common ignore directories inferred from the
  root manifests** (`package.json`→`node_modules`…, `Cargo.toml`→`target`, Python /
  Go / JVM / Ruby / PHP / .NET, …). The cheap baseline still pre-filters, so a giant
  dir never shells git; git results are memoized per path and reset when a
  `.gitignore` changes.
- R1.2.8 ✅ **Language logos on cards.** Each change card leads with the file's
  **brand logo** (a local `.svg` in `src/lib/icons`, rendered via `Icon.svelte`)
  instead of a text chip — multi-colour marks (TS/JS/Python…) keep their brand
  fills, single-colour marks and format glyphs take the card's language tone. An
  unrecognised type falls back to the text badge, so every row stays tagged.
- R1.2.9 ✅ **The feed survives panel switches.** Events accumulate in a persistent
  store (`lib/stores/feed`) that owns the single live subscription, so switching the
  side panel away from the feed and back no longer empties it (the panel unmounts on
  every switch; the backend keeps no replay). Cleared on a workspace switch.

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

### 1.6 Usage meter (✅ meter, 🔭 burn-rate)
- R1.6.1 ✅ Adapt to the running agents — the top-bar meter groups quota **per
  running agent**; agents without a reliable source show an honest "unknown".
- R1.6.2 ✅ Show % used and reset time. For Claude: the live 5-hour + weekly
  windows from the vendor's OAuth usage endpoint (the local token, cached, no
  message quota spent), with the subscription-tier label as offline fallback.
  🔭 burn-rate warning.

### 1.6a Auto-handoff (✅)
- R1.6a.1 ✅ Track each session's **context-window** fill — parse the agent CLI's
  own indicator from the PTY stream (`lib/stores/context.svelte.ts`), with a
  conservative byte-estimate fallback.
- R1.6a.2 ✅ Near the limit (≥90%), while the agent is idle and quota allows
  (usage gate), **hand off automatically**: ask it to write `continue-<slug>.md`,
  wait for the watcher to see it, end the session, and launch a successor seeded
  to resume from that doc. Opt-out via `prefs.autoHandoff` (default on).
  🔭 the CLI context parser is heuristic and should be tuned to the real output.
- R1.6a.3 ✅ **Consumed docs are retired** — once the successor finishes its
  first turn (it has certainly read the doc by then), the app deletes the
  `continue-*.md` via the narrow `handoff_doc_delete` seam (bare handoff-doc
  names only), so completed handoffs never litter the project.
- R1.6a.4 ✅ **Usage-limit auto-resume** (`lib/stores/usageResume.svelte.ts`) —
  the PTY sniffer spots the CLI's "limit reached" stop message (never the softer
  "approaching" warning), confirms against the OAuth usage window (a healthy
  window means stale scrollback), and schedules the session to resume when the
  window resets (the API's to-the-second `resets_at` stamp first — the same
  endpoint the usage meter reads — else the message's own "resets 3am" clock,
  else a retry probe): **"continue"** into the same session while
  its context has room, the **auto-handoff flow** when it doesn't. Opt-out via
  `prefs.autoResume` (default on).

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
- R1.9.2a ✅ **Throwaway lifecycle** — when the last session of a temp workspace
  that never earned a name ends (tab closed by hand, or the agent terminated),
  the window returns to the project picker and the folder is deleted. An
  auto-named temp holds real work and keeps the normal last-session behavior
  (respawn / agent picker). Backend: a per-session reaper thread notices a
  self-exited agent (Windows conhost never EOFs the PTY reader on its own) and
  drops the session, which is what fires `pty://exit`.
- R1.9.3 ✅ **Auto-naming** — after first meaningful activity (≈3 distinct files
  changed) a temp workspace gets a short, human-readable name: the installed agent
  CLI one-shot (`claude -p …`, cross-platform) → local heuristic (package/Cargo
  name, README heading, dominant file) as the always-on fallback. The name is
  applied as a **display label**, never a disk rename — the live agent holds the
  workspace as its cwd, which the OS (Windows) locks against rename. Toggle in the
  picker; disabled via `prefs.autoNameTemp`.
- R1.9.4 🔭 Copilot (Windows) as an optional name source via MSAL native token —
  seam in place (`copilot.rs`), not yet wired; see the auto-naming handoff doc.
- R1.9.5 ✅ **Move / rename re-points every linked reference** (`refs.rs`). Moving
  or renaming a workspace rewrites the old→new path everywhere it's remembered:
  the agent memory dir (`~/.claude/projects/<encoded-cwd>`; Codex/Gemini TODO),
  IDE recents **gated on the project's marker dir** (`.idea` → JetBrains recents +
  `.idea/*.xml`; `.vscode` → VS Code; `.vscode`/`.cursor` → Cursor — `storage.json`
  + the `state.vscdb` SQLite recents, via `rusqlite`), and **node_modules** links
  with absolute targets under the old path (pnpm junctions via `mklink /J`).
- R1.9.6 ✅ **Live-agent lock handling on move/rename** — an agent holding the dir
  as cwd locks it (Windows). PADE kills the sessions under it (remembering the
  live ones), does the move/rename, then **resumes** the live ones on the new path
  seeded with `continue`; idle/exited sessions stay closed.
- R1.9.7 ✅ **IntelliJ-style path completion** in the add-root field: while the
  suggestion list shows, exactly one option is always selected — the top by
  default, and across re-filters by the formula
  `next = survivorIndex ≥ 0 ? survivorIndex : min(previousIndex, lastIndex)`
  (a surviving option keeps its selection at its new position; a vanished one
  falls to the nearest remaining position). Enter accepts the selection; Tab
  accepts and drills into sub-folders; a cleared field forgets the selection.
- R1.9.8 ✅ Selecting a root (adding one in Root folders) auto-fills it as the
  Get started card's **Location** (shared by the New and Clone tabs); removing
  that root clears it. The chosen root prints in full — a long path wraps in
  the Location row, never truncates.
- R1.9.9 ✅ **Get started card** — one tabbed card, three ways in, behind an
  ARIA pill tablist (arrow keys switch tabs — `lib/rovingTabs.ts` — and Tab
  moves into the active panel's inputs; a pointer click on a tab focuses the
  panel's first input; panels stay mounted — typed state survives switching —
  and swap as an in-place cross-fade while the body's height glides to the
  open panel's measured size):
  - **New** — root select + project name + optional first prompt → "Create &
    open"; a blank name (or the quiet "…or start a throwaway temp workspace"
    button) falls through to a temp workspace. Replaces the old separate
    temp-workspace card.
  - **Local** — open an existing folder: monospace path input with Browse…
    (Tauri dialog); "Open project" is existence-gated through the debounced
    `workspace_probe_path` check. A folder dragged from Explorer or an IDE
    onto the field fills its path (native Tauri drag-drop events carry the
    absolute paths the web API withholds; the field highlights while a drag
    hovers it).
  - **Clone** — gated on git being installed (`vcs_git_installed`; when
    missing, a warning card offers "Install Git…" and "Re-check"). Repository
    URL + "Clone into" the same root select; the typed URL is probed live
    (`vcs_probe_remote` — a debounced `git ls-remote`) and the folder name
    auto-fills only once the repository answers, until edited. An
    unreachable repository says so under the field. An SSH-style URL with no
    SSH key on disk
    (`vcs_has_ssh_key`) falls back to an HTTPS-credentials panel — the
    credentials are used for that one `git clone` and **never persisted** (the
    saved remote is scrubbed back to the clean URL; error text is sanitized;
    `GIT_TERMINAL_PROMPT=0` prevents hidden prompts).

### 1.10 External tool launchers (✅)
- R1.10.1 **IDE menu** — open the active project in an installed editor (`ide.rs`;
  VS Code + its forks (Cursor, Antigravity, Windsurf, VSCodium), JetBrains
  family, Zed, Sublime…). ✅ Ranked by project kind, and ✅ a user-set
  **editor-rules** engine (project kind → chosen editor + a fallback, persisted
  in prefs) resolved rule → fallback → auto-rank. The fallback is any installed
  general-purpose editor the user selects in the project picker; VS Code is one
  option, not an algorithmic special case. The top-bar selector always has a
  drop-down, whose final action reveals the active project in the file explorer.
- R1.10.1a ✅ **Hybrid-aware ranking** (research-backed: Linguist / Nixpacks /
  Buildpacks / JetBrains compare-matrix). Ecosystem manifests are probed in the
  root and one level down (`src-tauri/Cargo.toml` counts); markerless web roots
  are also recognized from `index.html`, or a browser `manifest.json` containing
  `manifest_version`. A bounded, per-file **source census** excludes generated,
  vendored, and build output. Each manifest owns its nearest source files and
  expands its required editor coverage to every language co-located in its main
  source branch, so a web/Rust application requires a generalist while Python
  automation under a separate scripts branch does not veto WebStorm. With no
  manifests, every observed source kind is required; if there is no recognized
  source kind (for example, a text/Markdown-only folder), only general-purpose
  editors are eligible and the configured fallback leads. The same ownership
  rule applies across all registered ecosystems; it has no framework-name
  exceptions.
- R1.10.2 ✅ **Design menu** — an AI design/UI-generation tool as a design-to-code
  companion (`design.rs`; Claude, Google Stitch, Vercel v0, Figma Make). Roster
  **ranked for the active agent** (the vendor-matched tool is pinned first);
  one registry entry per product (DRY). Opens in a reused **companion PADE
  window** (a native Tauri webview — all four sites block iframes via
  `X-Frame-Options`, so in-page embedding is impossible). 🔭 a docked
  side-by-side panel form.
- R1.10.3 ✅ **Agent usage meter** — shipped as the top-bar meter; see 1.6.
- R1.10.4 ✅ **Task-runner dock** — runnable tasks parsed from manifests
  (`package.json` scripts, Cargo/Make/pyproject) launch as tracked **runners**
  (`runner.rs`, `std::process`) that stream their output live into a bottom dock
  (not a throwaway tab), with stop and **pipe-output-into-an-agent** (via the PTY).
  Auto-synced with the files; monorepo-aware (multiple manifests).
- R1.10.5 ✅ **Discord Rich Presence** — report PADE on the user's Discord profile
  as **"Playing PADE"** (`discord.rs`, pure-`std` IPC over Discord's local socket —
  no crate), with an **opt-in toggle** and an **option to show the open project's
  name** (both in the picker's settings). Best-effort: with Discord closed the
  backend fails quietly and the UI never sees it. The connection is reused and the
  run's start timestamp held steady so the profile's elapsed timer doesn't reset on
  a project switch. Displaying requires a registered Discord `APPLICATION_ID` (a
  documented constant) and, for the icon, a `pade` art asset. 🔭 per-agent detail
  line, idle/away states.

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
  lib/stores/*               vcs/          (git review + restore + clone)
  panels/*.svelte            ide.rs        (editor rules)
  theme.css                  usage.rs      (subscription)
                             lib.rs        (wiring only)
```
All backend access goes through `lib/bridge.ts`, zod-validated at the boundary
(DRY). **User input** is validated the same way at entry via `lib/validate.ts`.
Cross-component state lives in `lib/stores/*`. Each panel is one concern (SoC);
panels lazy-load (tree-shaking). Internal modules import via the `@/` alias.
