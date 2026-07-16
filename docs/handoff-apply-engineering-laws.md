# Handoff — apply the software-engineering laws across the codebase

> **Status: COMPLETED 2026-07-16** (`b445fda`..`0878ed9`, 17 commits). All four
> sweeps ran: design (doc-truthing, dead-IPC removal, shared popover shell +
> DiffView), quality (pty.rs chunk-boundary fixes with its first 12 tests, VCS
> diff-collapse fix, 179 JS / 103 Rust tests up from 136 / 67, `pnpm test:e2e`
> CDP smoke suite), architecture (Hyrum contracts section in ARCHITECTURE.md,
> two-screens doc linked from Terminal.svelte), planning (requirements.md
> statuses synced). Demeter + broken-windows audits came back clean. The audit
> table below is kept for reference.

**Mission:** audit PADE against the laws in the `software-engineering-laws`
skill (55 laws from lawsofsoftwareengineering.com, installed user-level at
`~/.claude/skills/software-engineering-laws/`) and land the improvements as
small, verified, digestible commits. This is an incremental hardening pass, not
a rewrite — Gall's Law applies to the pass itself: PADE works; evolve it.

## How to run the pass

1. Load the skill (`/software-engineering-laws`) and read ONLY the category
   file for the sweep you're doing (`references/<category>.md`).
2. Sweep in this order (most code-actionable first): **design → quality →
   architecture → planning**. Skip `teams`/`decisions`/`scale` as sweeps — one
   person, no distributed system; use them only as reasoning aids.
3. One concern per commit (conventional style), each verified before commit:
   `pnpm check && pnpm lint && pnpm test` (covers Rust clippy::pedantic + fmt +
   cargo test too). UI-visible changes: verify in the real app over CDP — see
   the memory note `pade-webview-cdp-reach` (launch with
   `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9222' pnpm app`,
   navigate phases via `?w=empty` / `?w=temp`, screenshot and Read it).
4. A structural change without an `ARCHITECTURE.md` update is incomplete
   (CLAUDE.md rule 4). Push to `main` when green.

## Ground rules — where the laws meet the house rules

- **Already codified — don't re-litigate, enforce:** DRY, KISS, YAGNI, SoC,
  minimize-dependencies are CLAUDE.md rules. The sweep looks for *violations*,
  not for new policy.
- **Postel's Law loses to the zod boundary:** the repo deliberately validates
  every IPC payload and user input strictly (`bridge.ts`, `validate.ts`).
  "Tolerant input" here means graceful *error handling*, never accepting
  malformed shapes. The skill's Postel entry itself carries this security
  caveat.
- **Premature Optimization:** the repo already prefers clarity over cleverness
  (rule 6). Don't add caching/virtualization anywhere without a measured
  hotspot; conversely don't strip existing perf work that's documented as
  measured (tooltip display:none, tabFit measurement — see comments/memories).
- **Boy Scout Rule, scoped:** clean adjacent smells in files you're already
  touching, but never mix that cleanup into a feature commit — separate
  `refactor:`/`chore:` commits.

## Audit map — hypotheses to verify, not findings

Candidate hotspots per law (verify before acting; some may already be fine):

| Law | Where to look |
| --- | --- |
| Kernighan's Law | The densest logic: `tabFit.ts` packing, `dragReorder.ts` geometry, `pty.rs` alt-screen replay, `usageGroups.ts`. Is each still debuggable? Add clarifying names/tests, not comments. |
| Hyrum's Law | The IPC surface (`bridge.ts` ↔ `#[tauri::command]`s) and `pty_history` chunk contract — every observable behavior is an internal-dependency risk; note load-bearing quirks in ARCHITECTURE.md. |
| Law of Leaky Abstractions | `Terminal.svelte`'s normal-vs-alt-screen resize policy leaks PTY reality into the UI layer — is the leak contained in one place and documented? |
| Law of Demeter | Svelte components reaching through props (`session.agent.command`-style chains) — consider narrowing what's passed down. |
| Principle of Least Astonishment | Public helper names vs behavior (`workspace.temp()`, `ide.suggest()` ordering, `detachSession` vs `close`); rename or document surprises. |
| Testing Pyramid / Pesticide Paradox | 136 JS + 67 Rust unit tests, zero integration/E2E. The CDP client (scratchpad `cdp.mjs`) could seed a tiny smoke suite (boot → picker renders → tabs render). New tests should cover *untested* paths, not re-walk covered ones. |
| Broken Windows | `rg -n "TODO|FIXME|HACK" src src-tauri` and stale comments vs behavior (several were fixed this week — verify none regressed). |
| Technical Debt | `docs/handoff-*.md` backlog — the deferred signed-MSIX installer, WebView2 resize-blank workaround. Record interest-bearing debt explicitly in the docs rather than fixing here. |
| Gall's / Second-System | Any temptation to "unify" the three menu implementations (AppMenu, SessionTabs menus, IdeMenu share popover chrome by copy) — extract only what's actually identical; resist the grand menu framework. |
| Zawinski's / YAGNI | Feature backlog in memory (design-embed, usage meter, task runner) — flag scope creep in `docs/requirements.md` rather than building. |

## State at handoff (2026-07-16)

- HEAD `1f87056` on `main`, tree clean, all checks green
  (136 JS tests, 67 Rust tests, svelte-check, oxlint+eslint, stylelint,
  clippy::pedantic, vite build).
- This week landed: per-tab context-usage % (`contextLevel.ts`, `--context-*`
  tokens), backend `KIND_REGISTRY` + `ide_kinds` (one table drives detection /
  preferences / UI rows), official brand icons (JetBrains per-product, devicon
  multi-colour languages) with `[data-brand]` tints for single-colour brands.
- The skill's reference files are the authority for each law's statement and
  application guidance — quote them, don't paraphrase from memory.
