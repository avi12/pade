<script lang="ts">
  import { pty } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";
  import { appearance, effective } from "@/lib/prefs.svelte";
  import SessionBadge from "@/lib/SessionBadge.svelte";
  import { dropContext, observeContext } from "@/lib/stores/context.svelte";
  import { setSessionStatus } from "@/lib/stores/sessions.svelte";
  import { SessionStatus } from "@/lib/types";
  import type { AgentSession } from "@/lib/types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebglAddon } from "@xterm/addon-webgl";
  import { Terminal } from "@xterm/xterm";
  import { onDestroy, onMount } from "svelte";

  const { session, removable = false, onremove }: {
    session: AgentSession;
    /** Show a trailing remove-from-split button in the session bar. */
    removable?: boolean;
    onremove?: () => void;
  } = $props();

  let host: HTMLDivElement;
  let term: Terminal;
  let fit: FitAddon;
  let unlisten: UnlistenFn | undefined;
  let exitUnlisten: UnlistenFn | undefined;
  let resizeObs: ResizeObserver | undefined;
  // Guards the async onMount against a teardown that runs before its awaits
  // settle: onDestroy sets this, and each awaited step bails so no listener is
  // registered after unmount and no write hits a disposed terminal.
  let destroyed = false;

  // Session status. Output flowing = working; a quiet gap while the process is
  // alive = ready (done with its task, waiting for you); exit = done.
  let status = $state<SessionStatus>(SessionStatus.enum.starting);
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  let fitFrame: number | undefined;
  const IDLE_MS = 700;
  // A resize makes the agent repaint; output within this window after one is
  // treated as that repaint's echo, not fresh activity — so revealing a hidden
  // pane (which refits it) can't flash the badge from "ready" to "working".
  const RESIZE_SETTLE_MS = 400;
  let lastResizeAt = 0;

  // Shift+Enter should add a newline to the agent's prompt, not submit it.
  // Terminals send plain `\r` (0x0D) for both Enter and Shift+Enter, so the
  // wrapped CLI can't tell them apart and submits on either. Emit the CSI u
  // (fixterms) encoding for Shift+Enter instead — key 13 (Enter) with modifier
  // 2 (Shift) — which Claude Code decodes as "insert newline". This mirrors what
  // `claude`'s own /terminal-setup makes terminals like VS Code emit.
  // https://code.claude.com/docs/en/terminal-config
  const SHIFT_ENTER = "\x1b[13;2u";

  function markActivity() {
    if (status === SessionStatus.enum.exited) {
      return;
    }

    // Ignore the agent's own resize-repaint: it isn't the agent working, so a
    // settled "ready" session shouldn't blink to "working" when its pane is
    // revealed and refitted. Real work arrives outside the settle window.
    const isResizeEcho =
      status === SessionStatus.enum.ready && Date.now() - lastResizeAt < RESIZE_SETTLE_MS;
    if (isResizeEcho) {
      return;
    }

    status = SessionStatus.enum.working;
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => {
      if (status === SessionStatus.enum.working) {
        status = SessionStatus.enum.ready;
      }
    }, IDLE_MS);
  }

  // Publish status to the shared store so the top-bar tab shows a matching dot.
  $effect(() => {
    setSessionStatus({
      id: session.id,
      status
    });
  });

  // Live-update the terminal font when the preference changes.
  $effect(() => {
    const family = effective.monoFamily;
    if (term) {
      term.options.fontFamily = family;
      fit?.fit();
    }
  });

  // Re-theme the terminal when the app scheme flips, so Claude Code's output
  // sits on a background that matches the light/dark theme.
  $effect(() => {
    const { scheme } = appearance;
    if (term) {
      void scheme;
      term.options.theme = readXtermTheme();
    }
  });

  onMount(async () => {
    term = new Terminal({
      fontFamily: effective.monoFamily,
      fontSize: 13,
      cursorBlink: true,
      allowProposedApi: true,
      theme: readXtermTheme()
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);

    // GPU-accelerated rendering; fall back silently if WebGL is unavailable.
    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => webgl.dispose());
      term.loadAddon(webgl);
    } catch {
    /* CPU renderer is fine as a fallback */
    }

    fit.fit();

    // Stream this session's PTY output into the terminal; each chunk is a sign
    // of life that resets the idle → ready timer. Events are filtered by id so
    // sibling sessions don't cross-write.
    const dataUnlisten = await pty.onData((id, chunk) => {
      if (id !== session.id) {
        return;
      }

      // The terminal may already be disposed if a late chunk arrives during
      // teardown; skip the write rather than throw.
      if (destroyed || !term) {
        return;
      }

      term.write(chunk);
      markActivity();
      // Track how full this agent's context window is (drives auto-handoff).
      observeContext({
        id: session.id,
        chunk
      });
    });
    // If we were destroyed while awaiting, this listener registered too late
    // for onDestroy to see — tear it down now and stop.
    if (destroyed) {
      dataUnlisten();
      return;
    }

    unlisten = dataUnlisten;

    const exitListener = await pty.onExit(id => {
      if (id !== session.id) {
        return;
      }

      clearTimeout(idleTimer);
      status = SessionStatus.enum.exited;
    });
    if (destroyed) {
      exitListener();
      return;
    }

    exitUnlisten = exitListener;

    // Send keystrokes to this session's PTY.
    term.onData(data => void pty.write({
      id: session.id,
      data
    }));

    // Translate Shift+Enter into a prompt newline before xterm encodes it as a
    // plain Enter (see SHIFT_ENTER). Returning false stops xterm from also
    // sending its own `\r`, which would submit.
    term.attachCustomKeyEventHandler(event => {
      const isShiftEnter =
        event.type === "keydown" &&
          event.key === "Enter" &&
          event.shiftKey &&
          !event.altKey &&
          !event.ctrlKey &&
          !event.metaKey;
      if (!isShiftEnter) {
        return true;
      }

      // preventDefault stops the browser inserting a newline into xterm's hidden
      // textarea, which xterm's input handler would otherwise forward to the PTY
      // as a submit. Returning false additionally stops xterm sending its own
      // `\r`, so the CSI u sequence we write is the only thing the agent sees.
      event.preventDefault();
      void pty.write({
        id: session.id,
        data: SHIFT_ENTER
      });
      return false;
    });

    // Keep the PTY's window size in sync with the visible grid. Stamp the time
    // so the repaint the agent sends back isn't counted as activity (markActivity).
    term.onResize(({ cols, rows }) => {
      lastResizeAt = Date.now();
      void pty.resize({
        id: session.id,
        cols,
        rows
      });
    });

    // Refit once per animation frame so the grid reflows in lockstep with the
    // drag. A terminal reflows in whole cells, so this steps a row/column at a
    // time — that's the natural terminal behaviour, and on xterm 6.1 the reflow
    // renders synchronously (issue #4922 / PR #5529) so each step stays crisp.
    // rAF coalesces a burst of resize events into one fit per frame.
    resizeObs = new ResizeObserver(() => {
      if (fitFrame !== undefined) {
        return;
      }

      fitFrame = requestAnimationFrame(() => {
        fitFrame = undefined;
        fit.fit();
      });
    });
    resizeObs.observe(host);

    // Spawn the chosen agent in a real PTY.
    if (destroyed) {
      return;
    }

    await pty.spawn({
      id: session.id,
      command: session.agent.command,
      cwd: session.cwd ?? null,
      cols: term.cols,
      rows: term.rows,
      args: session.args
    });

    // Seed a new-project first prompt into the input (typed, not submitted —
    // the user reviews and presses Enter).
    if (destroyed) {
      return;
    }

    if (session.initialPrompt) {
      await pty.write({
        id: session.id,
        data: session.initialPrompt
      });
    }
  });

  onDestroy(() => {
    destroyed = true;
    unlisten?.();
    exitUnlisten?.();
    clearTimeout(idleTimer);

    if (fitFrame !== undefined) {
      cancelAnimationFrame(fitFrame);
    }

    resizeObs?.disconnect();
    dropContext(session.id);
    term?.dispose();
  });

  function readXtermTheme() {
    const style = getComputedStyle(document.documentElement);
    return {
      background: style.getPropertyValue("--code-background").trim(),
      foreground: style.getPropertyValue("--code-foreground").trim(),
      cursor: style.getPropertyValue("--primary").trim()
    };
  }
