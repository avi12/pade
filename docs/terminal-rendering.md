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

**Whatever the user's own `tui` setting says.** ADE sets no renderer env for Claude —
exactly as Windows Terminal sets none. Claude's schema offers two:

| `tui` | Renderer | Who owns the scrollback |
| --- | --- | --- |
| `fullscreen` | alt-screen, flicker-free (≡ `CLAUDE_CODE_NO_FLICKER=1`) | **Claude**, virtualized inside itself |
| `default` | the classic main-screen one | **the terminal** |

ADE used to force `CLAUDE_CODE_NO_FLICKER=1` by env "so it cannot be undone by whatever
the user's own Claude config says". Overriding the user's renderer choice is not ADE's
call to make — and it is the choice that decides who owns the scrollback, which decides
what a wheel tick costs:

- **`fullscreen`** — the conversation lives inside Claude, so the terminal holds nothing
  to scroll. Every tick becomes a request: xterm → SGR mouse report → IPC → PTY → a whole
  agent repaint → back through IPC → DOM. Measured **~31ms per notch** on an idle session
  in the live app, and worse while the agent is also streaming.
- **`default`** — xterm owns the document and a tick is a local viewport scroll.

**Precedence, and the trap it sets:** `CLAUDE_CODE_NO_FLICKER` beats the `tui` setting, and
a `settings.json` `env` block sets that variable for *every* Claude, in every terminal. So
a user whose settings say `tui: "default"` can still be running fullscreen everywhere and
not know it — check the `env` block before concluding anything about which renderer is in
play. (`CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN=1` is the hard opposite lever: it beats both.)

**Do not explain a Windows-Terminal-versus-ADE difference by the screen mode without
checking that.** Measured on the wire (`portable-pty` harness, plus the live app over CDP),
the input side is *identical* to Windows Terminal: xterm emits exactly one SGR report per
wheel notch, the same one-per-`WHEEL_DELTA` that `mouseInput.cpp`'s
`_mouseInputState.accumulatedDelta` produces. There is nothing to port. When both terminals
run the same renderer, what is left is the repaint path itself — ConPTY straight into an
in-process buffer and a dedicated D3D render thread, against ADE's two IPC hops, JSON, a V8
script compile, zod, the per-chunk sniffers, and a DOM rebuild.

**The alternate-screen cost is still real, and still paid by anyone who chooses
`fullscreen`.** A resize there cannot flow like a web page: the agent owns the pixels, it
repaints in whole rows, and no emulator-side trick reaches that — the content is on the
far side of the PTY. Codex and any pager an agent opens still land there too, so none of
the machinery below is dead code; the "reflow like a document" rules are simply what
Claude now gets by default.

## The three rules on the NORMAL screen

