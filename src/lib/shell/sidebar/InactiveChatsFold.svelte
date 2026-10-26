<script lang="ts" module>
  import { SvelteSet } from "svelte/reactivity";

  // The projects whose inactive conversations are folded away. Shared by every
  // fold in the window, so a project moving between Pinned and Recent (which
  // remounts it) keeps its fold.
  const foldedProjects = new SvelteSet<string>();
</script>

<script lang="ts">
  import { formatCount } from "@/lib/util/format";
  import type { Snippet } from "svelte";

  // A project's inactive conversations (stopped chats and recorded ones) under
  // a fold that opens and closes with a glide. Open until the user folds it,
  // and hidden while there are none.
  const { project, count, hasLiveChats, children }: {
    project: string;
    count: number;
    /** The project runs live chats above the fold - its sessions are then
     *  the "other" ones, else simply the past ones. */
    hasLiveChats: boolean;
    children: Snippet;
  } = $props();

  const OTHER_SESSIONS = "Other sessions";
  const PAST_SESSIONS = "Past sessions";
  const heading = $derived.by(() => {
    if (hasLiveChats) {
      return OTHER_SESSIONS;
    }

    return PAST_SESSIONS;
  });
</script>

<!-- The summary is a keyboard line of its own, so the arrows open and close
     the fold and skip its rows while it's closed (sidebar-keyboard). -->
<details
  class="fold"
  hidden={count === 0}
  ontoggle={e => {
    if (e.currentTarget.open) {
      foldedProjects.delete(project);
      return;
    }

    foldedProjects.add(project);
  }}
  open={!foldedProjects.has(project)}
>
  <summary data-sidebar-item data-sidebar-line tabindex="-1">
    <span class="caret">▾</span>
    {heading}
    <span class="count">{formatCount(count)}</span>
  </summary>
  {@render children()}
</details>

<style>
  /* The fold grows its content to its natural height and back - the browser
     animates `auto` itself, no measuring. */
  .fold {
    interpolate-size: allow-keywords;

    &::details-content {
      overflow: clip;
      block-size: 0;
      transition: block-size 200ms var(--ease), content-visibility 200ms var(--ease) allow-discrete;

      @media (prefers-reduced-motion: reduce) {
        transition: none;
      }
    }

    &[open]::details-content {
      block-size: auto;
    }
  }

  summary {
    display: flex;
    gap: 6px;
    align-items: center;
    margin-inline-start: 18px;
    padding-block: 4px;
    padding-inline: 8px;
    border-radius: var(--radius-small);
    color: var(--on-surface-variant);
    list-style: none;
    font-weight: 600;
    font-size: 0.6875rem;
    cursor: pointer;
    transition: background 120ms var(--ease), color 120ms var(--ease);

    @media (prefers-reduced-motion: reduce) {
      transition: none;
    }

    &::marker {
      content: none;
    }

    &:hover {
      background: var(--surface-2);
      color: var(--on-surface);
    }
  }

  .caret {
    transition: rotate 200ms var(--ease);
    rotate: -90deg;

    @media (prefers-reduced-motion: reduce) {
      transition: none;
    }

    [open] > summary > & {
      rotate: 0deg;
    }
  }

  .count {
    padding-inline: 6px;
    border-radius: var(--radius-full);
    background: var(--surface-3);
    font-size: 0.625rem;
  }
</style>
