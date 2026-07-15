# Handoff — window drag lag (2026-07-15)

Dragging the PADE window by its **title bar** felt laggy. It turned out to be **two
separate issues**. One is fixed; one is open.

## TL;DR

| # | Symptom | Cause | Status |
|---|---------|-------|--------|
| 1 | **Every** drag lags (persistent) | `IdeMenu.svelte`'s `<svelte:window onfocus>` re-ran `ide.suggest()` (spawns editor-detection processes) on every focus; a title-bar drag churns window focus | **Fixed** — `6d287a8` debounces it |
| 2 | Only the **first** drag lags, smooth after | One-time WebView2/compositor init on the first window move; present on **every** commit | **Open** — deferred to next session |

Keep these separate when testing: **drag once** (issue 2) vs **drag several times** (issue 1).

## Issue 1 — persistent per-drag lag (FIXED)

`git bisect` (good `2b18e00`, bad `HEAD`, criterion = *does every drag lag, or just
the first?*) landed on **`597589a` "feat(ide): open the auto-detected best-fit editor
from a split button"** as the first commit with the persistent lag. It added:

```svelte
<svelte:window onfocus={async () => { ides = await ide.suggest(); }} />
```

`ide.suggest()` is a Rust IPC that spawns processes to detect installed editors. On
Windows a title-bar drag churns window `focus`, so that heavy call fired throughout
every drag → persistent lag. **Fix (`6d287a8`):** debounce the re-detect (250ms) so a
drag's focus churn collapses into a single run *after* it settles, never mid-drag —
the "new editor appears without a restart" feature is preserved.

> **Verify:** relaunch, drag the title bar several times — every drag should be smooth
> now (bar the first-drag warmup below). Avi had not yet confirmed at handoff time.

## Issue 2 — first-drag warmup (OPEN, next session)

Independent of any commit, the **first** title-bar drag after a fresh launch lags,
then every drag after is smooth. It reproduced on clean rebuilds of commits from
Jul 11–15 (incl. `2b18e00`, `8e68271`) — a one-time compositor/GPU init on the first
window move under WebView2, **not a git regression**.

**Suggested fix:** pre-warm the drag path at startup — after `main.show()` in
`src-tauri/src/lib.rs`, do a tiny programmatic window nudge (e.g. `set_position`
+1px and back) so the one-time init happens before the user's first real drag. It's
speculative (a `set_position` may not exercise the same modal-move path as a real
title-bar drag) — measure by testing the **first** drag on a fresh launch.

## Dead ends (don't re-chase)

- **Tooltips** (`c9d199a` pure-CSS `::after`): not the drag cause. (Still improved this
  session — render-on-hover via `display:none`, see `11fe04a`.)
- **WebGL renderer**: disabling it didn't help.
- **Window transparency**: `05458c2` added `transparent:true` as the Jul-10 drag
  mitigation; `c735d27` dropped it. Re-adding it (even with `53bd740`'s opaque
  `set_background_color` removed) does **not** help now — the mitigation stopped working
  after **WebView2 Runtime 150.0.4078.65** (installed 2026-07-11).
- **Fullscreen renderer** (`f4be16e`), JS window listeners other than the IDE one.

## Bisect mechanics (for next time)

- Test conditions must be identical: **fresh relaunch** each step (a cargo rebuild
  relaunches `pade.exe`; a frontend-only checkout only HMRs — force a relaunch with
  `touch src-tauri/build.rs src-tauri/src/lib.rs` so the first-drag test is valid).
- Commits older than ~24 back don't build against the current `node_modules` (the
  `@xterm/addon-fit` drop `1a0c0aa` + xterm 6.1 bump `ad466df`) — `pnpm install` at
  that commit to restore its deps, then `pnpm install` again on return.

## Also shipped this session (design-sync)

`681b888` onboarding working-directory row · `11fe04a` tooltips render-on-hover +
centered placement · `30268d1` drop redundant IDE / tab-covering project tooltips.
