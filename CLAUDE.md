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

## Project shape

See `docs/requirements.md` for the full MVP spec and `README.md` for how to run.
Stack: Tauri 2 + Rust core, Svelte 5 + Vite frontend, xterm.js + `portable-pty`
terminal, `notify` watcher, Material 3 Expressive theme.
