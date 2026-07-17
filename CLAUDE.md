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

4. **Document requirements & architecture** — software requirements live in
   `docs/` (see `docs/requirements.md`) and the persistent memory. Update them
   when scope changes; code and docs stay in sync. **`ARCHITECTURE.md` is part of
   this contract**: whenever you add, split, rename, or meaningfully re-scope a
   module — or change how the layers talk (a new IPC command, a new cross-module
   flow) — update `ARCHITECTURE.md` in the same change so its module map, prose,
   and mermaid diagrams never drift from the code. A structural change with a
   stale architecture doc is an incomplete change.

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
   - Validate **user input** too: every free-text field / form value is a trust
     boundary. Parse it through a zod schema (trim, length cap, shape) at the
     point of entry via `@/lib/validate` (`parseInput({ schema, raw })`, or
     `schema.safeParse` when you need the error to show), before it reaches logic
     or the backend. Never pass a raw `input.value` onward.
   - Path aliases: import internal modules through the `@/` alias (→ `src/`) —
     `@/lib/bridge`, `@/panels/Terminal.svelte` — never a relative `./` or `../`
     chain. Configured once in `vite.config.ts` (`resolve.alias`) and
     `tsconfig.json` (`paths`). Third-party packages stay bare imports.
   - Reuse relentlessly (TS & Rust): factor shared logic into one function and
     call it; a validated-`invoke` wrapper, `is_on_path`, `lookup` — extend these
     rather than re-deriving. If you write similar code twice, stop and extract.
   - CSS: logical properties, `gap`, container/`clamp()` sizing, nesting,
     `color-mix()`, custom properties. No hard-coded magic numbers where a token
     exists. No heavyweight frameworks — hand-authored M3 tokens only. In Svelte,
     never use a `style="…"` attribute — bind dynamic values with the `style:`
     directive (`style:--x={v}`, `style:anchor-name="--a"`).
   - Popovers/menus: use the native popover API (`popover` + `popovertarget`)
     with CSS anchor positioning so they light-dismiss on outside click — not a
     JS-toggled dropdown.
   - Performance first (virtualize long lists, debounce, GPU rendering, lazy
     panels) — but never at the cost of readability. Clear beats clever.
   - Early returns: prefer guard clauses that bail out early over nested
     `if`/`else` pyramids, wherever it makes the happy path read top-to-bottom.
   - Object params: a function taking two or more arguments takes a single
     destructured object param (`fn({ a, b })`) instead of positional args, and
     reduce/reuse the param types (`z.infer`, shared interfaces) where applicable.
   - `await` over `void`: never fire-and-forget a promise (`void p`) when a later
     step or shared state depends on it — `await` it so ordering is guaranteed.
     `void` is only for genuinely independent side effects (e.g. opening an
     external app) with no follow-up and no shared-state race.
   - `String.replaceAll`: use `replaceAll` (not `replace` with a `/g` regex) for
     global replacement — it states the intent and reads clearer.
   - `await` over `.then()`: use `async`/`await`, never a `.then()`/`.catch()`
     chain.
   - No IIFEs: never immediately invoke a function expression — async or not,
     `(() => {…})()` / `(async () => {…})()` are out. Give the function a name and
     call it. To kick off async work from a lifecycle, use an `async onMount` (and
     tear it down in `onDestroy`) or a named `async` function called from the
     effect — never an anonymous self-call.
   - Enums over magic strings: never compare against a bare string literal. Model
     the closed set once — a Rust `enum`, or a TS `z.enum`/`as const` union — and
     compare against its members (`kind === ChangeKind.Created`, not
     `kind === "created"`). One authoritative definition, no scattered literals.
     The one exception is the closed set's *own definition*: the string values
     inside the `z.enum` / `as const` / `IdeId` object literal that declares the
     set are allowed to be hardcoded there — that file is their authoritative
     home. A literal is only "magic" once it appears a second time somewhere that
     should have referenced the member instead.
   - Name your conditions: when an `if`/`while`/ternary test isn't self-evidently
     what-it-checks, extract it into a descriptively-named boolean first
     (`const isTempWorkspace = …; if (isTempWorkspace)`), so the happy path reads
     as prose. Inline only conditions that are already obvious.
   - Prefer names over comments: make the code self-documenting — reach for a
     descriptive variable, function, or boolean name before a comment. Avoid
     comments unless absolutely necessary; reserve them for a non-obvious *why* a
     name can't carry, never to restate *what* the code does. Especially: never
     write a comment above an `if` to explain its condition — name the condition.
   - Full words, no abbreviations: name variables, functions, parameters, types,
     CSS classes, and CSS custom properties with complete, spelled-out words —
     `index` not `idx`, `button` not `btn`, `element` not `el`, `previous` not
     `prev`, `configuration` not `cfg`, `column` not `col`, and
     `--radius-small` / `--on-surface-variant` / `--font-monospace` not
     `--r-sm` / `--on-surface-var` / `--font-mono`. A descriptive full-word name
     always beats a terse one; the only exceptions are a bare loop counter (`i`),
     an event-handler parameter (`e`), and domain terms/initialisms conventionally
     written short (`ui`, `id`, `url`). Applies in TS, Rust, and CSS alike.
   - Tabular numerals: every place a number is displayed (counts, percentages,
     stats, timers, SHAs' surrounding metrics) sets `font-variant-numeric:
     tabular-nums` so digits align and don't jitter as values change.
   - Icons live in SVG only: every pictographic icon is an individual `.svg` file
     in `src/lib/icons/` (its one authoritative home) — never a hardcoded path
     string — loaded by `Icon.svelte` and rendered via `<Icon name="…" />`. Never
     inline a raw `<svg>`/`<path>` in a component, and never use an emoji or a
     Unicode pictograph (`✓`, `⚙`, `▶`) as an icon in markup or a string. Need a
     new glyph? Drop a `<name>.svg` in `src/lib/icons/` and add its name to
     `ICON_NAMES` in `Icon.svelte`, then reference it by name. (Bare typographic
     affordances like a `▾` disclosure caret or a `\` path separator are text, not
     icons, and stay as-is.)
   - Inline single-use event handlers: an event handler bound in exactly one
     place — a DOM `on*` attribute, a `<svelte:window>`/`<svelte:document>` event,
     or a component's `on*`/callback prop — and referenced nowhere else is written
     inline as an arrow at the binding, never hoisted to a named function. This
     holds even for multi-statement bodies. Name the event parameter `e`
     (`onclick={(e) => …}`). Keep a handler named only when it is genuinely shared
     (bound or called in two or more places) or needs a stable reference (an
     `addEventListener` paired with a `removeEventListener`).
   - No `window.` prefix: call browser globals bare — `matchMedia(…)`,
     `setTimeout(…)`, `addEventListener(…)`, `location.reload()` — never
     `window.matchMedia` / `window.setTimeout`. The global is implicit; the prefix
     is noise.
   - Component-scoped CSS over global: keep styles in the component's own
     `<style>` (Svelte scopes them) rather than global `theme.css` whenever
     possible. Reserve global CSS for genuinely cross-cutting utilities many
     components share by token/attribute/name — the design tokens, `[popover]`,
     the `.ck` checkbox, the `[data-tooltip]` bubble, the shared keyframes.

7. **Semantic HTML over ARIA** — reach for the element that already carries the
   role and behavior (`<button>`, `<nav>`, `<dialog>`, `<details>`, `<output>`,
   headings, lists) before adding `role`/`aria-*`. ARIA is the fallback for gaps
   native elements can't express, not the default. Correct semantics first;
   annotate only what's left.

8. **Nested CSS** — use native CSS nesting to keep a component's rules together
   and mirror its markup, instead of repeating long selector prefixes.

9. **Modern platform first; pure HTML & CSS over JS** — always reach for current,
   native HTML, CSS, and JavaScript features (the platform's own capabilities over
   libraries or polyfills), and whenever a behavior can be expressed in HTML or
   CSS, do it there rather than in JS: `:hover`/`:focus-within`/`:has()` state,
   `<details>`/`popover`/`dialog`, CSS transitions and animations, scroll-snap,
   container queries, `accent-color`. Only add JS when the behavior genuinely
   can't be expressed in HTML/CSS. Less JS = less to ship, parse, and break.

10. **Minimize dependencies** — every third-party package is supply-chain attack
    surface. Prefer implementing a small, well-understood utility yourself over
    pulling in a dep: a formatter, a tiny parser, a debounce, a clamp, an ID
    generator — write it. Reach for a dependency only when the problem is
    substantial, genuinely hard to get right, and the package is well-audited and
    already load-bearing (Tauri, Svelte, Vite, zod, xterm, portable-pty). When you
    do add one, justify why in the commit message and prefer the smallest, most
    trusted option with the fewest transitive deps. **Vendor assets locally** —
    embed fonts (self-hosted `@font-face` + `woff2` in the repo), icons, and other
    static assets rather than loading them from a runtime CDN; no third-party
    origin should be fetched at runtime.

## Project shape

See `docs/requirements.md` for the full MVP spec and `README.md` for how to run.
Stack: Tauri 2 + Rust core, Svelte 5 + Vite frontend, xterm.js + `portable-pty`
terminal, `notify` watcher, Material 3 Expressive theme.

**Writing Rust?** Follow the `rust-best-practices` skill
(`.claude/skills/rust-best-practices/SKILL.md`) — idiomatic naming, error
handling, type safety, ownership, dependency discipline, and testing for the
`src-tauri` backend. Invoke it before writing or reviewing Rust here, or when a
clippy `-D warnings` finding needs an idiomatic fix rather than an `#[allow]`.
