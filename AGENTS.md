# Agent Instructions

Before working in this repository, read and follow `CLAUDE.md` in the repository
root. `CLAUDE.md` is the single authoritative source for project instructions;
do not duplicate those rules here.

## After every batch of changes

1. **Run the linters, and leave zero errors.** Run `pnpm lint` (oxlint and
   eslint, stylelint, clippy at `-D warnings` plus `cargo fmt`, and the fallow
   audit) and `pnpm check`. Fix every error, including ones in files you didn't
   touch, before you commit or call the work done. Never silence a rule to get
   past it.
2. **Enforce the engineering rules in `CLAUDE.md` on what you changed**, in
   Rust and TypeScript alike:
   - DRY: one source of truth for every value and every piece of logic
   - SoC: one concern per module and per component
   - early returns over nested branches
   - tree-shaking: named imports, `import type`, no side-effect barrels, and
     no exports that are unused or used only in their own file (fallow flags
     them)
   - the rest of `CLAUDE.md` rule 6, plus the `rust-best-practices` skill for
     Rust
