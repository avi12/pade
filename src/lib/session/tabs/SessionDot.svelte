<script lang="ts">
  // Per-session status dot - mirrors the SessionBadge states. Marks only the
  // collapsed tiers: the strip's overflow dots and the "+N" menu's rows.

  import { sessionStatus } from "@/lib/stores/session/sessions.svelte";

  const { sessionId, isAwaitingChoice }: {
    sessionId: string;
    /** The agent waits on a multiple-choice answer - flash the dot red. */
    isAwaitingChoice: boolean;
  } = $props();
</script>

<span class="dot {sessionStatus(sessionId)}" class:awaiting-choice={isAwaitingChoice}></span>

<style>
  /* Under `.tabs` so the rule keeps the specificity it had in the strip. */
  :global(.tabs) {
    .dot {
      flex: none;
      block-size: 8px;
      inline-size: 8px;
      border-radius: var(--radius-full);
      background: var(--on-surface-variant);

      &.working {
        background: var(--primary);
        animation: pulse 1100ms var(--ease) infinite;
      }

      &.ready {
        background: var(--tertiary);
        box-shadow: 0 0 0 4px var(--tertiary-wash);
      }

      /* Same red attention pulse as the full pill's glyph, on the collapsed dot. */
      &.awaiting-choice {
        background: var(--critical);
        animation: choice-flash 1100ms var(--ease) infinite;

        @media (prefers-reduced-motion: reduce) {
          box-shadow: 0 0 0 2px var(--critical-wash);
          animation: none;
        }
      }
    }
  }
</style>
