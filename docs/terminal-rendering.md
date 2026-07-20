# Terminal rendering — the two screens, and why they invert every rule

Read this before touching `src/panels/Terminal.svelte` or the `env` column of the
registry in `src-tauri/src/agents.rs`. It supersedes the resize handoffs
(`handoff-terminal-midstep.md`, `handoff-terminal-resize.md`), which are deleted.
Everything below is **measured** against the live app, not reasoned from first
principles — several very plausible ideas in here turned out to be wrong, and the
point of the document is to stop you re-deriving them.

## The one fact everything follows from

A terminal has two screens, and **which one a program paints on decides everything**
about how a resize must behave. They are opposites, and ADE hosts both.

| | **Normal screen** | **Alternate screen** |
| --- | --- | --- |
| What it is | A real document, with real scrollback | A framebuffer the program owns |
| Who can paint a row | The terminal (it holds the text) | **Only the program** |
| On resize | xterm can rewrap the text itself — continuously, like a web page | Nothing to reflow: the terminal must wait for the program to repaint, which lands a whole row at a time |
| Runs there | a shell, an agent with no fullscreen mode, Claude Code with `/tui default` | **Claude Code (as ADE runs it)**, Codex, aider, a pager or editor an agent opens |

## What ADE runs Claude Code as

Its **fullscreen renderer**, on the alternate screen — the polished TUI: flicker-free
output, mouse support, selection that copies itself. The registry forces it:

```
CLAUDE_CODE_NO_FLICKER=1
```

by env rather than the `tui` setting, so it does not depend on — and cannot be undone
by — whatever the user's own Claude config says. (`CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN=1`
is the opposite lever: it forces the classic main-screen renderer, and takes precedence
over both this and the `tui` setting.)

**The cost is real and was chosen deliberately.** On the alternate screen a resize
cannot flow like a web page: the agent owns the pixels, it repaints in whole rows, and
no emulator-side trick reaches that — the content is on the far side of the PTY. The
"reflow like a document" machinery below is therefore *not* dead code: it is what runs
for every session on the normal screen, and it is what would run for Claude Code again
the moment someone flips the renderer back.

## The three rules on the NORMAL screen

