<script lang="ts">
  // The tab's leading glyph: the agent's brand mark, tinted by how full its
  // context window is (green→amber→red toward the auto-handoff threshold) and
  // carrying status - a working agent breathes, a ready one gets a soft halo.

  import { agentIconName } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import { contextTooltip } from "@/lib/session/tabs/context-tooltip";
  import { effective } from "@/lib/settings/prefs.svelte";
  import { contextPercentage, measuredContextPercentage } from "@/lib/stores/agent/context.svelte";
  import { sessionStatus } from "@/lib/stores/session/sessions.svelte";
  import type { AgentSession } from "@/lib/types";
  import { ContextLevel, contextLevel } from "@/lib/usage/context-level";

  const { session, isAwaitingChoice }: {
    session: AgentSession;
    /** The agent waits on a multiple-choice answer - flash the glyph red. */
    isAwaitingChoice: boolean;
  } = $props();

  const percentage = $derived(contextPercentage(session.id));
  const level = $derived.by(() => {
    if (percentage === null) {
      return null;
    }

    return contextLevel({
      percentage,
      threshold: effective.handoffPercentage
    });
  });
</script>

<span
  class="agent-icon {sessionStatus(session.id)}"
  class:awaiting-choice={isAwaitingChoice}
  class:critical={level === ContextLevel.Critical}
  class:unknown={percentage === null}
  class:warning={level === ContextLevel.Warning}
  data-tooltip={contextTooltip({
    measured: measuredContextPercentage(session.id),
    estimate: percentage
  })}
><Icon name={agentIconName(session.agent.id)} size={14} /></span>

<style>
  /* Under `.tabs` so the rule keeps the specificity it had in the strip. */
  :global(.tabs) {
    /* Leading glyph on a full tab: the agent's brand mark, coloured by how full
       its context window is (the --context-* gauge) and carrying status. Stands
       in for the plain status dot, which now marks only the collapsed tiers. */
    .agent-icon {
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

      /* No signal yet (a just-launched agent, or a non-agent terminal) - stay
         neutral so the blue never reads as a real "plenty of room" measurement. */
      &.unknown {
        color: var(--on-surface-variant);
      }

      /* Working breathes; ready (idle, awaiting you) keeps the dot's soft halo.
         The pulse dims the inner mark, never this host span: the tooltip bubble
         is this span's ::after, and an opacity animation on the host would drag
         the visible bubble through every breath with it. */
      &.working > :global(svg) {
        animation: pulse 1100ms var(--ease) infinite;
      }

      &.ready {
        box-shadow: 0 0 0 3px var(--tertiary-wash);
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
  }
</style>
