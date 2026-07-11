# Handoff — refactor for agent-first (and human) maintainability

> **Status (2026-07-11): steps 1–5 landed** as 17 behaviour-preserving
> commits; step 6 remains optional follow-up.
>
> - Tests: vitest + cargo test wired (`pnpm test`), 85 JS + 23 Rust tests over
>   the pure modules.
> - `App.svelte` 1756 → 903 lines: tab packing → `lib/tabFit.ts`, auto-handoff
>   → `lib/stores/handoff.svelte.ts`, relocate → `lib/workspaceRelocate.ts`,
>   toast → `lib/stores/toast.svelte.ts`, send-shortcut → `lib/sendShortcut.ts`,
>   auto-naming → `lib/autoName.ts`, tab strip → `lib/SessionTabs.svelte`.
> - `ProjectPicker.svelte` 1918 → 210 lines: six section components +
>   `RowMenu` + `lifecycle.svelte.ts` + shared `chrome.css` under
>   `src/panels/picker/`.
> - `ARCHITECTURE.md` added (module → responsibility map).
> - Deviation from step 3: no `TopBar.svelte` wrapper — once the tab strip left,
>   the header was ~30 lines of wiring, and a wrapper would only add ~17
>   pass-through props. App.svelte *is* the orchestrator.
> - Still open (step 6): decompose `VcsPanel.svelte` (818), `CommitModal.svelte`
>   (729), `vcs.rs` (700).

**Goal (from the user):** make the codebase *readable and maintainable primarily
by agents, but also accessible to humans.* This is a fresh-session task — audit,
agree a plan with the user, then land it as small, verifiable commits.

The engineering principles in `CLAUDE.md` still govern everything. This doc adds
the *why* and a prioritized starting map; it does not replace a real audit.

---

## What "agent-first maintainable" means here

An agent edits by locating an exact string in a file and replacing it, and it
pays context for every line it must read. So the levers are:

1. **Small, single-concern files.** A 1,900-line component forces an agent to
   load the whole thing to change one panel, and raises edit-collision risk.
   Prefer many focused files (the Rust side already does this — `lib.rs` only
   wires; each concern is its own module).
2. **Pure logic extracted from UI.** Logic that lives inside a `.svelte`
   component can only be exercised by rendering it. The same logic in a plain
   module (or `.svelte.ts` runes module) can be unit-tested and reused.
3. **Tests as a safety net.** There is **no test runner** in the repo today
   (`package.json` has no `vitest`; no `cargo test` targets). Tests are the
   single biggest agent lever: they let a future agent refactor boldly and
   *know* it didn't break behaviour. Add them.
4. **A one-read orientation map.** `docs/requirements.md` is the spec; there is
   no module→responsibility index. An `ARCHITECTURE.md` (or a `docs/` map) lets
   an agent orient in one read instead of grepping the tree.
5. **Intent in comments, not mechanics.** Say *why*, not *what*. The codebase is
   already good at this — keep it.

---

## Current state (2026-07-11)

Strong foundation: strict TS, zod at every IPC boundary, enums over string
literals, full-word names, `@/` path aliases, hand-authored M3 tokens (now
full-word CSS custom properties). Rust modules are cleanly separated. This is a
tidy codebase — the work is **decomposition and test coverage**, not cleanup.

### The mega-files (biggest wins first)

| File | Lines | Concerns bundled into one file |
| --- | --- | --- |
| `src/panels/ProjectPicker.svelte` | 1918 | recent list + inline rename, quick-start card, new-project form, on-launch toggle, default-agent chips, **editor rules**, root folders + detected projects, kebab menus |
| `src/App.svelte` | 1756 | phase routing, spawned-window routing, auto-naming, toast, global send-shortcut, **auto-handoff** (context→successor), **relocate** (move/rename w/ lock handling), **session-tab overflow packing**, side-panel state, send-to-agent FAB |
| `src/panels/VcsPanel.svelte` | 818 | status groups, restore-a-version, inline diff, recent commits |
| `src/lib/CommitModal.svelte` | 729 | commit dialog: file list + diff + big-file fallback |
| `src-tauri/src/vcs.rs` | 700 | status, log, diff, restore/bisect, worktrees |

`App.svelte` and `ProjectPicker.svelte` are the priorities — each mixes ~6–8
independent concerns. Most are extractable with no behaviour change.

---

## Suggested plan (confirm with the user before large moves)

Work in this order; each step is its own commit and must keep
`pnpm check`, `pnpm lint`, `pnpm build`, and `pnpm lint:rust` green.

1. **Test harness first** — add `vitest` (frontend) and wire `cargo test`. This
   is the safety net for everything after. Then cover the pure functions that
   already exist: `lib/diff.ts` (`parseDiff`/`toSplitRows`/`firstChangedLine`),
   `lib/format.ts`, `lib/paths.ts`, `lib/colors.ts`, `lib/validate.ts`; Rust
   `ide::open_args`, `naming`, `refs`. Green tests here make the rest safe.
2. **Extract non-UI logic from `App.svelte` into modules** (behaviour-preserving,
   test each as you go):
   - session-tab overflow packing → `lib/tabFit.ts` (pure) + a thin action;
   - auto-handoff → `lib/stores/handoff.svelte.ts`;
   - relocate (move/rename + lock handling) → `lib/workspaceRelocate.ts`;
   - auto-naming, toast, send-shortcut → small modules/composables.
   `App.svelte` should end up a thin orchestrator that wires them.
3. **Split the workspace shell UI** — lift the top bar (brand/app-menu, session
   tabs, usage/design/ide menus, panel segmented control) into a `TopBar`
   component; the tab-strip + overflow into its own component.
4. **Decompose `ProjectPicker.svelte`** into section components (RecentList,
   QuickStart, NewProjectForm, EditorRules, RootFolders), each owning its markup
   + styles. Shared row/menu chrome → a small reusable snippet/component.
5. **Add `ARCHITECTURE.md`** — one table mapping every `src/` and
   `src-tauri/src/` module to its single responsibility and its collaborators.
6. **VcsPanel / CommitModal / vcs.rs** — same decomposition pass if time allows.

## Constraints / guardrails

- **No behaviour or visual change** during decomposition — it must match the
  design canvas (`PADE.dc.html` on claude.ai/design) exactly as it does now.
- **Digestible commits:** one extraction per commit; never mix a move with a
  behaviour change. Each commit compiles + lints + builds.
- Keep the M3 design system and the `CLAUDE.md` rules intact.
- Don't add dependencies beyond a test runner without justifying it (principle
  #10). `vitest` is justified: it's the standard Vite-native runner, minimal, and
  it is the enabling tool for this whole effort.

## Verify

`pnpm check` · `pnpm lint` (js+css+rust) · `pnpm build` · `pnpm app` to smoke-test
the real window, and (once added) `pnpm test` + `cargo test`.