All in `Terminal.svelte`. Each is load-bearing; each is measured. (For the alternate
screen — which is what Claude Code runs on today — skip to "The alternate screen
inverts every rule".)

### 1. Whole cells, rounded down

`rows = floor(paneHeight / cellHeight)`, `cols = floor(...)`. Never `ceil`: the
overflowing row would have to be clipped, and on the normal buffer the top row is
**real content**, not slack. (An earlier design did exactly this — `ceil` plus a
clipped top row — and it worked on the alt screen, where every row was the agent's
to redraw. On the normal buffer it cuts the top off the welcome box.)

### 2. Pin the grid to the end of the document you are looking at

The grid is whole cells and the pane is not, so a sub-cell remainder always exists
and has to go somewhere. Which end depends on what the terminal is showing —
`anchorBottom` in the component, driven by `buffer.active.baseY > 0`:

| State | Pin | Why |
| --- | --- | --- |
| Conversation still fits (no scrollback) | **top** | Output starts at row 0, so pinning row 0 keeps every line at a fixed y. A resize then moves *nothing* — the pane just reveals or hides empty rows at the bottom. |
| Conversation scrolls (scrollback exists) | **bottom** | xterm scrolls the document in whole rows, so a top-pinned grid would step the text a full row each time the row count changes. |

Bottom-pinned, a visible line sits at `y = paneBottom - (linesFromEnd + 1) *
cellHeight` — **no `rows` term in it**. The row xterm scrolls away and the
remainder the grid gains cancel exactly, so the text is continuous *through* a row
boundary. Measured, sweeping the pane 5px at a time across three row changes: the
prompt's hint line held **34px from the pane's bottom edge at every single
height** (519−485, 514−480, … 469−435).

### 3. Tell the agent the WIDTH only, and only once the drag settles

The grid refits every animation frame (rAF-coalesced `ResizeObserver`), so the text
tracks the drag. But `pty.resize` — the `SIGWINCH` — fires **only when the column
count changes**, debounced by `SIGWINCH_SETTLE_MS` (150ms). A vertical drag now
sends the agent *nothing at all*.

A CLI printing an inline document needs the width: that is what its text wraps to.
It does not need the height — how much of a document you can see is the terminal's
business, and xterm already knows. Sending the height anyway is what kept a step on
screen, because every `SIGWINCH` makes the agent re-render:

- it **re-lays-out its own frame** for the new row count, dropping or adding a line.
  The conversation above that line then sits a full row off from the text below it.
  This is the step that survived every geometry fix — because it was never geometry.
  It was the document changing underneath us. (Watch `buffer.active.length`: it went
  44 → 43 → 42 as rows shrank. With the height suppressed it does not move.)
- Ink **reprints its whole static history** on a resize, so the previous copy is left
  behind in the scrollback — one orphaned conversation per `SIGWINCH`. A per-frame
  drag once left **52** of them. The visible screen looked perfect in every case; the
  damage was entirely in the scrollback, which no screenshot catches.

Width changes still have to go through — the agent's own box must rewrap to them —
but debounced, one drag costs one reprint. Measured after this: five height gestures
and five width gestures each leave the buffer with exactly **one** copy of the
conversation.

**Do not "fix" this by sending the height again.** It is the same trap as #3 in the
rejected list, wearing a different hat.

## The alternate screen inverts every rule

This is where Claude Code runs today, and the terminal can switch screens under you at
any moment anyway — a pager or editor an agent opens, or Claude Code put back with
`/tui default`. `Terminal.svelte` watches `buffer.onBufferChange` and keeps
`onAlternateScreen`, and every rule flips on it:

| | Normal screen | Alternate screen |
| --- | --- | --- |
| Grid refit | **every frame** — xterm owns the document and reflows it | **on settle only** — see below |
| `SIGWINCH` | width only, debounced; height **never** | **cols and rows, immediately** — only the agent can paint a row, so a size it hasn't heard is a row nobody paints |
| Grid anchor | top until it scrolls, then bottom | **top** — see "the stabiliser" below |
| xterm patch | active | **inert** (gated on `_hasScrollback`) |

The grid rule is the surprising one, and it took three attempts to get right.

A fullscreen TUI paints by **diffing against its own model of the screen** — that is
what makes it flicker-free. Resize the grid under it faster than it can process the
`SIGWINCH` and its model starts describing a screen that no longer exists; from then on
it writes only the cells it *believes* changed, so a torn frame **never repairs itself**.
Measured, resizing every frame through a fast drag: the agent **stopped painting
altogether** — the pane went blank and stayed blank, the process still alive, typing
into it producing nothing. This is not cosmetic damage. It wedges the renderer.

The obvious answers both fail:

| Attempt | Result |
| --- | --- |
| Resize every frame (as on the normal screen) | Renderer wedged; pane blank, unrecoverable |
| Freeze the grid for the whole gesture, move it on release | Safe, but then **the TUI only updates when you let go** |
| Fixed throttle (100ms) | Survived one drag, wedged on the third — a fixed rate just moves the cliff |

What works is **flow control**: one resize in flight at a time. Give the agent a size,
wait until it has finished painting it (its output goes quiet for `ALT_REPAINT_QUIET_MS`),
and only then hand it the size the pane has reached in the meantime. The drag is paced by
the agent itself — as fast as it can follow, never faster. Measured through four
consecutive fast drags: the frame stays intact every time, and mid-gesture the agent has
painted right down to the grid's last row, so it tracks the drag live.

Waiting on its repaint is necessary but **not sufficient**: the agent goes quiet
*between* the bursts of a single repaint, so the credit comes back early and the resizes
still pile up. `ALT_FIT_MIN_INTERVAL_MS` (250ms) is the floor on how often it may be
disturbed at all, whatever the pane is doing.

One more rule, either screen: **a hidden pane is never fitted.** A background
tab's slot is `display: none`, so its ResizeObserver reports 0×0; fitting to that
clamps the grid to 2×1 and SIGWINCHes the agent down to it — Claude shrugged it
off, Codex exited (switching tabs "closed" the Codex session). `fitToPane` bails
on a zero-size viewport; the show fires the observer again with real dimensions.

Three more traps found the hard way:

- A drag that ends inside the minimum interval still has to land its last size — nothing
  else will come back to collect it, so the parked fit needs its own timer.
- If we ever give up waiting for a repaint (`ALT_REPAINT_TIMEOUT_MS`), the frame may be
  torn, and the gesture owes it a **full repaint** when it stops. Only then — forcing one
  after every drag just makes it end with a needless blink.
- Switching screens must immediately re-send the size, because on the normal screen we
  deliberately let the agent's idea of the height go stale.

### The stabiliser: pin the top, and squeeze the lag

A fullscreen agent's frame is **rigid** — the conversation is nailed to its first row, the
prompt to its last — and the pane's height is not a whole number of rows. So whichever
edge is *not* pinned is the one that jumps a whole row every time the row count changes.

Pinning the **bottom** welds the prompt to the pane's edge, which sounds right and is
wrong: it makes the entire conversation sawtooth by a row on every boundary. That is the
"mid-step". Pinning the **top** instead nails the conversation — measured across three row
boundaries, it does not move by a single pixel (`y = 51` at every height) — and the
remainder collects at the bottom as a strip of *terminal background*, which is not visible
as anything. The prompt block steps a row instead, into the space that just appeared,
which is what a terminal getting taller ought to look like.

One thing must not follow from that. Because the agent only reaches the new size at the
pace it can paint (above), the grid is briefly **taller than the pane** during a shrink —
and with the top pinned, that overflow hangs past the bottom edge and **cuts the agent's
status line off**. Nothing may ever cut that line. So while the grid is too tall it is
scaled to fit (`squeeze`): at most the size of the lag, ~3% on a normal drag, back to
exactly **1** the moment the agent catches up. Every settled state is unscaled, so the
text is crisp and clicks map true; the scale exists only in the moments the agent is
behind, and it is what stops the lag being visible at all.

## Attaching to a session already in flight

A PTY has no scrollback of its own, so a terminal that mounts onto a running session —
a hot-reloaded component, a reloaded window — has nothing to paint and sits blank while
the agent, quite happily, waits for input. It reads as *"the agent isn't starting"*, and
it is the same bug every time. `pty.rs` keeps each session's raw stream and hands it
back through `pty_history`; every chunk carries a sequence number, so a frontend that is
already listening to the live feed while it asks for the history can tell which chunks
that history already contains from which are genuinely new.

**But a fullscreen program's history is not a document — it is a stream of edits to a
framebuffer.** Once the buffer has been trimmed, the edits that built the frame are half
gone, and replaying it paints a torn one. So when the history says the program is on the
alternate screen (`pty.rs` tracks the DEC 1049 switches), the terminal switches to that
screen, replays what it has, and then **asks the program to repaint** — its own model of
the screen is the only complete copy.

The only lever for that is a resize: a fullscreen program re-lays-out when the size
changes. Two things had to be right, both measured:

- **The grid must move, not just the PTY.** Sending a new size to the program alone
  leaves xterm's grid saying one thing and the program's model another, and it paints
  its frame a row short (the hint under its prompt goes missing). Resizing the grid
  drives `term.onResize`, which sends the `SIGWINCH` — terminal and program move
  together, exactly as in a real resize.
- **The nudge must outlast the program's own coalescing of resize events.** At 40ms it
  processed the two as one and painted for the wrong size; 180ms is honest.

## The xterm patch (`patches/@xterm__xterm@…`)

Once the agent stopped repainting on every height change, its repaints stopped
papering over two bugs in xterm's own row resize. They are the two halves of one
thing: **a shrink followed by a grow must return the buffer exactly where it was.**
Stock xterm loses content in both directions.

### Shrink discards content below the cursor

```js
this.lines.length > this.ybase + this.y + 1
  ? this.lines.pop()            // "The line is a blank line below the cursor"
  : (this.ybase++, this.ydisp++);
```

The comment asserts the popped line is blank. **The code never checks.** Anything a
program printed *below* the cursor dies every time the terminal loses a row — and
Claude Code's `accept edits` hint sits below its prompt box, exactly there. It
vanished on the first shrink (`hintPresent: false`).

Patched: pop only a genuinely blank trailing line, otherwise scroll, so the content
moves into scrollback instead of the bin. When it scrolls and the cursor is *not* on
the last line, the cursor's viewport-relative `y` comes down with `ybase` so it stays
on the same absolute line (in the stock case the cursor *is* on the last line, and
the `y = min(y, newRows - 1)` clamp below already handles that).

### Grow refuses to reclaim the scrollback

```js
if (this.ybase > 0 && this.lines.length <= this.ybase + this.y + addToY + 1) {
  /* pull a line back from scrollback */
} else {
  /* push a blank line at the bottom */
}
```

That second test means *"only when there is nothing below the cursor"* — and the hint
line is below the cursor. So growing never reclaimed the scrollback; it pushed blank
lines under the conversation instead. Shrink → grow was therefore **lossy**: the
conversation marched off the top a row per cycle while the pane filled with dead
space, until the terminal looked empty. Measured, one shrink/grow round trip:
`baseY` 8 → 17 → **17** (stuck) and buffer length 46 → **56** (blanks piling up).

Patched: whenever scrollback exists, reveal it — which is what every terminal does
when you enlarge it. Same round trip after: `baseY` 6 → 15 → **5**, length constant
at **43**, no blanks. Lossless.

Both changes are gated on `this._hasScrollback`, which is exactly what tells the two
buffers apart. The **alternate screen keeps stock behaviour, byte for byte** — it has
nowhere to scroll a line *to*, and "preserving" content there just shoves the agent's
frame around underneath it. (An earlier ungated version did precisely that.)

Both shipped bundles (`lib/xterm.js`, `lib/xterm.mjs`) carry both changes; the patch
script asserts each site matches exactly once, so a version bump fails loudly instead
of silently no-op'ing. After any `pnpm patch` / `pnpm install`, clear
`node_modules/.vite` and restart, or Vite serves a stale pre-patch bundle and you
will "verify" the wrong code.

## Do not repeat these — all tried, all measured, all rejected

1. **CSS `scale` to stretch the grid over the sub-cell remainder.** Pins both
   edges, but glyphs breathe ~2.3% and snap back at each row (the user reads it as
   jumping), and it breaks xterm's click mapping (xterm divides pointer px by the
   *unscaled* cell size), which then needs an xterm patch. **Rejected.**
