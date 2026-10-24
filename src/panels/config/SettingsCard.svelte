<script lang="ts">
  import SectionLabel from "@/panels/config/SectionLabel.svelte";
  import type { Snippet } from "svelte";

  // One settings section of the Config panel: a titled card. It also owns the
  // field vocabulary its sections write in (`.field`, `.field-row`,
  // `.field-text`, `.field-label`, `.field-hint`, `.check`), so every card reads
  // alike without each section restating it.
  const { title, isCompact = false, children }: {
    title: string;
    /** A few short rows that read as one block, tighter than the card rhythm. */
    isCompact?: boolean;
    children: Snippet;
  } = $props();
</script>

<section class="card" class:compact={isCompact}>
  <SectionLabel text={title} />
  {@render children()}
</section>

<style>
  /* Colours and corners come from the theme's tokens, so each theme reshapes all
     the cards at once. The cards answer their own layout questions: the side
     panel is drag-resizable, so anything that reflows on width asks the card
     (@container), never the viewport. */
  .card {
    container-type: inline-size;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 14px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);

    &.compact {
      gap: 10px;
    }
  }

  /* The field vocabulary reaches into the sections at zero specificity
     (`:where`), so a section that styles one of these itself still wins. */
  :where(.card) :global(.field) {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  :where(.card) :global(.field-row) {
    display: flex;
    gap: 12px;
    justify-content: space-between;
    align-items: center;
  }

  :where(.card) :global(.field-text) {
    display: flex;
    flex-direction: column;
    gap: 1px;
    line-height: 1.3;
  }

  :where(.card) :global(.field-label) {
    color: var(--on-surface);
    font-weight: 600;
    font-size: 0.75rem;
  }

  :where(.card) :global(.field-hint) {
    color: var(--on-surface-variant);
    font-size: 0.625rem;
  }

  /* A row's one action (Restart PADE, Check now): a filled pill. */
  :where(.card) :global(.field-action) {
    flex: none;
    padding-block: 7px;
    padding-inline: 12px;
    border: none;
    border-radius: var(--radius-full);
    background: var(--primary-container);
    color: var(--on-primary-container);
    font: inherit;
    font-weight: 700;
    font-size: 0.6875rem;
    cursor: pointer;
    transition: filter 140ms var(--ease);

    &:hover {
      filter: brightness(0.96);
    }

    &:disabled {
      cursor: default;
    }
  }

  /* A checkbox beside its label; a dependent row dims while it has no effect. */
  :where(.card) :global(.check) {
    display: flex;
    gap: 10px;
    align-items: center;
    min-inline-size: 0;
    cursor: pointer;
  }

  :where(.card) :global(.check.disabled) {
    cursor: default;
  }

  :where(.card) :global(.check.disabled .field-label) {
    color: var(--on-surface-variant);
  }
</style>