</script>

<div class="term-wrap">
  <header class="session-bar">
    <SessionBadge label={session.branch ? `${session.agent.label} · ${session.branch}` : session.agent.label} {status} />
    {#if removable}
      <button
        class="remove-pane"
        aria-label="Remove from split"
        data-tooltip="Remove from split"
        onclick={() => onremove?.()}
      >
        <Icon name="close" size={16} />
      </button>
    {/if}
  </header>
  <div class="term-pad">
    <div bind:this={host} class="term-host"></div>
  </div>
</div>

<style>
  .term-wrap {
    display: flex;
    flex-direction: column;
    block-size: 100%;
  }

  /* Thin session bar on surface-1 with a hairline divider; the SessionBadge
     (dot + mono label + state phrase) sits flush at the start. */
  .session-bar {
    display: flex;
    flex-shrink: 0;
    gap: 10px;
    align-items: center;
    padding-block: 8px;
    padding-inline: 14px;
    border-block-end: 1px solid var(--outline);
    background: var(--surface-1);
  }

  /* Inline remove-from-split action at the end of the bar — transparent until
     hovered, then a soft crit wash (canvas line 276). */
  .remove-pane {
    display: inline-flex;
    flex-shrink: 0;
    justify-content: center;
    align-items: center;
    block-size: 24px;
    inline-size: 24px;
    margin-inline-start: auto;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--on-surface-variant);
    cursor: pointer;
    transition: color 150ms var(--ease), background 150ms var(--ease);

    &:hover {
      background: var(--critical-wash);
      color: var(--critical);
    }
  }

  /* The xterm element must have no padding: FitAddon measures its full box to
     compute rows, so padding would fit one row too many and clip the bottom.
     Visual insets live on the wrapper instead — it lifts the output off every
     pane edge (canvas line 264: 10px top, 8px right, 8px bottom, 14px left). Any
     sub-cell remainder is filled by the shared code-bg, so it reads flush. */
  .term-pad {
    flex: 1;
    min-block-size: 0;
    padding-block: 10px 8px;
    padding-inline: 14px 8px;
    background: var(--code-background);
  }

  .term-host {
    block-size: 100%;
    inline-size: 100%;
  }
</style>