2. **Freeze the row count during the drag, reflow on release.** Perfectly smooth,
   and rejected explicitly: *"it will pause the terminal content… I want the
   resizing to behave identical to a webpage."*
3. **Bottom-anchoring xterm's buffer resize** (insert a blank at the top on grow;
   never `pop()` on shrink). On the alt buffer it shoved the agent's block down a
   row and pushed its last line off the end. The *real* bug in that code is
   narrower, and is what the patch above fixes: `pop()` discarding a line that
   isn't blank.
4. **Reaching for a different terminal library.** The library was never the
   problem; the screen buffer was. The one xterm bug that is real got a five-line
   patch.

## Harness — how to measure this yourself

- Launch with CDP:
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9222' pnpm tauri dev -- "<project>"`,
  then drive `http://127.0.0.1:9222` (a dependency-free WebSocket client is enough).
- **Never resize the OS window programmatically** (CDP `Browser.setWindowBounds`) —
  it triggers the WebView2 non-present bug in `handoff-webview2-resize-blank.md`
  (window goes blank) and corrupts the measurement. Resize the **pane** from inside
  the page instead: set `.term-pad`'s `paddingBottom` / `paddingRight`. It hits the
  identical `ResizeObserver → fitToPane → term.resize` path and is safe.
- To inspect the buffer, temporarily expose the terminal after `term.open(host)`:
  ```ts
  if (import.meta.env.DEV) { Reflect.set(globalThis, "__padeTerm", term); }
  ```
  then read `__padeTerm.buffer.active` over CDP (`type`, `baseY`, `length`).
  **Remove before committing.**
- WebGL renders to a canvas, so there are **no DOM rows to measure**. Locate a line
  by scanning the buffer for its text and converting its row index to a y.
- **Editing `Terminal.svelte` HMR-remounts the terminal**, which reattaches to the
  live PTY with no replay, so the pane looks blank. Dev artifact, not a bug —
  recover with a CDP `Page.reload`.

## Still open

- **Resume properly on relaunch.** When a session is resumed (green-dot resume,
  `workspaceRelocate.ts`, `stores/handoff.svelte.ts`), PADE seeds the literal prompt
  `"continue\r"` into a *fresh* conversation, which loses the context. It should
  launch the agent with its native resume flag instead — a `resume` column in the
  `agents.rs` registry (claude → `--continue`), mirroring `oneshot` and `env`.
  Resumed sessions only; fresh launches stay fresh.
