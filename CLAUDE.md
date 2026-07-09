# ADE — engineering principles (always apply)

These are non-negotiable for all work in this repo.

1. **DRY** — Don't Repeat Yourself. Every piece of knowledge has one authoritative
   home. Extract shared logic into a single module (e.g. the Tauri IPC bridge, the
   M3 tokens, the VCS abstraction) rather than copy-pasting. Before writing a
   helper, check whether one already exists.

2. **SoC** — Separation of Concerns. Each module/component owns one concern.
   - Rust: PTY, watcher, VCS, usage each live in their own module; `lib.rs` only
     wires them.
   - Frontend: one Svelte component per panel; shared state in `lib/stores`, all
     backend calls funnel through `lib/bridge.ts`. UI never talks to Tauri directly.

3. **Tree-shaking** — keep the bundle lean. Use named ESM imports (never
   `import * as`), no side-effectful barrel files, `import type` for types, and
   lazy-load heavy/optional panels so unused code is dropped.

4. **Document requirements** — software requirements live in `docs/` (see
   `docs/requirements.md`) and the persistent memory. Update them when scope
   changes; code and docs stay in sync.

5. **Digestible commits** — small, single-purpose commits with a clear message.
   One feature or refactor per commit; never mix a refactor with a feature. Each
   commit compiles. Conventional-commit style (`feat:`, `fix:`, `refactor:`,
   `docs:`, `chore:`).

6. **Modern & performant** — use current language features and best practices:
   - TypeScript: `strict` mode, `import type`, discriminated unions, `satisfies`,
     no `any`. Prefer immutable data and pure functions.
   - Runtime validation: use **zod** at every trust boundary (all IPC responses
     from Rust). Define one schema per payload and derive the TS type with
     `z.infer` — schema is the single source of truth, never a hand-written
     `interface` alongside it.
   - Reuse relentlessly (TS & Rust): factor shared logic into one function and
     call it; a validated-`invoke` wrapper, `is_on_path`, `lookup` — extend these
     rather than re-deriving. If you write similar code twice, stop and extract.
   - CSS: logical properties, `gap`, container/`clamp()` sizing, nesting,
     `color-mix()`, custom properties. No hard-coded magic numbers where a token
     exists. No heavyweight frameworks — hand-authored M3 tokens only.
   - Performance first (virtualize long lists, debounce, GPU rendering, lazy
     panels) — but never at the cost of readability. Clear beats clever.
   - Early returns: prefer guard clauses that bail out early over nested
     `if`/`else` pyramids, wherever it makes the happy path read top-to-bottom.
   - Object params: a function taking two or more arguments takes a single
     destructured object param (`fn({ a, b })`) instead of positional args, and
     reduce/reuse the param types (`z.infer`, shared interfaces) where applicable.

7. **Semantic HTML over ARIA** — reach for the element that already carries the
   role and behavior (`<button>`, `<nav>`, `<dialog>`, `<details>`, `<output>`,
   headings, lists) before adding `role`/`aria-*`. ARIA is the fallback for gaps
   native elements can't express, not the default. Correct semantics first;
   annotate only what's left.

8. **Nested CSS** — use native CSS nesting to keep a component's rules together
   and mirror its markup, instead of repeating long selector prefixes.

9. **Pure CSS over JS** — prefer a CSS solution whenever one exists: `:hover`/
   `:focus-within`/`:has()` state, `<details>`/`popover`/`dialog`, CSS transitions
   and animations, scroll-snap, container queries, `accent-color`. Only add JS
   when the behavior genuinely can't be expressed in CSS. Less JS = less to ship,
   parse, and break.

## Project shape

See `docs/requirements.md` for the full MVP spec and `README.md` for how to run.
Stack: Tauri 2 + Rust core, Svelte 5 + Vite frontend, xterm.js + `portable-pty`
terminal, `notify` watcher, Material 3 Expressive theme.
