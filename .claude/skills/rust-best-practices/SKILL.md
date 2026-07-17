---
name: rust-best-practices
description: >-
  Idiomatic Rust conventions for the PADE Tauri backend (src-tauri) — naming,
  error handling, type safety, ownership, control flow, dependency discipline,
  and testing. Use BEFORE writing or reviewing any Rust in this repo, and when a
  clippy/pedantic warning needs an idiomatic fix rather than an `#[allow]`.
---

# Rust best practices (PADE `src-tauri`)

Synthesized from the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/checklist.html),
the [Rust Book on error handling](https://doc.rust-lang.org/book/ch09-00-error-handling.html),
the [idiomatic-rust](https://github.com/mre/idiomatic-rust) collection, and this
repo's own `CLAUDE.md`. These override defaults; where `CLAUDE.md` is more
specific (full-word names, no magic strings), it wins.

## 0. The gate is non-negotiable

Every Rust change must pass `pnpm lint:rust` before it is committed:

```
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt   --manifest-path src-tauri/Cargo.toml
```

Clippy runs at **`-D warnings`** (pedantic-leaning). A warning is an error. Fix
it idiomatically; reach for `#[allow(...)]` only with a one-line comment
justifying *why* the lint is wrong here (e.g. a dependency the lint would pull in
violates minimize-dependencies). Prefer restructuring:

- `single_match_else` → use `if let … else`, not `match` with one arm + `_`.
- `naive_bytecount` → don't `filter(|b| *b == x).count()` a big buffer; if you
  truly need a count, restructure (e.g. `position` to short-circuit) rather than
  add the `bytecount` crate.
- `needless_return`, `redundant_clone`, `manual_map`, `map_or` → take the
  suggestion; it is almost always cleaner.

Run the tests too: `cargo test --manifest-path src-tauri/Cargo.toml`.

## 1. Naming (RFC 430 + this repo's full-word rule)

- Types/traits/enums `UpperCamelCase`; functions/vars/modules `snake_case`;
  consts `SCREAMING_SNAKE_CASE`.
- **Full, spelled-out words** — `extension` not `ext`, `previous` not `prev`,
  `command` not `cmd`, `index` not `idx`. Only universal short forms (`id`,
  `url`, `ok`) and a bare loop `i` are allowed.
- Conversions follow the cost convention: `as_*` (cheap borrow), `to_*`
  (expensive/owned), `into_*` (consuming). Iterators: `iter` / `iter_mut` /
  `into_iter`.
- A function with a clear receiver is a method; constructors are inherent static
  methods (`Foo::new`).

## 2. Error handling

- Model fallibility with `Result<T, E>` and `Option<T>`; never paper over it.
- **No `unwrap()` / `expect()` / `panic!` on a runtime path.** They are for
  tests and truly-impossible invariants only. Propagate with `?`.
- Tauri commands return `Result<T, String>` (the wire boundary) — map internal
  errors with `.map_err(|e| e.to_string())`. Internal helpers should return a
  real error type, not `String`, when a caller needs to branch on it.
- Library/error types are **meaningful and well-behaved**: implement
  `std::error::Error` + `Display`, and keep them `Send + Sync + 'static` so they
  cross threads and `?` composes.
- Prefer a **guard with `let … else`** to unwrap-or-return:
  ```rust
  let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
      return None;
  };
  ```
- Use combinators (`map`, `and_then`, `map_or`, `filter`, `ok()?`) over nested
  `match` when they read cleaner; use `match` when the arms carry real logic.

## 3. Type safety — make illegal states unrepresentable

- **Newtypes for domain distinctions** — a `ProjectPath(String)` beats a bare
  `String`; the compiler then stops you mixing it with any other string.
- **Enums, never magic values.** Model a closed set once as an `enum` and match
  its variants — mirrors `CLAUDE.md`'s "enums over magic strings". Give the enum
  the one authoritative `as_str()`/`from_*` mapping (see `watcher::ChangeKind`).
- **Arguments convey meaning through types, not `bool`/`Option` flags.** Two
  `bool` params at a call site are unreadable; use an enum or a small struct.
- **No `as` casts.** `as` silently truncates/wraps. Use `From`/`Into` for
  widening, `TryFrom`/`try_into()` (handle the error) for narrowing, or
  `u128::from(x)` explicitly. (Enforced by clippy in this repo.)
- Structs keep **private fields**; expose behavior, not representation.

## 4. Ownership, borrowing, immutability

- **Borrow, don't clone.** Take `&str`/`&Path`/`&[T]` in function params; clone
  only when you must own. A `.clone()` in a hot path is a smell — check whether a
  borrow works first.
- **Immutable by default.** `let` over `let mut`; reach for `mut` only with a
  reason. Prefer building a value with an iterator chain over mutating an
  accumulator in a loop.
- Accept the most general borrow: `impl AsRef<Path>`, `R: Read` by value, `&[T]`
  over `&Vec<T>`.
- Iterators over index loops: `for entry in entries.flatten()`, `.filter_map()`,
  `.take(n)` for bounds — expressive and bounds-checked.

## 5. Control flow

- **Early returns / guard clauses** so the happy path reads top-to-bottom; don't
  nest `if/else` pyramids.
- `if let … else` for a single-pattern branch; `let … else` for the "bail if not
  this shape" guard.
- `matches!(value, Pattern)` for a boolean shape test.

## 6. Interoperability & derives

- Types **eagerly derive** the common traits when it makes sense: `Debug`
  (always, on every public type — clippy wants it), `Clone`, `Copy` (small POD),
  `PartialEq`/`Eq`, `Hash`, `Default`.
- Wire/persisted types derive serde `Serialize`/`Deserialize` with
  `#[serde(rename_all = "camelCase")]` so the TS side sees camelCase (the zod
  schema is the source of truth — see `CLAUDE.md`).
- Conversions use `From`/`TryFrom`/`AsRef`, not ad-hoc constructor methods.

## 7. Dependencies — minimize (supply-chain surface)

- **std first.** Before adding a crate, check whether std or a ~20-line helper
  does it. This repo hand-rolls percent-encoding, PATH search, registry reads
  (`reg query`), and home-dir lookup rather than pulling crates.
- **Shell out through `crate::util::command(...)`**, never `Command::new`
  directly — it suppresses the console window on Windows. Use it for `git`,
  `reg`, editor launchers, task runners. `git ls-files`, `git diff`, etc. go
  through the CLI, not a git library.
- A new dependency needs a justification in the commit message and must be small,
  well-audited, and already load-bearing.

## 8. Modules & SoC

- One concern per module (`pty`, `watcher`, `vcs`, `ide`, `usage`, `workspace`,
  `os`, `util`); `lib.rs` only wires them and registers Tauri commands.
- Shared helpers live in `util` and are reused (DRY) — extend `is_on_path`,
  `command`, `resolve`, `percent_encode` rather than re-deriving.
- Keep a `#[tauri::command]` thin: validate inputs, call into the module's real
  logic, map the error to `String`.

## 9. Testing

- Unit tests live in a `#[cfg(test)] mod tests` at the **bottom of the same
  file** as the code they cover; `use super::*` (or name the items).
- Test the **pure decision function**, not the I/O wrapper: factor the logic out
  (`surfaces(kind, exists)`, `is_generated(path, size)`, `dominant_kind(...)`) so
  it's testable without a live watcher or window.
- Use `std::env::temp_dir().join(format!("pade-…-{}", std::process::id()))` for
  scratch dirs (unique per test process); clean up at the end.
- When a test needs an external tool (git), **guard and skip** if it's absent
  rather than failing the suite:
  ```rust
  let Ok(init) = git(&["init", "-q"]) else { cleanup(); return; };
  if !init.status.success() { cleanup(); return; }
  ```
- Prefer several small named tests (`a_deletion_always_surfaces_…`) over one
  mega-test; the name documents the invariant.

## 10. Documentation

- `///` doc comments on public items, explaining the **why** (the non-obvious
  intent), not restating the signature. This repo's Rust is heavily commented on
  rationale — match that density.
- `//!` module docs at the top of each file state the module's one concern.
- Reserve inline `//` comments for a non-obvious *why* a name can't carry; name
  the thing well first.

---

**Sources:** [Rust API Guidelines checklist](https://rust-lang.github.io/api-guidelines/checklist.html) ·
[The Rust Book — Error Handling](https://doc.rust-lang.org/book/ch09-00-error-handling.html) ·
[idiomatic-rust](https://github.com/mre/idiomatic-rust) ·
[Rust By Example — Error handling](https://doc.rust-lang.org/rust-by-example/error.html) ·
[Rust Design Patterns](https://rust-unofficial.github.io/patterns/) · this repo's `CLAUDE.md`.
