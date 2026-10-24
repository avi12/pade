<script lang="ts">
  import type { ChangeEntry } from "@/lib/feed/change-groups";
  import { revealBlock } from "@/lib/ui/motion";
  import { truncationTooltip } from "@/lib/ui/tooltip/truncation-tooltip";
  import { formatAge, formatCount, formatTimestamp } from "@/lib/util/format";
  import { baseName, parentDirectory, relativeToRoot } from "@/lib/util/paths";
  import FileTypeBadge from "@/panels/feed/card/FileTypeBadge.svelte";
  import LineCounts from "@/panels/feed/card/LineCounts.svelte";
  import type { Snippet } from "svelte";

  // One feed card: the whole summary is a toggle button, and the open card
  // reveals its viewer (`children`) below it.
  const { entry, isOpen, now, project, onToggle, children }: {
    entry: ChangeEntry;
    isOpen: boolean;
    /** The feed's ticking clock, so the age label moves forward on its own. */
    now: number;
    project: string;
    onToggle: () => void;
    children: Snippet;
  } = $props();

  const event = $derived(entry.event);
</script>

<li class="card {event.kind}" class:open={isOpen}>
  <span class="stripe" aria-hidden="true"></span>
  <button class="body" aria-expanded={isOpen} onclick={onToggle}>
    <span class="row">
      <FileTypeBadge path={event.path} />
      <span class="name" data-tooltip={event.path}>{baseName(event.path)}</span>
      <span class="time" data-tooltip={formatTimestamp(event.ts)}>{formatAge({
        stamp: event.ts,
        now
      })} ago</span>
    </span>
    <span class="summary">
      {event.summary}
      {#if entry.repeats > 1}
        <span
          class="repeats"
          data-tooltip="{formatCount(entry.repeats)} changes to this file in a row"
        >×{formatCount(entry.repeats)}</span>
      {/if}
    </span>
    <span class="meta">
      <!-- Root-anchored relative to the open project ("/backend/convex"), "/"
           for a file at the root. It ellipsizes in the narrow panel, so it grows
           a bubble while clipped - with the FULL absolute path, since what the
           clipped relative dir hides is exactly what the reader couldn't see. -->
      <span class="path" {@attach truncationTooltip({ tooltip: event.path })}>{relativeToRoot({
        path: parentDirectory(event.path) ?? event.path,
        root: project
      })}</span>
      <span class="statistics">
        <LineCounts added={entry.added} removed={entry.removed} />
      </span>
    </span>
  </button>

  {#if isOpen}
    <div class="diff" transition:revealBlock>
      {@render children()}
    </div>
  {/if}
</li>

<style>
  .card {
    contain-intrinsic-block-size: auto 86px;
    position: relative;

    /* Offscreen cards skip layout and paint entirely, so a full feed (the
       300-event cap) scrolls without jank; the placeholder size keeps the
       scrollbar honest until a card has rendered once and remembers its own. */
    content-visibility: auto;

    /* Never let the scroller's flex column squash a card to fit - an
       overflowing feed scrolls; cards keep their natural height. */
    flex-shrink: 0;
    overflow: hidden;

    /* Border reserves its space always so the layout doesn't shift when a card
       opens; it's transparent when idle and lights to primary while expanded. */
    border: 1px solid transparent;
    border-radius: var(--radius-medium);
    background: var(--surface-1);
    transition:
      background 140ms var(--ease),
      border-color 140ms var(--ease);
    animation: pop-in 260ms var(--spring);

    &:hover {
      background: var(--surface-2);
    }

    &.open {
      border-color: var(--primary);
      background: var(--surface-2);
    }

    /* Accent stripe hugging the rounded left edge, tinted by change kind. */
    .stripe {
      position: absolute;
      inset-block: 0;
      inset-inline-start: 0;
      inline-size: 3px;
      background: var(--outline);
    }

    &.created .stripe {
      background: var(--tertiary);
    }

    &.modified .stripe {
      background: var(--primary);
    }

    &.deleted .stripe {
      background: var(--critical);
    }
  }

  /* The whole card content is one big toggle button; keep it text-aligned like a
     block, not a centred control. */
  .body {
    display: block;
    inline-size: 100%;
    padding-block: 11px;
    padding-inline: 15px 13px;
    text-align: start;
    cursor: pointer;
  }

  .row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .name {
    overflow: hidden;
    font-family: var(--font-monospace);
    font-weight: 600;
    font-size: 0.8125rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .time {
    margin-left: auto;
    color: var(--on-surface-variant);
    font-size: 0.6875rem;
  }

  .summary {
    display: flex;
    gap: 6px;
    align-items: baseline;
    margin-block: 5px 0;
    margin-inline: 0;
    color: var(--on-surface);
    font-size: 0.8125rem;
  }

  /* How many changes in a row this one card stands for. Quiet by default - it
     is a count, not a state. */
  .repeats {
    padding-block: 1px;
    padding-inline: 6px;
    border-radius: var(--radius-full);
    background: var(--surface-3);
    color: var(--on-surface-variant);
    font-size: 0.6875rem;
  }

  .meta {
    display: flex;
    gap: 10px;
    align-items: center;
    margin-block-start: 6px;
    color: var(--on-surface-variant);
    font-size: 0.6875rem;
  }

  .path {
    overflow: hidden;
    font-family: var(--font-monospace);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .statistics {
    display: flex;
    gap: 8px;
    margin-inline-start: auto;
    font-weight: 600;
  }

  /* The open card's viewer, inset under the summary. */
  .diff {
    overflow: hidden;
    margin-block: 0 11px;
    margin-inline: 15px 13px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
  }
</style>
