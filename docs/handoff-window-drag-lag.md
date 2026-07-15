# Window drag lag — resolved (2026-07-16)

Dragging the PADE window by its **title bar** felt laggy. It was **one root cause with
two faces**, not two bugs: a **window `focus` listener that re-ran heavy detection**
(`ide.suggest()` / `agents_detect`, each spawns processes). A Windows title-bar drag
enters a native modal-move loop that churns focus, so those calls fired during the drag.

## The two faces

| Symptom | Why |
|---------|-----|
| **Every** drag lags | Focus churns throughout the drag → detection fires repeatedly |
| Only the **first** drag lags, rest smooth | The *first* focus ran the probes **cold** (OS caches empty → slow); later drags hit warm caches → fast. Misfiled by an earlier session as an inherent WebView2 compositor init — it never was. |

## The fix — re-detect on visibility, not focus

Detection is now triggered by **page visibility**, never window focus:

- `src/lib/IdeMenu.svelte` (editors): detect once at mount, then on `<svelte:document
  onvisibilitychange>` when the page becomes visible again.
- `src/App.svelte` (agents): same `visibilitychange` trigger, plus the pre-existing 30s
  poll as a fallback.

**Why visibility is the right signal:** a page's `visibilityState` **cannot change while
you drag a window that stays on screen** — dragging never hides it. So `visibilitychange`
is drag-safe *by construction*, not by a timing heuristic. It still fires on the real case
this feature exists for — you switched away to install an editor/agent and came back
(minimize or full-occlusion → `hidden` → `visible`). Minor gap: switching back without
ever hiding PADE (side-by-side, no occlusion) won't refresh editors until the next
visibility change; agents still get the 30s poll. Rare, accepted.

## What did NOT work (don't re-try)

- **Debounce the focus handler** (250ms, prior commit `6d287a8`). Only *delayed* the one
  call to ~250ms *into* the drag → a brief hitch a moment after grabbing the bar.
- **Switch to OS activation** (Tauri `getCurrentWindow().onFocusChanged`), betting the
  top-level window's activation stays put while only the child webview's DOM focus churns.
  **Wrong** — the modal-move loop churns OS activation too, so it fired repeatedly mid-drag
  and (debounce now gone) the lag came back worse. **Lesson: during a title-bar drag EVERY
  focus signal churns — DOM focus and OS activation alike. Don't re-signal focus; use a
  signal a drag doesn't touch (visibility).**
- **A Rust `window::prewarm_drag` `set_position` nudge at startup** to "pre-warm" a
  supposed compositor init. Didn't help (a programmatic move isn't the native modal-move
  loop) and was pointless once the real cause was found — added then reverted.

## Testing gotcha that misled two sessions

**Svelte HMR does not reliably deregister a `<svelte:window>` / `<svelte:document>` /
Tauri event listener on hot-reload.** After you edit a listener out, the old one stays
live in the running app, so you still feel the lag — masking the fix. **Always confirm a
listener-removal fix on a full rebuild+relaunch, never an HMR'd instance.** The earlier
"compositor init reproduced on old commit `2b18e00`" bisect was almost certainly this
contamination (an old *frontend* checkout only HMRs; force a relaunch with
`touch src-tauri/build.rs src-tauri/src/lib.rs`).

## Dead ends (ruled out earlier, don't re-chase)

- **Tooltips** (`c9d199a` pure-CSS `::after`) — not the cause.
- **WebGL renderer** — disabling it didn't help.
- **Window transparency** (`05458c2` added `transparent:true`; `c735d27` dropped it) — the
  Jul-10 mitigation stopped working after **WebView2 Runtime 150.0.4078.65** (2026-07-11).
- **Fullscreen renderer** (`f4be16e`).
- Bisecting >~24 commits back needs `pnpm install` for drifted deps (`@xterm/addon-fit`
  drop `1a0c0aa` + xterm 6.1 bump `ad466df`).
