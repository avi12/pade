<script lang="ts">
  import { pty } from "@/lib/bridge";
  import { appearance, effective } from "@/lib/prefs.svelte";
  import SessionBadge from "@/lib/SessionBadge.svelte";
  import { setSessionStatus } from "@/lib/stores/sessions.svelte";
  import { SessionStatus } from "@/lib/types";
  import type { AgentSession } from "@/lib/types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebglAddon } from "@xterm/addon-webgl";
  import { Terminal } from "@xterm/xterm";
  import { onDestroy, onMount } from "svelte";

  const { session }: { session: AgentSession } = $props();

  let host: HTMLDivElement;
  let term: Terminal;
  let fit: FitAddon;
  let unlisten: UnlistenFn | undefined;
  let exitUnlisten: UnlistenFn | undefined;
  let resizeObs: ResizeObserver | undefined;

  // Session status. Output flowing = working; a quiet gap while the process is
  // alive = ready (done with its task, waiting for you); exit = done.
  let status = $state<SessionStatus>(SessionStatus.enum.starting);
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  let fitTimer: ReturnType<typeof setTimeout> | undefined;
  const IDLE_MS = 700;

  function markActivity() {
    if (status === SessionStatus.enum.exited) {
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
    unlisten = await pty.onData((id, chunk) => {
      if (id !== session.id) {
        return;
      }

      term.write(chunk);
      markActivity();
    });
    exitUnlisten = await pty.onExit(id => {
      if (id !== session.id) {
        return;
      }

      clearTimeout(idleTimer);
      status = SessionStatus.enum.exited;
    });

    // Send keystrokes to this session's PTY.
    term.onData(data => void pty.write({
      id: session.id,
      data
    }));

    // Keep the PTY's window size in sync with the visible grid.
    term.onResize(({ cols, rows }) => void pty.resize({
      id: session.id,
      cols,
      rows
    }));

    // Debounce fit(): reflowing xterm is expensive, and a burst of resize
    // events (e.g. dragging across monitors with different DPI) would otherwise
    // reflow on every frame and stutter. Coalesce to the trailing edge.
    resizeObs = new ResizeObserver(() => {
      clearTimeout(fitTimer);
      fitTimer = setTimeout(() => fit.fit(), 80);
    });
    resizeObs.observe(host);

    // Spawn the chosen agent in a real PTY.
    await pty.spawn({
      id: session.id,
      command: session.agent.command,
      cwd: session.cwd ?? null,
      cols: term.cols,
      rows: term.rows
    });

    // Seed a new-project first prompt into the input (typed, not submitted —
    // the user reviews and presses Enter).
    if (session.initialPrompt) {
      await pty.write({
        id: session.id,
        data: session.initialPrompt
      });
    }
  });

  onDestroy(() => {
    unlisten?.();
    exitUnlisten?.();
    clearTimeout(idleTimer);
    clearTimeout(fitTimer);
    resizeObs?.disconnect();
    term?.dispose();
  });

  function readXtermTheme() {
    const style = getComputedStyle(document.documentElement);
    return {
      background: style.getPropertyValue("--code-bg").trim(),
      foreground: style.getPropertyValue("--code-fg").trim(),
      cursor: style.getPropertyValue("--primary").trim()
    };
  }
</script>

<div class="term-wrap">
  <header class="session-bar">
    <SessionBadge label={session.branch ? `${session.agent.label} · ${session.branch}` : session.agent.label} {status} />
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

  /* The xterm element must have no padding: FitAddon measures its full box to
     compute rows, so padding would fit one row too many and clip the bottom.
     Visual insets live on the wrapper instead — a small top/left inset lifts the
     output off the pane edge (canvas 10px top, 14px left). */
  .term-pad {
    flex: 1;
    min-block-size: 0;

    /* No end inset: the terminal spans the full pane width/height. Any sub-cell
       remainder is filled by the shared code-bg, so it reads flush. */
    padding-block: 10px 0;
    padding-inline: 14px 0;
    background: var(--code-bg);
  }

  .term-host {
    block-size: 100%;
    inline-size: 100%;
  }
</style>
