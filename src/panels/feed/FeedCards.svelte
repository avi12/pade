<script lang="ts">
  import type { ChangeGroup } from "@/lib/feed/change-groups";
  import type { FeedPreviews } from "@/lib/feed/preview/feed-previews";
  import { isVideoPath } from "@/lib/feed/preview/preview";
  import type { DiffStyle } from "@/lib/types/theme";
  import ChangeCard from "@/panels/feed/card/ChangeCard.svelte";
  import ChangeDetail from "@/panels/feed/card/ChangeDetail.svelte";
  import GroupHeader from "@/panels/feed/GroupHeader.svelte";
  import type { z } from "zod";

  // The scrolling list of cards, grouped by project under each group's header.
  // Only one card is open at a time.
  let { groups, project, previews, now, expandedId = $bindable(), diffMode = $bindable() }: {
    groups: ChangeGroup[];
    project: string;
    previews: FeedPreviews;
    /** The feed's ticking clock, for the cards' age labels. */
    now: number;
    /** The open card's event id, `null` with every card closed. */
    expandedId: string | null;
    /** The feed-wide Unified|Split choice, kept across cards. */
    diffMode: z.infer<typeof DiffStyle>;
  } = $props();
</script>

<div class="cards scroll-fade">
  {#each groups as group (group.id)}
    <section class="group">
      <GroupHeader {group} />
      <ul class="grouplist">
        {#each group.entries as entry (entry.event.id)}
          {@const event = entry.event}
          <ChangeCard
            {entry}
            isOpen={expandedId === event.id}
            {now}
            onToggle={async () => {
              const isAlreadyOpen = expandedId === event.id;
              if (isAlreadyOpen) {
                expandedId = null;
                return;
              }

              // A video card expands at once: its clip is transcoded on demand,
              // too slow to hold the reveal for, so the card shows a
              // transcoding state until the clip lands.
              if (isVideoPath(event.path)) {
                expandedId = event.id;
                await previews.loadCardPreview(event);
                return;
              }

              // Load the preview BEFORE expanding so the reveal measures the card's
              // full height and glides straight to it. Expanding first would animate
              // to the pre-diff height, then jump when the async diff lands.
              await previews.loadCardPreview(event);
              expandedId = event.id;
            }}
            {project}
          >
            <ChangeDetail
              {event}
              onClose={() => (expandedId = null)}
              {previews}
              {project}
              bind:diffMode
            />
          </ChangeCard>
        {/each}
      </ul>
    </section>
  {/each}
</div>

<style>
  .cards {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 12px;
    overflow-y: auto;
    overscroll-behavior: contain;
    min-block-size: 0;
    margin: 0;
    padding-block: 12px 18px;
    padding-inline: 14px;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /* A group's cards hang off a thin rail under its header, so which package a
     card belongs to reads from the indent rather than a box around the group. */
  .grouplist {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-block: 0;
    margin-inline: 8px 0;
    padding-block: 0;
    padding-inline: 10px 0;
    border-inline-start: 1px solid var(--outline);
    list-style: none;
  }
</style>
