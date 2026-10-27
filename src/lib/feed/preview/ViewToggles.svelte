<script lang="ts">
  import { PreviewPane } from "@/lib/feed/preview/feed-previews";
  import type { FeedPreviews, PreviewCard } from "@/lib/feed/preview/feed-previews";
  import RadioPills from "@/lib/feed/preview/RadioPills.svelte";
  import { DiffStyle } from "@/lib/types";
  import { prefersReducedMotion } from "@/lib/ui/motion";
  import { tick } from "svelte";
  import type { z } from "zod";

  // An open card's view choices, stacked - Code|Preview above Unified|Split -
  // so a previewable diff reads them top-to-bottom, right-aligned.
  let { card, previews, canPreview, canSwitchDiffMode, diffMode = $bindable() }: {
    card: PreviewCard;
    previews: FeedPreviews;
    /** A markdown/HTML card: offer its rendered Preview beside the code. */
    canPreview: boolean;
    /** A text diff is showing: offer its Unified|Split layout. */
    canSwitchDiffMode: boolean;
    /** The view-wide Unified|Split choice, kept across cards. */
    diffMode: z.infer<typeof DiffStyle>;
  } = $props();

  const PANE_OPTIONS = [
    {
      value: PreviewPane.Code,
      label: "Code"
    },
    {
      value: PreviewPane.Preview,
      label: "Preview"
    }
  ] as const;
  const DIFF_OPTIONS = [
    {
      value: DiffStyle.enum.unified,
      label: "Unified"
    },
    {
      value: DiffStyle.enum.split,
      label: "Split"
    }
  ] as const;
</script>

<div class="pane-toggles">
  {#if canPreview}
    <!-- The rendered document is fetched on the first switch to Preview. -->
    <RadioPills
      name="pane-{card.id}"
      chosen={previews.paneOf(card.id)}
      legend="Preview mode"
      onchoose={async value => {
        previews.paneById.set(card.id, value);

        if (value === PreviewPane.Preview) {
          await previews.loadPreviewDocument(card);
        }
      }}
      options={PANE_OPTIONS}
    />
  {/if}
  {#if canSwitchDiffMode}
    <!-- The swap runs through a View Transition so unified↔split cross-fades
         in place rather than snapping; a plain swap when the API is absent or
         motion is reduced. `tick()` lets Svelte apply the new layout inside the
         callback so the API captures the "after" state. -->
    <RadioPills
      name="diff-{card.id}"
      chosen={diffMode}
      legend="Diff view"
      onchoose={value => {
        if (value === diffMode) {
          return;
        }

        if (prefersReducedMotion() || !document.startViewTransition) {
          diffMode = value;
          return;
        }

        document.startViewTransition(async () => {
          diffMode = value;
          await tick();
        });
      }}
      options={DIFF_OPTIONS}
    />
  {/if}
</div>

<style>
  .pane-toggles {
    display: flex;
    flex-shrink: 0;
    flex-direction: column;
    gap: 4px;
    align-items: flex-end;
  }
</style>
