<!--
  Draws right-to-left text over the cells xterm painted it into.

  Read `lib/terminal-rtl` first — it owns both the decision of WHICH columns are
  right-to-left and the reading of them out of xterm's buffer. This component
  owns only the drawing: it hands each run to the browser as a single string and
  lets the browser's own bidi algorithm and OpenType shaping put the letters in
  order and join them.

  It is a pure overlay. The buffer, the selection, the copied text, the link
  provider and every output parser still see the agent's logical order — so
  nothing downstream can be confused by what is drawn here.
-->
<script lang="ts">
  import type { PlacedRtlRun } from "@/lib/terminal-rtl";
  import { RtlPartKind, rtlViewport } from "@/lib/terminal-rtl";
  import type { Terminal } from "@xterm/xterm";

  const {
    terminal,
    enabled = false,
    focused = false
  }: {
    /** Undefined until the terminal has been built and attached — the pane is
        mounted well before that, inside an async onMount. */
    terminal: Terminal | undefined;
    /** This session has actually printed right-to-left text. Until it has, the
        scan below never runs — an English-only session pays nothing at all. */
    enabled?: boolean;
    /** The keyboard is in this pane, so its caret is the live one. */
    focused?: boolean;
  } = $props();

  let runs = $state<PlacedRtlRun[]>([]);
  let cellWidth = $state(0);
  let cellHeight = $state(0);
  // Read back off the terminal rather than from the font preference, so the
  // overlay's metrics are by definition the ones xterm is laying out with.
  let fontFamily = $state<string | undefined>();
  let fontSize = $state<number | undefined>();

  function scanViewport(attached: Terminal) {
    const cell = attached.dimensions?.css.cell;
    if (!cell?.width || !cell.height) {
      return;
    }

    cellWidth = cell.width;
    cellHeight = cell.height;
    fontFamily = attached.options.fontFamily;
    fontSize = attached.options.fontSize;
    runs = rtlViewport({
      terminal: attached,
      focused
    });
  }

  // xterm repainting, the viewport scrolling, the cursor moving, the grid
  // resizing — all of it arrives as a render, which is exactly when what is on
  // screen has changed. Selection is the one thing that moves without one.
  $effect(() => {
    const attached = terminal;
    if (!enabled || !attached) {
      runs = [];
      return;
    }

    scanViewport(attached);
    const render = attached.onRender(() => scanViewport(attached));
    const selection = attached.onSelectionChange(() => scanViewport(attached));
    return () => {
      render.dispose();
      selection.dispose();
    };
  });
</script>

<!-- Decorative by construction: every character drawn here is already in xterm's
     buffer and its own accessibility tree, in the agent's logical order. -->
<div
  style:--cell-height="{cellHeight}px"
  style:--terminal-font-family={fontFamily}
  style:--terminal-font-size="{fontSize}px"
  class="rtl-overlay"
  aria-hidden="true"
>
  {#each runs as run (run.key)}
    <div
      style:--run-top="{run.row * cellHeight}px"
      style:--run-start="{run.startColumn * cellWidth}px"
      style:--run-width="{run.columns * cellWidth}px"
      style:--run-background={run.background}
      class="rtl-run"
    >
      {#each run.parts as part, index (index)}
        {#if part.kind === RtlPartKind.Caret}
          <span class="rtl-caret"></span>
        {:else}
          <span
            style:--part-color={part.foreground}
            style:--part-background={part.background}
            class="rtl-text"
            class:bold={part.bold}
            class:dim={part.dim}
            class:italic={part.italic}
            class:selected={part.selected}
            class:strikethrough={part.strikethrough}
            class:underline={part.underline}>{part.text}</span
          >
        {/if}
      {/each}
    </div>
  {/each}
</div>

<style>
  /* Sits over xterm's rows, never in front of the pointer: selection, links and
     clicks all still reach the terminal underneath. */
  .rtl-overlay {
    position: absolute;
    inset: 0;
    z-index: 2;
    font-family: var(--terminal-font-family), var(--font-rtl-fallback);
    font-size: var(--terminal-font-size);
    line-height: var(--cell-height);
    pointer-events: none;
  }

  /* One run, anchored to exactly the cells it covers.
     `direction: ltr` is the TERMINAL's own direction, not a claim about the text:
     the browser reads the run's characters and reorders them itself, and giving it
     the same base direction the grid has is what makes the result identical to the
     rest of the row — the run starts at its first column and reads away from
     there, instead of hanging off its last one with a hole in front of it.
     `isolate` keeps it from reordering anything around it. The letter-spacing
     xterm uses to hold every glyph to one cell would pull the joins apart, so this
     text is spaced normally and covers the cells instead. Physical `top`/`left` on
     purpose: a cell grid has no writing mode — column 0 is the leftmost cell
     whatever is printed in it. */
  .rtl-run {
    position: absolute;
    top: var(--run-top);
    left: var(--run-start);
    block-size: var(--cell-height);
    inline-size: var(--run-width);
    background: var(--run-background);
    letter-spacing: normal;
    text-align: start;
    white-space: pre;
    unicode-bidi: isolate;
    direction: ltr;

    .rtl-text {
      background: var(--part-background);
      color: var(--part-color);
    }

    .bold {
      font-weight: 700;
    }

    .dim {
      opacity: 50%;
    }

    .italic {
      font-style: italic;
    }

    .underline {
      text-decoration: underline;
    }

    .strikethrough {
      text-decoration: line-through;
    }

    .underline.strikethrough {
      text-decoration: underline line-through;
    }

    /* Painted over the text rather than behind it, so a selection reads the same
       whatever colours the agent chose — and costs only the one split the
       highlight's own edges force. */
    .selected {
      background: var(--terminal-selection);
    }
  }

  /* A marker between parts, not a measured position: the browser lays it out
     with the text, so it lands where the character it precedes actually ended up
     once the line was reordered. */
  .rtl-caret {
    display: inline-block;
    vertical-align: top;
    block-size: var(--cell-height);
    inline-size: 0;
    border-inline-start: 2px solid var(--primary);
    animation: terminal-caret-blink 1000ms step-end infinite;
  }

  @keyframes terminal-caret-blink {
    50% {
      opacity: 0%;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .rtl-caret {
      animation: none;
    }
  }
</style>
