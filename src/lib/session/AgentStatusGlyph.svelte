<script lang="ts">
  // An agent session's glyph, the one look the tab strip and the left rail
  // share: the agent's brand mark, tinted by how full its context window is
  // (green→amber→red toward the auto-handoff threshold) and carrying status -
  // a working agent breathes, a ready one gets a soft halo.

  import { agentIconName } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import { contextTooltip } from "@/lib/session/tabs/context-tooltip";
  import { effective } from "@/lib/settings/prefs.svelte";
  import { contextPercentage, measuredContextPercentage } from "@/lib/stores/agent/context.svelte";
  import type { SessionStatus } from "@/lib/types";
  import { tooltip } from "@/lib/ui/tooltip/truncation-tooltip";
  import { ContextLevel, contextLevel } from "@/lib/usage/context-level";

  const { agentId, sessionId, status, isAwaitingChoice = false }: {
    agentId: string;
    /** The session whose context fill tints the mark, or null for one this
     *  window can't measure (it runs in another window): the mark stays
     *  neutral rather than claim a reading. */
    sessionId: string | null;
    status: SessionStatus;
    /** The agent waits on a multiple-choice answer - flash the glyph red. */
    isAwaitingChoice?: boolean;
  } = $props();

  const percentage = $derived(sessionId === null ? null : contextPercentage(sessionId));
  const level = $derived.by(() => {
    if (percentage === null) {
      return null;
    }

    return contextLevel({
      percentage,
      threshold: effective.handoffPercentage
    });
  });

  // Top-layer, so the bubble works inside the rail's scroller as in the strip.
  function contextTooltipAttachment(element: HTMLElement) {
    if (sessionId === null) {
      return;
    }

    return tooltip(() => contextTooltip({
      measured: measuredContextPercentage(sessionId),
      estimate: percentage
    }))(element);
  }
</script>

<span
  class="agent-glyph {status}"
  class:awaiting-choice={isAwaitingChoice}
  class:critical={level === ContextLevel.Critical}
  class:unknown={percentage === null}
  class:warning={level === ContextLevel.Warning}
  {@attach contextTooltipAttachment}
><Icon name={agentIconName(agentId)} size={14} /></span>

<style>
  /* Coloured by how full the context window is (the --context-* gauge). */
  .agent-glyph {
    display: inline-flex;
    flex: none;
    border-radius: var(--radius-full);
    color: var(--context-ok);
    transition: color 300ms var(--ease);

    &.warning {
      color: var(--context-warning);
    }

    &.critical {
      color: var(--context-critical);
    }

    /* No signal yet (a just-launched agent, or one another window runs) -
       stay neutral so the blue never reads as a real "plenty of room"
       measurement. */
    &.unknown {
      color: var(--on-surface-variant);
    }

    /* Working breathes; ready (idle, awaiting you) keeps a soft halo. The
       pulse dims the inner mark, never this host span, so a tooltip anchored
       to it isn't dragged through every breath. */
    &.working > :global(svg) {
      animation: pulse 1100ms var(--ease) infinite;
    }

    &.ready {
      box-shadow: 0 0 0 3px var(--tertiary-wash);
    }

    /* Under Cyberpunk a spread ring follows the chamfer and turns the halo
       into a hard frame round a 12px glyph, a stray box rather than a state.
       Night lights the mark instead (a blur has no edge for the chamfer to
       find); day has no glow to spend, so it backs the mark with a tinted
       plate - a badge rather than a frame. */
    :global(:root[data-palette="cyberpunk"][data-theme="dark"]) &.ready {
      box-shadow: 0 0 8px var(--tertiary);
    }

    :global(:root[data-palette="cyberpunk"][data-theme="light"]) &.ready {
      background: var(--tertiary-wash);
      box-shadow: none;
    }

    &.exited {
      color: var(--critical);
    }

    /* Waiting on a multiple-choice answer - a red ring pulses out to grab the
       eye (choice-flash lives in SessionTabs). Reduced motion swaps the pulse
       for a steady red ring. */
    &.awaiting-choice {
      color: var(--critical);
      animation: choice-flash 1100ms var(--ease) infinite;

      @media (prefers-reduced-motion: reduce) {
        box-shadow: 0 0 0 2px var(--critical-wash);
        animation: none;
      }
    }
  }
</style>
