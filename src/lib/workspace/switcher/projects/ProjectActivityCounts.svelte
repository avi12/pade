<script lang="ts">
  import { formatCount } from "@/lib/util/format";

  // A project row's two counts: how many of its chats wait on you, in a
  // warning-washed box, and how many are working, a primary dot and its
  // number. Each hides at zero (kept in the layout's place, never moved).
  const { needsYouCount, workingCount }: {
    needsYouCount: number;
    workingCount: number;
  } = $props();

  const NEEDS_YOU_NOTE = "waiting on you";
  const WORKING_NOTE = "working";
</script>

<span class="needs-you-count" hidden={needsYouCount === 0}>
  {formatCount(needsYouCount)}<span class="spoken"> {NEEDS_YOU_NOTE}</span>
</span>
<span class="working-count" hidden={workingCount === 0}>
  <span class="working-dot" aria-hidden="true"></span>
  {formatCount(workingCount)}<span class="spoken"> {WORKING_NOTE}</span>
</span>

<style>
  .needs-you-count,
  .working-count {
    flex: none;
    font-weight: 800;
    font-size: 0.625rem;
    line-height: 1.35;

    &[hidden] {
      display: none;
    }
  }

  .needs-you-count {
    padding-block: 0.0625rem;
    padding-inline: 0.375rem;
    border-radius: var(--radius-small);
    background: var(--warning-wash);
    color: var(--warning);
  }

  .working-count {
    display: inline-flex;
    gap: 0.25rem;
    align-items: center;
    color: var(--primary);

    :global(:root[data-palette="cyberpunk"]) & {
      text-shadow: var(--cyber-glow);
    }
  }

  .working-dot {
    flex: none;
    block-size: 0.375rem;
    inline-size: 0.375rem;
    border-radius: 50%;
    background: var(--primary);

    :global(:root[data-palette="cyberpunk"]) & {
      border-radius: 0;
    }
  }

  /* Spoken, not shown - the colour says it to the eye. */
  .spoken {
    position: absolute;
    overflow: hidden;
    block-size: 1px;
    inline-size: 1px;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
