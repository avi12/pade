<script lang="ts">
  import type { PacedWindowKind } from "@/lib/types";
  import { PACED_WINDOW_TEXT } from "@/lib/usage/pacing/pacing-policy";

  // A paced window's heading: its name and how it is judged, and - in the
  // workspace scope - a tag while it follows the global settings, or the way
  // back to them once it doesn't.
  const { kind, inherit }: {
    kind: PacedWindowKind;
    inherit?: {
      isFollowing: boolean;
      onreset: () => Promise<void>;
    };
  } = $props();

  const text = $derived(PACED_WINDOW_TEXT[kind]);
</script>

<header class="field-row">
  <span class="field-text">
    <span class="title">{text.title}</span>
    <span class="field-hint">{text.hint}</span>
  </span>
  {#if inherit?.isFollowing}
    <span class="scope-tag">Global</span>
  {:else if inherit}
    <button class="reset" onclick={async () => await inherit.onreset()} type="button">Use global</button>
  {/if}
</header>

<style>
  .title {
    color: var(--on-surface);
    font-weight: 700;
    font-size: 0.6875rem;
    letter-spacing: 0.07em;
    text-transform: uppercase;
  }

  .scope-tag,
  .reset {
    flex: none;
    font-family: var(--font-monospace);
    font-size: 0.625rem;
  }

  .scope-tag {
    padding-block: 1px;
    padding-inline: 6px;
    border: 1px solid var(--outline);
    color: var(--on-surface-variant);
  }

  .reset {
    padding: 0;
    border: 0;
    background: none;
    color: var(--primary);
    text-decoration: underline;
    cursor: pointer;
  }
</style>