All in `Terminal.svelte`. Each is load-bearing; each is measured. (For the alternate
screen — which is what Claude Code runs on today — skip to "The alternate screen
inverts every rule".)

### 1. Whole cells, rounded down

`rows = floor(paneHeight / cellHeight)`, `columns = floor(...)`. Never `ceil`: the
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

One more rule, either screen: **a hidden pane keeps a real layout size.** A
background tab's slot used to be `display: none`, so its ResizeObserver reported
0×0; fitToPane clamped that to a 2×1 grid and SIGWINCHed the agent down to it —
Claude shrugged it off, Codex exited (switching tabs "closed" the Codex session).
Hidden slots are now lifted out of flow over the whole pane (`position: absolute;
inset: 0`) and `visibility: hidden` instead (App.svelte's `.term-slot`), so a
background PTY stays sized to exactly what its tab will show and switching tabs
needs no refit at all. The hidden DOM terminal still lays out but does not paint,
and the `shown` prop disables its cursor blink loop.

The GPU rule is stricter: **terminals always use xterm's DOM renderer.** The
optional WebGL addon held one VRAM-backed context per shown terminal and kept GPU
work alive whenever PADE was focused, competing with games even though background
contexts were released. The DOM renderer keeps output and resizing live without
owning a WebGL context. A background tab or unfocused window also disables its
cursor blink paint loop; PTY parsing, scrollback, prompt detection, and replay
ordering continue uninterrupted. WebView2's native occlusion detection remains
enabled so a fully covered, minimized, or off-desktop window can suspend frontend
visual work entirely. The theme reconciles from the live OS state on
`visibilitychange` when it resumes.

PTY event handling and terminal painting have separate budgets. Every chunk is
inspected immediately for activity, trust prompts, usage limits, API failures,
and context state, while adjacent bytes are joined before xterm sees them. A
shown terminal in the focused window flushes once per animation frame while it
is at the live bottom. Hidden tabs, unfocused windows, and a terminal whose user
is reading scrollback flush every 250ms. Returning to the live bottom restores
frame cadence immediately. Ordering and every byte are kept, but a token stream
cannot force DOM parsing and paint into the middle of scroll frames.

**The wheel deferral is only for a tick that scrolls xterm's own document.** An
active wheel gesture there defers xterm writes until 120ms of quiet, with a
one-second ceiling for a continuous gesture, then one coalesced write catches up.
It must *not* apply when the agent has grabbed the mouse (every fullscreen TUI:
Claude, Codex, a pager) or when the tick is forwarded as PageUp/PageDown — there
the wheel is **input**, and the frame the agent paints back *is* the scroll.
Deferring it withholds the scroll itself: the agent answered every tick in a few
ms while the terminal sat on the frames, so a continuous gesture repainted about
**once per second** instead of once per frame. That was the "PADE's terminal lags
when I scroll, Windows Terminal doesn't" report — not renderer cost, a policy
applied one branch too wide. Gated on `wheelScrollsTerminalDocument`
(`lib/terminal-output`).

One final guard covers structural layout changes such as opening the task-runner
dock. A flex layout can briefly report an undersized viewport while it settles;
`fitToPane` keeps the previous grid until it measures at least 20 columns and 4
rows. Passing that transient as a clamped 2×1 PTY resize corrupts Codex's TUI in
the same way as hiding a pane did, even though the final dock layout is roomy.

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

## Right-to-left text: an overlay, never the stream

Arabic and Hebrew break the two things a terminal grid assumes — one glyph per cell,
and left to right. Their letters join into initial/medial/final forms depending on
neighbours, and they read the other way, so xterm paints them disconnected and
backwards.

**First, find out who is doing the reordering.** This is the trap the whole feature
fell into, and it costs a day: *correctly rendered* right-to-left text always looks
reversed when you transcribe the glyphs left to right, because that is the order they
sit in. `מידע נוסף` rendered correctly reads `ףסונ עדימ` off the screen — the same
string you would write down if it were broken. **A screenshot cannot tell you which
one you are looking at.** The only honest test is to compare two sources that cannot
both be wrong: the terminal buffer's *code points*
(`row.textContent`, escaped to `\uXXXX` so nothing can re-order them on the way out)
against what the agent actually wrote (its own transcript in
`~/.claude/projects/<slug>/<session-id>.jsonl`). If the buffer holds `ךו…`
where the transcript holds `מת…`, the agent shipped visual order.

**The stream is not the place to fix it.** The obvious design — reshape and reorder
the PTY bytes on the way in (what `ar-terminal` does for VS Code, and what an
extension has no other choice about) — is wrong *here*, for reasons that are specific
to what ADE does with its buffer:

- It bails on the **alternate screen**, which is where ADE's agents live. A fullscreen
  TUI addresses cells by column; reordering its output moves text out from under its
  own cursor arithmetic.
- Everything downstream reads the buffer: `getSelection` (the copied text), the link
  provider, the context parser, the choice-prompt and usage sniffers. Reordered bytes
  corrupt all of them at once — a user would copy visually-ordered gibberish.
- It requires hand-rolling the bidi algorithm and Arabic shaping tables. The browser
  already has both, correct and maintained.

So: the buffer keeps the agent's own logical order, and an overlay draws over it.

**Runs, not lines.** `lib/terminal-rtl` marks only the columns that read right to left,
and `lib/TerminalRtl` positions one box over exactly those cells. A whole-line overlay
(the `ar-terminal` shape) re-renders the entire row proportionally, which drags a TUI's
box borders and aligned columns off the grid. Two rules keep a run honest:

- a **frame glyph** (box drawing, geometric shapes, Powerline) ends a run — the bidi
  algorithm calls those neutral and would happily reorder a border into the middle of
  a sentence. Measured: `│ عربي داخل صندوق │` renders with both borders at their exact
  columns and the Arabic joined and reordered between them.
- interior neutrals stay *inside* a run up to a padding-width gap. A single space is
  prose — splitting there would reorder each word on its own and the sentence would
  read backwards — while a wide gap is an agent aligning a table, where joining would
  let one column's text slide into the next.
- a **digit continues a run but cannot open one**. Digits are *weak* in the bidi
  algorithm: a number that follows right-to-left text is carried along and reordered
  with it, so leaving `11.01.2027` out of the run that ends at `ביותר` left the phrase
  reading one way and the date it belongs to sitting at the row's other end. A number
  with nothing right-to-left before it reads left to right like the rest of the row,
  which is why it may not start one.

Three things to know before changing it:

- **The run box carries the terminal's own `direction: ltr`, not `rtl`.** The browser
  reads the characters and reorders them itself; the base direction only decides where
  the result is *anchored*. Claiming `rtl` anchored every run to its last column, so a
  phrase narrower than its cells hung off the right with a hole in front of it —
  `  - ` then a blank third of a row, then the Hebrew. With the grid's own direction
  the run starts at its first column and reads away from there, exactly as the same
  string does in any browser. Position it with physical `top`/`left` regardless: a cell
  grid has no writing mode (column 0 is the leftmost cell whatever is printed in it),
  and `inset-inline-start` would resolve against the box's own direction.
- **The caret and the selection are markers between spans, not measured x positions.**
  A logical column means nothing once a line has been reordered, so the run's text is
  split at those boundaries and the browser lays them out with the text. No
  measurement exists to disagree with the glyphs.
- **Every span boundary is a broken join.** Arabic does not shape across an element
  boundary, so the run is split only where the styling genuinely changes (and at the
  caret/selection edges). Don't split per cell — that is the disconnected rendering
  this exists to fix.

Programming fonts carry no Arabic or Hebrew at all, so the overlay appends
`--font-rtl-fallback` (theme.css) after the terminal stack rather than leaving the
platform to pick.

### Claude Code already did it, so the overlay stands down

Measured 2026-08-16 against 2.1.227: **Claude Code reorders its own right-to-left
text before it writes it.** The buffer held `ךותמ`/`בלש` while the same session's
transcript held `מתוך`/`שלב` — the CLI splits each row into per-character cells, runs
`bidi-js` over the row with an **`auto`** paragraph direction, and reverses the cells
by embedding level. In the binary:

```js
class BidiGate { needed; isNeeded(){ if (this.needed === undefined) this.needed = true; return this.needed } }
function reorderCells(cells) {
  if (!gate.isNeeded() || cells.length === 0) return cells;
  const text = cells.map(c => c.value.replace(/[؜‪-‮⁦-⁩]/g, "�")).join("");
  if (!/[֐-׿…܀-ݏ]/u.test(text)) return cells;
  const { levels } = bidi.getEmbeddingLevels(text, "auto");
  /* reverse runs of cells, level by level */
}
```

Two consequences:

- **`auto` is why a bullet flips.** The paragraph direction comes from the row's first
  strong character, so `- <hebrew>: 11.01.2027` is laid out right-to-left as a whole:
  the `- ` marker lands at the row's right edge and the date at its left. That is the
  correct bidi result, and it is Claude's, not ours.
- **There is no switch.** `isNeeded()` caches `true` and nothing in the bundle ever
  assigns `false` — no env var, no setting, no terminal capability query. Do not go
  looking for one again without re-checking the bundle first.

So reordering it a second time is what put it back into logical order on screen, which
is what a reader sees as backwards. `reorders_bidi` in the `agents.rs` registry marks
these agents and `Terminal.svelte` never latches `rightToLeftSeen` for them — the
overlay is not merely hidden, it never exists. The cost is Arabic *shaping* in Claude
sessions: the letters are in the right order but unjoined, because un-reversing them to
shape and re-reversing to place is a bidi implementation of our own, which is exactly
what this design refuses to own. Every other agent, and any shell, still gets the
overlay.

Cost when a session has no such text: one `translateToString` and one regex per visible
row — and not even that until the session has printed a right-to-left character once
(`rightToLeftSeen` latches off the PTY stream, so an English-only session never scans).

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
5. **Expecting the DECSET 2031 `?997` relay to re-theme a running Claude.** It
   does not, on Windows, and the code reads as though it does. Claude's theme is
   `live override ?? $COLORFGBG ?? dark`; the override is set *only* by the answer
   to an OSC 11 background-color query, and its `?997` handler **throws away the
   scheme the report carries** and re-probes instead — the report is a doorbell,
   not the news. Measured against the real binary under a real `ConPTY` (harness:
   `portable-pty`, answer XTVERSION/kitty/DA1/DA2/CPR, accept the trust gate, wait
   for `?1049h`, then send `CSI ?997;2n`): Claude subscribes with `?2031h`, emits
   **OSC 0 and nothing else**, and rang-doorbell produced **zero** OSC 11 queries
   in 30s. So there is nothing for a terminal-side OSC 11 handler to answer —
   writing one is dead code here (tried, measured, reverted). Re-measured on
   2.1.227 with the agent's own `--debug` log as the witness: the doorbell *does*
   arrive (a query attempt is logged within a second of it) and every attempt ends
   `OSC 11 query … got no response`. An `auto` session's scheme is therefore
   decided by the `$COLORFGBG` it was spawned with and cannot change while it
   runs — which is why ADE no longer leaves Claude on `auto` at all. **The live
   lever is a theme file, not the protocol** (see below).

   **Nor does pushing the answer it never asked for.** The obvious next idea —
   the query is unwinnable, but the *answer* is just a matched report, so ring
   the doorbell and hand over `OSC 11;rgb:…` in the same write — is also dead,
   measured against 2.1.220 (harness: `portable-pty`, one fresh session per
   variant, answering DA1 the way a real terminal does; all three deliveries —
   one write, two writes, answer alone — repainted **nothing**). The formats are
   right (its parser is `/^\x1b\[\?997;([12])n$/` and
   `/^\x1b\](\d+);(.*?)(?:\x07|\x1b\\)$/`), and the queued query really would
   match. What kills it is the **fence**: the probe is
   `Promise.all([send(query), flush()])`, `flush()` writes a DA1 (`ESC [ c`)
   sentinel, and any DA1 answer resolves every query queued before it with
   *undefined*. On Windows the pseudoconsole answers DA1 itself, locally and
   instantly, so the give-up beats anything the host can inject. A host that
   holds the fence open cannot exist — conhost owns that reply. Do not re-try
   this without first re-measuring whether ConPTY still swallows the query.

   **The fence has one gap, and it is still not enough.** With `TMUX`/`STY` in the
   environment the probe takes a different shape — `Promise.race([send(query),
   2s timeout])`, DCS-wrapped, no DA1 sentinel — so an answer inside that window
   would win. Measured on 2.1.227 with `STY` set: the query still never reaches
   the PTY master, and an answer written blind into the window changes nothing,
   because ConPTY drops OSC on the way *in* as well. CSI does get through (the
   `?997` doorbell proves it); OSC does not, in either direction.

   **What DOES re-theme a running session: the theme definition it is using.**
   Claude watches its user theme directory and re-renders when a definition in use
   changes, so ADE owns one (`~/.claude/themes/pade.json`), selects it per session
   with `--settings '{"theme":"custom:pade"}'` — never by writing a settings file
   of the user's — and rewrites it on every scheme flip. Measured on 2.1.227: an
   idle session repainted from the dark palette to the light one **unprompted,
   within 5s**, matching a light-spawned control exactly, with the conversation
   untouched. That is `ThemeConfig::SpawnSelectedLiveTheme` and
   `theming::publish_live_themes`. Note what carries the scheme: the file's
   *contents*. The launch args only name it, so they are identical on both
   schemes — that split is the whole reason a flip can reach a process that is
   already running.
6. **Putting Claude *into* the flip-restart because a harness says the
   `--session-id` race is gone.** It isn't, where it counts. Codex and opencode
   are respawned onto real resume args (`codex resume <uuid>`,
   `opencode --session <id>`); Claude has none, so it is respawned onto the same
   `--session-id` the process just killed was pinned to. A `portable-pty` harness
   — start, kill, respawn on that id at 0ms / 250ms / 1500ms — reports all three
   surviving to their TUI, and on the strength of that Claude was added to
   `SPAWN_THEMED_AGENTS`. In the live app the very next flip left a **blank pane
   stuck at "Starting…"**: the session wedged instead of re-theming. Twice now
   (2026-08-01, 2026-08-07) this has been tried and reverted. The harness is
   measuring a narrower thing than the app does — believe the app. Wedging a
   working session is worse than colours that lag. **Settled for good since
   2026-08-13:** Claude follows a flip through its watched theme definition, so
   there is no longer anything to gain by restarting it. `SPAWN_THEMED_AGENTS` is
   Codex and opencode, and Claude must never be added to it.

   Closed since: a session that is **busy** when the scheme flips can't be
   respawned then — that would sever the turn in flight — so App holds the flip
   and applies it at the session's next idle prompt (`whenSessionIdle` with no
   timeout, since there is nothing to give up for). `sessionSpawnScheme` records
   the scheme each session actually launched under, so a session that sat out a
   flip *and its way back* is never restarted for nothing. Until that idle
   moment the agent keeps painting its spawn scheme. For Claude, which is not in
   the set at all, that is the whole story: dark diffs on a light terminal until
   it is next launched, and nothing on Windows can close that gap safely.

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
- Prefer scanning xterm's buffer for a line and converting its row index to a y;
  this remains stable across renderer implementation details.
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
