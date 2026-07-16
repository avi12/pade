# Handoff — fully-printed diff snippets + Notepad++-driven optimization pass

Two asks from Avi (2026-07-16), for the next session.

## 1. Fully-printed code snippets in every diff view

**Avi's words:** "Whenever viewing, from the change feed or from the commit
history, each code snippet must be fully printed (in the diffs, only show the
diffs and surround code)."

**Reading (verify with the running app before building):** wherever a diff is
shown — the Change Feed's expanded card, the VCS panel's selected file, the
commit modal's file pane — every code line must be visible **in full**: no
horizontal clipping, no cut-off line tails. The diff itself should stay a
diff: the changed hunks plus their surrounding context lines, never the whole
file.

**Where today's rendering falls short of that:**

- All three surfaces now render through the one shared renderer,
  `src/lib/DiffView.svelte` (extracted 2026-07-16) — fix it once there.
- The **split view's cells clip**: `.cell { overflow: hidden; white-space:
  pre }` — a long line's tail is silently cut off. That is the clearest
  violation of "fully printed".
- The **unified view** uses `white-space: pre` with the caller's
  `overflow: auto` scroll container — long lines survive but only via
  horizontal scrolling. Decide with the visual result in hand whether
  "fully printed" means *wrap* (`pre-wrap`, everything visible at once) or
  *scroll* is acceptable; the VCS panel used `pre-wrap` before the DiffView
  unification, so Avi has seen and lived with wrapped diffs there.
- **Context lines:** `git diff` already emits hunks with 3 context lines and
  the parser (`src/lib/diff.ts`) keeps them. If "surround code" means *more*
  context, the seam is the diff commands in `src-tauri/src/vcs/`
  (`vcs_diff`, `vcs_commit_diff`) — e.g. `-U<n>`; plumb a single shared
  constant, don't scatter it.

**Acceptance sketch:** open a file with a very long changed line from (a) a
Change Feed card, (b) the Git panel, (c) the commit modal — the whole line is
readable in all three, unified and split, without guessing at a cut-off tail.
Verify live over CDP (see `pade-webview-cdp-reach` memory) and screenshot.

## 2. Optimization pass using the Notepad++ repo

**Avi's words:** "look into potential optimizations via notepadplusplus repo."

**Reading (confirm the intent before deep work):** use
`github.com/notepad-plus-plus/notepad-plus-plus` — a large, mature repo
(thousands of files, C++ sources with long lines, decades of history) — as a
**stress-test workload** for PADE, and fix what it exposes. Candidate
hotspots, in likely order of pain:

- `workspace_scan` / picker over a huge tree; `vcs_status`/`vcs_log` on a
  deep history; the commit modal's per-file diff on large C++ files.
- `DiffView` rendering thousands of diff lines with per-line `<ColorText>`
  spans — the first place virtualization might be *measured* as needed.
- The Change Feed watcher during a branch switch (thousands of events at
  once; the feed caps at `CAP` events — check the debounce path).
- `highlight.ts` tokenizing very long single lines.

**Ground rule from the laws pass (still binding):** Premature Optimization —
profile first, against the Notepad++ clone, and only optimize what the
measurement indicts. Record numbers (before/after) in the eventual commits.

An alternative reading — mining Notepad++'s own source for optimization
*techniques* — seems less likely but cheap to ask Avi about if the stress-test
reading finds nothing worth fixing.

## State at handoff

- HEAD `7418cd9` on `main`, clean, all green (179 JS / 103 Rust tests,
  `pnpm test:e2e` smoke passing, clippy::pedantic, svelte-check, linters).
- The engineering-laws pass just landed (see
  `docs/handoff-apply-engineering-laws.md` for what changed); DiffView,
  the shared popover shell, and the pty.rs chunk-boundary fixes are the
  freshest code.
- House rules apply: digestible conventional commits, verify before each
  commit, ARCHITECTURE.md stays in sync, no obfuscated literal values
  (compose from named parts).
